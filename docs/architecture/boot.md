# 启动流程、内存管理与异常处理

## 1. 启动与初始化顺序

1. **固件/模拟器加载**：QEMU 加载内核 ELF 并跳转到汇编入口 `_start`。
2. **汇编早期准备（`entry.S`）**：拦截非主核并使其进入休眠循环（`wfi` / `idle 0`）；主核设置 64 KiB 启动栈（`.bss.stack`），将固件传入的启动数据地址整理到第一个参数寄存器（`a0` / `$a0`），跳转到 `kernel_main(boot_arg)`。
3. **清零 BSS（`clear_bss`）**：使用 `write_volatile` 逐字节清零 `sbss..ebss`，避免编译器将其优化为尚不可用的外部 `memset` 调用。注意链接脚本将 `.bss.stack` 放在 `sbss` 之前，防止清零 BSS 时冲掉正在使用的启动栈。
4. **基础子系统初始化**：依次初始化日志（`utils::logging::init`）、异常入口（`trap::init`）以及内存管理（`mm::init`）。
5. **自检与停机**：打印物理页框统计与当前架构名称；若编译时开启了 `ktest` feature，则调用 `ktest::run(&frames)` 顺序执行内核自检，最后调用 `arch::shutdown()`。

---

## 2. 内存管理（`mm`）

### 2.1 静态内核堆（`mm::heap`）

`mm::init(boot_arg)` 首先将位于 BSS 段、按 4 KiB 对齐的 1 MiB 静态数组交给 `buddy_system_allocator::LockedHeap` 管理，随后才开始解析设备树和初始化页框分配器：

- **无页表依赖**：静态堆直接位于内核镜像的 BSS 段内，双架构均可直接通过内核地址访问，无需依赖物理页框分配器或动态页表。
- **使用约束**：必须在首次堆分配前且在单核关中断状态下调用一次（重复调用会直接 panic）。伙伴分配器按 2 的幂次拆分与对齐内存块，因此 1 MiB 总容量并不意味着单次可申请完整的 1 MiB 块。常规分配失败会触发 `#[alloc_error_handler]` 并通过不依赖堆的串口输出 panic 停机；需要容错的场景（如页框位图初始化）使用 `Vec::try_reserve_exact` 等可失败接口。

### 2.2 物理内存发现与区间整理（`mm::fdt` & `mm::regions`）

静态堆就绪后，`mm::init` 调用 `arch::boot::memory(boot_arg)` 提取平台原始的 RAM 与保留区间（`BootMemory`），再加上 `arch::boot::kernel_region()` 报告的内核镜像物理区间（`skernel..ekernel`），交由 `mm::regions::MemoryMap::new` 统一整理：

- **裸指针与切片解析分离**：
  - 平台启动协议及各架构早期启动代码（RISC-V 的 `satp = Bare` 物理直通模式、LoongArch 的 `DMW0` 直映窗口）必须保证 FDT 头部及声明的完整存储已映射、在借用期间持续有效可读且不被并发修改，并确保传入的物理地址与访问地址确实对应同一块物理内存。
  - 架构层的 `arch::boot_data::MappedFdt::from_raw` 集中负责 `unsafe` 的裸地址边界与头部初筛：先验证物理地址 8 字节对齐及访问地址非空无溢出，读取 40 字节 FDT 头部校验魔数、版本兼容性与总长度（`totalsize`），确认不超过平台容量上限后借用构造 `&[u8]` 切片，并记录 FDT 自身的物理区间（数值检查本身不能替代平台对底层存储可读性的保证；完整块布局与内部节点结构由后续解析步骤校验）。
  - `mm::fdt::parse(&[u8])` 是纯安全的字节切片解析器，依据 [Devicetree Specification v0.4 第 5 章](https://github.com/devicetree-org/devicetree-specification/blob/v0.4/source/chapter5-flattened-format.rst) 解析 DTB v17（及向后兼容版本）。它会校验头部声明的各数据块偏移与长度、对齐与边界及无重叠关系，提取所有启用的根级 `memory` 节点（`reg`）、内存保留表（reservation map）、`/reserved-memory` 下的静态保留区以及 `/chosen` 中的 `initrd` 范围。
  - `/reserved-memory` 节点必须显式提供空的 `ranges` 属性，且其 `#address-cells` 与 `#size-cells` 必须与根节点完全一致。若遇到缺失或非空的 `ranges`、与根节点不一致或不支持的 `#address-cells`/`#size-cells`（仅支持 1 或 2）、动态保留区（仅指定 `size` 无 `reg`）、嵌套保留区或节点深度超过 32 层，解析器会直接报错并由内核 panic 停机，防止误将用途不明的物理内存当成空闲页分配。
- **物理区间归一化（`MemoryMap`）**：
  - 分别对 RAM 区间和保留区间排序并合并重叠/相邻区间，随后从 RAM 中挖去所有保留区，并将剩余片段向内按 4 KiB 页边界对齐，生成 `usable` 可用页区间。
  - 若设备树未描述任何 RAM，或内核镜像（`skernel..ekernel`）未完整落在 RAM 区间内，内核直接拒绝启动。若扣除保留区并按页对齐后没有任何完整的可用页，`MemoryMap` 依然合法（`usable` 为空），此时页框分配器总容量为 0，后续 `alloc()` 返回 `None`。
  - 启动日志会依次打印归一化后的 `RAM`、`reserved` 和 `usable` 半开物理区间 `[start, end)`。

### 2.3 物理页框分配器（`mm::frame`）

`FrameAllocator` 使用位图管理 `MemoryMap` 移交的可用物理区间，每个 4 KiB 页框占 1 bit（置 1 表示空闲），RAM 空洞不占用位图空间：

- **常数时间分配与回收**：除了位图数组 `free` 外，分配器还维护一个记录“非零位图字下标”的索引栈 `nonempty`。每个含有空闲页的位图字在栈中恰好出现一次：分配时直接取栈顶字并用 `trailing_zeros` 定位空闲位，当该字变为全 0 时弹出栈顶；回收时仅当某个字从全 0 变为非 0 时才将其下标压回栈中。初始化时一次性在静态堆上预留全部位图与索引槽，后续 `alloc` / `drop` 均为 $O(1)$ 操作且不再触发堆分配。
- **RAII 生命周期管理**：`alloc()` 返回借用分配器的 `Frame<'_>` 句柄（不支持 `Copy`/`Clone`），句柄离开作用域时通过 `Drop` 自动归还页框。`Frame::start_address()` 返回的是**物理字节地址**而非可直接解引用的虚拟地址；分配器本身不负责建立页表映射或清零页面，使用方必须在 `Frame` 销毁前解除映射并停止访问对应物理页。

---

## 3. 双架构平台约定

### 3.1 RISC-V 64（`arch::riscv64`）

- **启动环境**：QEMU `virt` 平台 + OpenSBI，内核链接与加载基址均为 `0x80200000`。
- **启动参数**：OpenSBI 通过 `a0` 传入 hart ID、`a1` 传入 FDT 物理地址。`entry.S` 仅放行 hart 0，将 `a1` 移入 `a0` 后调用 `kernel_main`。
- **地址访问与关机**：当前处于 `satp = Bare` 物理直通模式，直接访问 `0x10000000` 的 NS16550A 串口与 FDT 物理地址。正常关机按 SBI v0.2+ 寄存器调用约定调用 SBI v0.3 引入的 System Reset（SRST）扩展（若固件不支持则回退到 `wfi` 死循环）。

### 3.2 LoongArch 64（`arch::loongarch64`）

- **启动环境**：QEMU `virt` 平台直接加载 ELF（direct boot），物理加载地址为 `0x90000000`，虚拟链接基址为 `0x9000000090000000`。
- **DMW 窗口切换**：QEMU 从物理地址进入 `_start`。`entry.S` 先配置 `DMW0`（虚拟前缀 `0x9000_0000_0000_0000`，PLV0 可缓存直映）和临时 `DMW1`（恒等映射低地址段），开启分页（`CRMD.PG = 1`）以确保开启瞬间当前 PC 仍能取指；随后跳转到 `0x9000...` 高地址空间并清除 `DMW1`。
- **EFI 启动表解析**：QEMU direct boot 在低端 `[0, 1 MiB)` 构造了命令行与合成的 EFI 系统表，并通过 `$a2` 传入系统表物理地址（`entry.S` 将其移入 `$a0`）。`arch::boot` 通过 `DMW0` 读取系统表，校验签名后按 GUID 查找 FDT 地址与 `initrd` 范围，并将前 1 MiB 启动块、FDT 本体和 `initrd` 一并加入保留区（布局参考 [QEMU 10.0.2 `hw/loongarch/boot.c`](https://github.com/qemu/qemu/blob/v10.0.2/hw/loongarch/boot.c)）。
- **RAM 布局要求**：QEMU `virt` 将 LoongArch RAM 分为低端 `[0, 256 MiB)`（`[0, 0x10000000)`）和 `0x80000000` 起的高端区间。由于内核加载在 `0x90000000`，QEMU 内存配置必须覆盖该地址（例如 `128M` 无法容纳内核，测试需使用 `768M` 或默认的 `1G`）。
- **串口与关机**：串口物理地址为 `0x1fe001e0`，通过 `DMW0`（`0x9000_0000_1fe0_01e0`）访问。目前尚未实现 LoongArch 硬件关机，结束时打印提示并通过 `idle 0` 挂起 CPU。

---

## 4. Trap 与特权级切换（`arch::trap`）

双架构对外提供一致的 `TrapContext` 与切换函数：

- `trap::init()`：安装异常入口向量，关闭全局中断以及浮点/向量扩展（RISC-V 清零 `sstatus` 的 `SIE`/`FS`/`VS`/`XS` 与 `sie`；LoongArch 清零 `CRMD.IE`、`ECFG` 与 `EUEN`），并初始化内核使用的暂存 CSR。
- `TrapContext::user(entry, stack)`：创建初始用户态上下文，设置入口 PC、用户栈指针及用户特权级状态位。
- `unsafe fn run_user(context: &mut TrapContext)`：在当前内核栈上保存被调用者保存寄存器，将 `context` 中的寄存器恢复到硬件并降权进入用户态（U-mode / PLV3）。每次进入前都会强制覆写特权状态位（不信任 `context` 中原有的特权位或扩展使能位）。当用户态发生任何异常或系统调用时，汇编将完整的用户整数现场写回同一个 `context`，恢复内核寄存器并直接返回给 `run_user` 的调用方。
  - **执行与安全边界**：调用前须已完成 `trap::init` 初始化，保持单核、关中断且无嵌套的用户态切换；`context` 及所有内核态存储必须持续有效。正常执行用户程序时，调用方需保证用户入口 PC 与栈指针按 ABI 对齐且用户代码、栈及数据内存持续可访问（LoongArch 下若故意传入未映射地址以测试异常捕获，硬件触发的异常或 TLB refill 会保存现场并安全返回内核，不会自动填表或重试）。当前机制不建立用户页表、不提供内存隔离（RISC-V `satp = Bare` 与 LoongArch 测试用 `DMW1` 均无地址空间隔离），仅可运行受控测试探针，用户代码严禁修改任何内核代码、内核栈、静态数据、分配器状态或当前 `context`。

| 维度 | RISC-V 64 (`riscv64`) | LoongArch 64 (`loongarch64`) |
| --- | --- | --- |
| **降权返回指令** | `sret`（S-mode ↔ U-mode） | `ertn`（PLV0 ↔ PLV3） |
| **捕获的异常 CSR** | `sepc`、`sstatus`、`scause`、`stval` | `ERA`、`PRMD`、`ESTAT`、`BADV`（重填时为 `TLBR*`） |
| **内核/用户 Trap 区分** | 正常内核态执行期间 `sscratch = 0`；进入用户态前在 `sret` 前的过渡窗口将内核切换帧地址写入 `sscratch`（该窗口内仍处 S-mode，要求 `TrapContext` 访存必不触发异常） | 检查 `PRMD.PPLV`（重填时检查 `TLBRPRMD.PPLV`）：若来自 PLV0 则在当前内核栈下推栈帧；若来自 PLV3 则先从 `SAVE2` 取出内核切换帧指针，再从该帧首槽读取 `TrapContext` 指针保存用户现场，随后从切换帧恢复内核态 |
| **暂存 CSR 占用** | `sscratch` | `SAVE0`、`SAVE1`（入口暂存 `$t0`/`$sp`），`SAVE2`（切换帧指针），`SAVE3`（保存内核 `$tp`） |

### 内核态 Trap 与 LoongArch TLB Refill 说明

- **内核态同步嵌套异常**：内核态发生异常时，入口汇编在当前内核栈上分配 16 字节对齐的 `TrapContext` 并在保存完毕后调用 `trap::handle_kernel`。只要栈空间充足，保存完毕后可安全处理同步嵌套异常。正常构建下所有内核态异常均会打印异常原因、PC、原始状态/原因 CSR、故障地址及 SP 等诊断信息并 panic；仅在 `ktest` 模式下由测试钩子拦截已知的断点/重填测试探针。
- **LoongArch TLB 重填入口（`TLBRENTRY`）**：硬件触发 TLB refill 时会关闭分页（进入直接地址翻译模式 `DA=1, PG=0`）并跳转到物理地址入口 `__loongarch_refill_entry`。汇编先通过或运算跳回 `0x9000...` 高地址窗口并重新置位 `CRMD.PG = 1`，随后复用通用保存逻辑：读取 `TLBRERA` 和 `TLBRBADV`，将 `TLBRPRMD.PWE` 归一化到 `PRMD` 第 3 位，并在保存的 `estat` 第 63 位打上软件重填标记（因为发生 refill 时硬件不会更新 `ESTAT.Ecode`）。目前内核仅捕获并报告 refill 异常，页表遍历与 TLB 填充待后续实现。

---

## 5. 最小用户任务与系统调用（`task` & `syscall`）

- **`Task` 状态机**：`Task` 独占一个 `TrapContext`，状态按 `Ready -> Running -> Finished(ExitStatus)` 单向流转。调用 `unsafe Task::run(&mut self)` 后，内核在同一调用栈上循环执行 `run_user`：
  1. 若用户触发的是系统调用，通过 `syscall::decode` 提取调用号与 6 个参数（RISC-V 为 `a7` + `a0..a5`，LoongArch 为 `$a7/$r11` + `$a0..$a5/$r4..$r9`），交由 `syscall::dispatch` 分发。
  2. 普通返回（`Outcome::Return(val)`）时，调用 `syscall::complete` 将返回值写入 `a0` 并将异常 PC 加 4（若 `pc + 4` 发生地址溢出则判定为 `ExitStatus::Faulted`），随后继续下一轮 `run_user`。
  3. 调用 `exit`（93）或 `exit_group`（94）时，提取首个参数的低 8 位作为退出码，任务进入 `Finished(ExitStatus::Exited(code))` 并退出循环（保持退出指令处的 PC 与 `a0` 不变）。
  4. 若触发非系统调用异常（如断点、非法指令、缺页等），任务进入 `Finished(ExitStatus::Faulted)` 并返回，不会导致内核 panic。
- **当前边界**：尚未实现用户页表、ELF 加载器、独立内核栈、多任务切换或时钟抢占。在页表实现前，`ktest` 在 RISC-V 上利用物理直通模式与 OpenSBI 默认 PMP 运行测试探针，在 LoongArch 上通过 `UserEnv` 临时开启 `DMW1`（PLV3 低地址映射）运行测试探针并在测试结束时撤销。
