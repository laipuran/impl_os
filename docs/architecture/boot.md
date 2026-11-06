# 启动流程与当前限制

## 启动与初始化顺序

1. QEMU 加载 ELF，进入 `_start`。
2. 架构汇编停放非启动 CPU，设置启动栈，并按 C ABI 将平台启动参数传给 `kernel_main(boot_arg)`。
3. Rust 用 volatile stores 清零 `sbss..ebss`，避免依赖尚未提供的 memset。
4. 内核依次初始化日志、trap 和内核堆，再解析启动信息并建立物理内存地图。
5. 内核输出架构名称；启用 `ktest` 时，`ktest::run` 按测试组列表顺序执行自检。
6. 内核调用架构关机入口。未预期的内核异常则输出保存的异常状态并 panic 停机。

链接脚本把 `.bss.stack` 放在 `sbss` 之前，不能在 Rust 已经使用启动栈后将它清零。启动栈固定 64 KiB。

### 内核堆

`mm::init()` 将 BSS 中按 4 KiB 对齐的 1 MiB 静态空间交给 `buddy_system_allocator` 管理。该空间属于内核镜像，双架构均通过当前内核地址直接访问，不依赖页框分配器或用户页表。初始化只能执行一次，且必须早于首次堆分配；重复初始化会 panic。

内核通过 `alloc` 中的 `Box`、`Vec` 等类型申请内存，由全局 `LockedHeap` 按大小和对齐拆分伙伴块，对象释放时回收并合并。二次幂取整和地址对齐会限制可满足的单次申请，1 MiB 总容量不保证能申请一个连续的 1 MiB 块；目前不支持自动扩容。普通分配失败通过不依赖堆的 panic 输出停机；需要处理耗尽的调用者可以使用 `Vec::try_reserve` 等可失败接口。

分配器的自旋锁只保护内部状态，不会关闭中断。目前仅允许关闭中断的启动 CPU 分配；启用中断或多核分配之前必须重新检查同步和重入约束。此堆不负责用户堆、设备内存或页框分配。

## 从启动信息建立物理内存地图

两架构均从启动信息读取实际 RAM 区间，扣除保留区后生成可用物理页区间。目前内核只建立这份地图，尚未实现页框分配、堆扩容或用户映射。

`mm::discover` 在静态堆初始化后调用 `arch::boot::memory`。平台模块解释启动参数，提供设备树（FDT）的访问方式和平台保留区；共享的 `mm::fdt` 与 `mm::regions` 分别负责解析和区间运算。入口汇编将平台参数放入第一个参数寄存器，因此 `kernel_main` 无需解释平台寄存器或地址转换。

平台必须保证启动数据完整、可读，且在内存发现期间不变。解析器可以校验 FDT 的结构，但不能证明物理地址可读；这一前提由启动协议保证。内核会保留 FDT 的物理存储区间，包括位于 RAM 之外的 FDT。

FDT 解析支持 v17 及声明兼容 v17 的后续版本，并校验头部、数据块边界和重叠、字符串、属性长度及节点结构。解析器收集所有启用的根级 memory 节点及其多段 `reg`、内存保留表（reservation map）、`/reserved-memory` 的静态区间，以及 `/chosen` 中的 initrd。

地址和长度支持一或两个 32 位 cell；`/reserved-memory` 必须使用与根节点相同的 cell 数，且 `ranges` 必须为空。解析器会拒绝动态保留区、嵌套保留区、不支持的寻址方式和超过 32 层的设备树。启动信息无效时，内核 panic 停机，避免将用途不明的内存交给分配器。

`arch::boot::kernel_region` 将链接符号 `skernel..ekernel` 转成物理区间：RISC-V 使用 bare 地址，LoongArch 扣除 DMW 高地址前缀。该区间覆盖内核代码、数据、启动栈和静态堆，也包含地图自身的堆对象占用；这些空间均列入保留区。

`mm::regions` 分别排序、合并 RAM 和保留区，再从 RAM 中扣除保留区，并将结果向内按 4 KiB 对齐。没有完整可用页，或内核不在已描述的 RAM 中时，内核拒绝启动。

启动日志以 `RAM`、`reserved`、`usable` 输出半开物理地址区间。地图由启动函数持有，启动数据和静态堆均不回收。

## RISC-V 启动约定

- QEMU virt + 默认 SBI 固件，ELF 链接/入口位于 `0x80200000`。
- 固件通过 `a0` 传入 hart ID、`a1` 传入 FDT 物理地址；入口只允许 hart 0 继续运行，再将 FDT 地址移到 `a0` 后进入 Rust。
- 在分页关闭状态下访问 UART `0x10000000`。
- 正常结束调用 SBI v0.2 System Reset 的 shutdown；不支持该扩展时停放 CPU。
- panic 输出诊断后停放 CPU，不调用正常关机入口。

## LoongArch 启动约定

LoongArch 目前支持比赛镜像中 QEMU virt 直接加载 ELF 的启动方式（direct boot）。启动信息解析针对 QEMU 合成的 EFI 系统表，不调用 EFI 服务；通用 UEFI 固件启动尚未支持。

ELF 虚拟起始地址为 `0x9000000090000000`，物理加载地址为 `0x90000000`。入口先建立 DMW0 高地址窗口和临时 DMW1 低地址窗口，再开启分页、跳到高地址并撤销临时窗口。这个顺序保证分页切换期间仍能取指；入口随后用 `la.local` 生成地址，设置栈并进入 Rust。

QEMU 通过 `$a2` 传入系统表的物理地址，入口将它移到 `$a0` 后传给 `kernel_main`。`arch::boot` 经 DMW0 读取系统表，校验签名和表项范围，再按 GUID 查找 FDT 和 initrd 描述。

QEMU 将命令行和配置表放在 `[0, 1 MiB)`，并为 FDT 提供 1 MiB 存储容量。内核保留整个启动块、FDT 和 initrd，FDT 地址从配置表取得。布局依据 [QEMU 10.0.2 boot.c](https://github.com/qemu/qemu/blob/v10.0.2/hw/loongarch/boot.c) 和 [virt.h](https://github.com/qemu/qemu/blob/v10.0.2/include/hw/loongarch/virt.h)。

QEMU 将 RAM 分布在低端 `[0, 0x10000000)` 和 `0x80000000` 起的高端区间，实际区间由 FDT 描述。配置的 RAM 必须覆盖整个内核物理区间；128 MiB 等小配置无法容纳位于 `0x90000000` 的内核。运行配置及 initrd 加载限制见[物理内存地图检查](../testing/README.md#物理内存地图检查)。

UART 的物理地址为 `0x1fe001e0`，内核经 DMW0 访问。关机功能尚未实现：内核输出提示后停放 CPU，因此测试结束时不会主动退出 QEMU。

## Trap 与特权级切换

`arch::trap` 在两个目标上提供相同的整数上下文接口：

- `init()`：启动 CPU 安装异常向量，关闭中断源及浮点/向量扩展，初始化保留的 scratch CSR。
- `TrapContext::user(entry, stack)`：构造用户态上下文。字段私有，内部保留架构寄存器及 CSR 名称；入口和栈的对齐要求见对应 Rust API。
- `unsafe run_user(&mut context)`：保存调用者内核 ABI 状态，恢复用户整数寄存器并降权执行；用户异常直接写回该上下文，再返回原内核调用栈。可以检查异常、修改 PC/寄存器，然后再次调用。入口强制用户权限并关闭中断/扩展，不信任传入的特权状态位。
- `registers()` / `registers_mut()`：按硬件寄存器编号读取或修改整数现场；零号寄存器在硬件恢复时仍为零。`pc()` / `set_pc()` 访问异常 PC，`sp()` 读取被打断时的栈指针。仅 PC/GPR 可通过公共接口修改，特权状态和返回路径标记由架构实现控制。
- `cause()`：通过架构的 `trap::cause::{TrapCause, InterruptCause, ExceptionCause}` 类型描述原因，不决定恢复策略；未知编号保留。`raw_cause()`、`raw_status()`、`bad_addr()` 提供架构相关的保存值，不能将两架构的状态位视为相同语义。

| 架构 | 用户态返回指令 | 普通异常状态 | 内核/用户栈区分 |
| --- | --- | --- | --- |
| RISC-V | `sret`，S → U | `sepc`、`sstatus`、`scause`、`stval` | 内核 `sscratch=0`；用户执行时指向可信内核切换帧 |
| LoongArch | `ertn`，PLV0 → PLV3 | `ERA`、`PRMD`、`ESTAT`、`BADV` | 根据保存的特权级选择当前内核栈或 SAVE2 中的切换帧；保留 SAVE0..SAVE3 |

所有整数寄存器在调用 Rust 前保存，用户栈不会用于执行内核处理代码。内核异常在当前栈上分配独立、16 字节对齐的帧，允许保存完成后的同步嵌套异常；处理结束恢复帧并返回内核。共享 `trap::handle_kernel` 默认对所有内核异常 panic；只有 `ktest` 构建中的 trap 自检识别并恢复其自身的探针。

日志初始化早于 trap 安装，因此不能依赖 trap 诊断日志器初始化期间的异常。安装后的致命内核异常通过 panic 直接输出类型化原因、PC、原始原因/状态、故障地址及原始 SP，不受日志级别过滤；普通断点不会在正式构建中自动跳过。

LoongArch 另安装物理 `TLBRENTRY`，处理 refill 时从直接地址模式回到内核 DMW。此类上下文的 `raw_cause()` 最高位为 1，其余位保留 ESTAT（其中 Ecode 不是此次 refill 的原因）；`cause()` 优先识别该标记。PC/故障地址来自 `TLBRERA`/`TLBRBADV`，`raw_status()` 将 TLBRPRMD.PWE 归一化到 PRMD bit 3，其余返回位采用 PRMD 布局，并非所有 CSR 的逐位原始转储。当前只报告 refill，不遍历页表或填充 TLB。

### 异常处理的支持范围

- 仅支持启动 CPU、关闭中断的同步执行；没有调度、时钟/外设中断处理或系统调用分发。
- 只保存整数状态；浮点、向量指令会异常，不提供相应上下文切换。
- 特权级切换不等于内存隔离。尚无用户页表、进程或 ELF 加载器；调用者负责映射及保护上下文、内核栈和内核数据。
- RISC-V 当前使用 bare 地址空间，用户访问依赖固件 PMP。LoongArch 默认 DMW0 只允许 PLV0；仅 `ktest` 中的 trap 自检临时开启 PLV3 低地址直映，并在返回统一测试入口前恢复、失效缓存翻译。该测试映射不能用于不可信程序。
- 异常入口保存完成之前的故障、内核栈溢出和不可恢复硬件异常不提供恢复保证。
