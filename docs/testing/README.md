# 内核自检与测试验证

## 1. 检查命令速览

| 命令 | 执行位置 | 覆盖范围 |
| --- | --- | --- |
| `just check` | 容器内 | 检查 `rustfmt` 格式，并对双架构的默认配置与 `ktest` 配置运行 Clippy（共 4 组） |
| `just build` / `make all` | 容器内 | 离线编译并导出 `kernel-rv`、`kernel-la` |
| `just judge-build` | 宿主机 | 在原始比赛镜像中断网、剔除隐藏文件与本地缓存，验证 `make all` 能否干净产出双架构 ELF |
| `just run rv` / `just run la` | 容器内 | 启动正常内核，观察物理内存发现日志与关机/停机行为 |
| `just test [rv\|la] [release\|debug]` | 容器内 | 编译开启 `ktest` feature 的内核并在 QEMU 中运行全部已接入的白盒自检套件 |

---

## 2. 启动期内核自检框架（`ktest`）

由于内核是禁用标准测试 harness（`test = false`）的裸机二进制，项目通过 Cargo 特性 `ktest` 提供统一的启动期自检入口（位于 [`os/src/ktest.rs`](../../os/src/ktest.rs)）：

```bash
just test            # 默认运行 RISC-V (release) 自检
just test la         # 运行 LoongArch (release) 自检
just test rv debug   # 运行 RISC-V (debug) 自检
```

### 组织方式与设计约定

- **启动入口**：`ktest` 仅在启用 `--features ktest` 时参与编译。日志、Trap 和内存分配器初始化后，启动入口调用无参数的 `ktest::run()`；需要真实页框的套件通过 `mm::frames()` 借用全局分配器。
- **目录与白盒访问**：各测试套件与专用汇编探针存放在 `os/src/ktest/` 下，按被测模块组织，再由被测模块通过 `#[cfg(feature = "ktest")] #[path = "..."] mod tests;` 挂载。这样既保持了业务源码目录的整洁，又允许测试直接访问模块内部私有项。
- **顺序执行与状态恢复**：`ktest::run` 按 `SUITES` 数组顺序依次调用各套件的无参数、无返回值入口。每个套件在返回前必须恢复临时修改的硬件状态（如 LoongArch 的 `DMW1` 窗口）并归还借用的物理页框。任何断言失败都会直接触发内核 panic 并停机；只有全部套件通过才会打印最终的 `<arch>: all kernel tests passed`。
- **注意 LoongArch 退出方式**：LoongArch 尚未实现主动关机，打印 `loongarch64: all kernel tests passed` 后会停在 `idle` 循环（需按 `Ctrl-a x` 退出）。判断自检是否通过必须以串口输出的成功标记为准，不能仅看 QEMU 是否退出。

---

## 3. 各测试套件覆盖详情

### 3.1 用户执行环境组（`user execution`）

`user execution` 套件通过 `ktest::user_env::UserEnv` 准备一块 4 KiB 独占测试栈及临时用户态映射（RISC-V 利用 `satp = Bare` 下的固件 PMP；LoongArch 临时配置 `DMW1 = 0x18` 允许 PLV3 访问低地址 RAM，并在 `UserEnv` 销毁时恢复进入环境前的 `DMW1` 原值及刷新 TLB），依次执行以下三项测试：

1. **架构 Trap 与上下文切换测试（`arch::trap::tests`）**：
   - **异常原因解码**：验证中断与异常编号分离、未知编号保留；LoongArch 额外验证多位并发中断掩码、异常子码（`EsubCode`）以及软件重填标记（bit 63）优先于残留的 `ESTAT.Ecode`。
   - **内核态断点与同步嵌套异常**：汇编探针向全部通用寄存器写入特征值后触发内核断点（`ebreak` / `break 0`），并在处理函数内部再次触发嵌套断点（LoongArch 还会额外触发一次内核态未映射地址的 TLB refill），验证外层栈帧不被内层覆盖、处理函数修改后的寄存器值能原样恢复，且返回前栈指针保持 16 字节对齐。
   - **C ABI 保存与恢复校验**：通过独立的汇编包装函数调用 `run_user`，在恢复调用栈前逐一校验被调用者保存寄存器（RISC-V 的 `s0..s11`/`gp`/`tp`/`sp`；LoongArch 的 `fp`/`s0..s8`/`tp`/`r21`/`sp`）是否完好无损。
   - **用户态降权与特权隔离**：故意在传入的 `TrapContext` 中填入全 1 的恶意状态位（`sstatus` / `prmd`），确认 `run_user` 强制降权至 U-mode / PLV3 且关闭中断与扩展；验证用户态执行 `ecall`/`syscall`、修改 PC/GPR/SP 后重入、越权访问特权 CSR 以及在 FPU 关闭时执行浮点指令均能精确捕获。
   - **LoongArch 用户态 TLB Refill**：临时撤销 `DMW1` 后进入用户态触发取指 TLB refill，确认捕获的 `ERA`、`BADV` 与特权级正确，随后重新开启 `DMW1` 验证常规 `syscall` 仍可正常执行。

2. **系统调用 ABI 测试（`syscall::tests`）**：
   - 用户态探针连续发起 3 次系统调用，覆盖超过 32 位的调用号、全 1 字、最高位置 1 及负数参数，确认 `syscall::decode` 完整提取 6 个参数且 `syscall::complete` 仅更新返回值寄存器（`a0`）并将 PC 前移 4 字节。
   - 前两次系统调用返回后，探针将返回值寄存器 `a0` 拷贝到下一次调用的第 6 个参数（`a5`），证明用户态确实观测到了超过 32 位的正返回值与负 errno；第 3 次系统调用返回后直接触发末尾断点。
   - 验证非系统调用异常（如断点、S-mode/PLV0 系统调用、带残留 syscall 编码的 LoongArch TLB refill）会被 `decode` 和 `complete` 拒绝且不破坏原现场；验证当 `PC = usize::MAX - 3` 时，`complete` 能检测到 `PC + 4` 溢出并拒绝执行。

3. **最小任务循环测试（`task::tests`）**：
   - 验证 `Task::run` 面对未知调用号（含低 32 位与 `exit` 相同但高 32 位非零的 64 位调用号）时能向用户态返回 `-ENOSYS`（-38）并继续运行。
   - 验证 `exit`（93）和 `exit_group`（94）仅截取首个参数的低 8 位作为退出码，将状态置为 `Finished(Exited(code))`，且保持退出指令处的 PC 与 `a0` 不变。
   - 验证已结束的任务再次调用 `run()` 会直接返回原退出状态，不会重复进入用户态。
   - 验证用户态触发未处理断点（即便寄存器中伪造了 `a7 = 93`）会以 `Finished(Faulted)` 终止，且不影响后续新建任务的正常运行。

### 3.2 内核堆测试（`heap`）

- 在全局静态堆上验证 4 KiB 页对齐的 `Box`、`Vec` 动态扩容与数据完整性、超出 1 MiB 容量的 `try_reserve_exact` 错误返回，以及反复分配释放后全局堆实际占用字节数恢复至测试前基线（无内存泄漏）。
- 在一块独立的 8 KiB 静态测试池上实例化局部 `Heap`，验证不同大小与对齐的块互不重叠、耗尽时返回 `Err`、释放后复用原地址，以及两个 4 KiB 伙伴块全部释放后能合并满足 8 KiB 分配且释放后已用字节数归零。

### 3.3 物理内存与启动数据测试（`physical regions`、`physical frames`、`FDT memory`、`boot FDT mapping`、`boot tables`）

- **`physical regions`**：测试物理区间的排序、重叠/相邻合并、保留区扣除、向内页对齐、空可用区（`usable` 为空切片）以及对 8 个页面的 $2^8 = 256$ 种保留组合进行穷举验证。
- **`physical frames`**：
  - 借用启动时创建的真实 `FrameAllocator` 试分配一页，校验页框地址 4 KiB 对齐且落在可用区间内，离开作用域后空闲页数恢复。
  - 在合成区间上验证跨空洞分配、耗尽返回 `None`、释放后复用、位图字边界（`1`、`WORD_BITS - 1`、`WORD_BITS`、`WORD_BITS + 1` 等）以及 8 页 256 种保留掩码组合。
  - 在 128 字与 8192 字的合成位图上进行 5000 轮稀疏回收与重分配压测，确认 `nonempty` 索引栈不产生重复项、不重分配堆内存且容量完全恢复。
- **`FDT memory` & `boot FDT mapping`**：使用 [`os/src/ktest/support/fdt.rs`](../../os/src/ktest/support/fdt.rs) 独立编码生成的测试设备树，验证多段 RAM、`1`/`2` cell 宽度、`status = "disabled"` 节点跳过、`initrd` 保留、FDT 本体物理区间保留，对截断、块重叠、非法魔数/版本、动态保留区、超深嵌套（> 32 层）的错误拦截，以及对整份 DTB 逐字节按位取反（`^= 0xff`）时解析器绝不 panic。
- **`boot tables`（仅 LoongArch）**：在合成的字节数组上验证 EFI 系统表签名、乱序 GUID 查找（FDT 与 initrd）、重复/缺失表项报错、地址越界拦截以及 `DMW0` 映射地址转换。

### 3.4 RISC-V Sv39 页表测试（`Sv39 paging`）

`just test rv` 和 `just test rv debug` 在物理页框测试后执行此测试组。测试与前序 Trap 测试的机器状态无关，测试代码位于 [`paging.rs`](../../os/src/ktest/arch/riscv64/paging.rs)，汇编探针位于 [`paging/probes.S`](../../os/src/ktest/arch/riscv64/paging/probes.S)。

- 软件映射覆盖同表多页、2 MiB / 1 GiB 分界、Sv39 正负规范地址边界、页内偏移、物理地址零和 56 位上界，并验证拒绝非规范地址、未对齐地址、非法权限和重复映射。
- 解除映射验证共享表保留、空中间表立即回收、重复解除返回错误，以及销毁非空页表后页框数恢复。
- 从真实页框借出 0 至 4 页建立小池，验证根表及中间分支耗尽、部分分配回滚、已有映射保持不变，避免耗尽启动分配器。
- 临时启用 Sv39，恒等映射内核镜像、页表页和 UART，另映射独占用户代码、数据和栈页。用户探针读写数据及栈后通过 `exit` 返回；未映射读取、用户读取监管态页面、写只读代码及从不可执行数据页取指产生对应的用户页故障。
- 返回后检查原 `satp` 和用户数据；将同一虚拟地址重新映射到另一物理页后激活，验证探针读到新物理页的值。
- 嵌套激活按 `Bare → A → B → A → Bare` 切换，同一探针虚拟地址在 A、B 中映射到不同物理页。检查进入 B 前后的实际读值，以及退出 B 后完整 `satp` 与进入前一致；退出 A 后恢复 Bare。另通过受控预期值验证 `satp` 回读不匹配时返回 `ActivationFailed`、跳过闭包并恢复原 `satp`。整组结束时页框数恢复。

[QEMU 10.0.2](https://github.com/qemu/qemu/blob/v10.0.2/target/riscv/csr.c#L1918-L1941) 在有效的 `satp` 写入改变模式、ASID 或根页号时自动清空 TLB。上述用例验证映射结果和 `satp` 恢复；显式 `sfence.vma` 的回归验证需要在切换 `satp` 时保留缓存翻译的环境中另行执行。

活动期间输出 `riscv64: Sv39 active`，通过后输出 `riscv64: Sv39 paging kernel tests passed`；整次自检仍以总成功标记为准。这组底层测试使用监管态 RWX 临时映射，覆盖分页下的内核执行和固定用户探针；分段权限由下一组覆盖。验证范围限于 RISC-V 的 4 KiB 页及单核地址转换。

### 3.5 RISC-V 用户地址空间测试（`user address space`）

`just test rv` 和 `just test rv debug` 在 `Sv39 paging` 之后运行此组，源码与探针位于 [`os/src/ktest/mm/address_space/`](../../os/src/ktest/mm/address_space/)。
测试使用正式地址空间接口构造两个用户根，交替运行使用相同用户虚拟地址的固定探针，检查各自数据独立且跨轮保留。
探针先验证未知系统调用返回 `-ENOSYS`，再访问用户数据和栈，最后调用 `exit`。

测试检查内核分段与 UART 的监管态权限、用户代码/数据/栈页的 `RXU`/`RWU` 权限和不同地址空间的独立页框。
用户态探针验证代码页可读、仅可执行（`XU`）页可取指但读取触发 `LoadPageFault`、只读（`RU`）页可读但写入和取指分别触发 `StoreAmoPageFault` 与 `InstructionPageFault`，以及仅在一侧映射的普通用户页在另一侧触发缺页。
测试还检查未对齐地址查重、非法及重复映射（含零空闲页框小池下重复映射仍返回 `AlreadyMapped` 且原映射不变）。在活动态下构建地址空间、调用 `map_page`、嵌套 `with_active` 或初始化物理页均因 `UnavailableInCurrentMode` 被拒绝，不执行回调也不影响外层映射。权限探针另覆盖用户读取内核代码、UART 和根表，写代码页，访问栈保护页，以及从数据页取指产生的故障。
软件查表在 Bare 下确认根表与中间表页均未建立同地址映射。
正常退出与故障终止后均检查恢复未激活状态（`is_user_inactive()`）且执行前后空闲页框数一致，销毁地址空间后检查页框全部回收。
通过时输出 `riscv64: user address space kernel tests passed`；整次自检以总成功标记为准。
范围限于 RISC-V 单核、关中断和同步探针。

### 3.6 用户程序装载准备测试（`ELF64`、`initial user stack`）

- **`ELF64`**：使用 [`os/src/ktest/support/elf.rs`](../../os/src/ktest/support/elf.rs) 编码的最小 RISC-V / LoongArch ELF64 镜像，验证双架构机器号匹配与跨架构拒绝、多加载段数据切片与页内偏移、跨页 BSS 清零范围、权限位、空段与纯 BSS 段处理、入口指令对齐差异（RISC-V 允许 2 字节、LoongArch 要求 4 字节）。
  同时验证 `PT_PHDR` 的优先选择、跳过仅 `PF_X` 不可读副本与非法声明（重复、晚于 `PT_LOAD`、尺寸不符、被 BSS 截断、显式指向不可读段），以及坏魔数、非 64 位/大端/非 `ET_EXEC`/`PT_INTERP`、`PN_XNUM`、偏移未对齐、文件越界、乱序段、仅在页级重叠的加载段与页边界地址溢出。
- **`initial user stack`**：验证非对齐栈顶下的 64 位字布局（`argc`、`argv`/`envp` 指针与空终止符、`auxv` 顺序与 `(AT_NULL, 0)`）、用户虚拟地址换算、字符串内容、零填充对齐区与 16 字节对齐的最终 `sp`；覆盖恰好匹配与差 1 字节的边界容量、空列表与空字符串，以及基址下溢、内嵌 `\0` 和传入 `AT_NULL` 等错误路径，并确认失败时缓冲区保持原样、成功时不改动 `sp` 以下区域。

### 3.7 RISC-V 静态 ELF 执行（`static ELF execution`）

`just test rv` 与 `just test rv debug` 运行此组，源码与探针位于 [`os/src/ktest/task/loader/`](../../os/src/ktest/task/loader/)。
测试在预先填充非零脏字节（`0xa5`）的小页框池上装载含用户探针的 ELF 镜像，在 U-mode 验证初始栈 16 字节对齐、以奇地址 `argv[0]` 逐字节 `lbu` 核对完整 `argv`/`envp` 及空终止符、按键扫描 `auxv` 核对 `AT_ENTRY`/`AT_PAGESZ` 与成组程序头项。
同时覆盖可读（实测 U-mode 读取 `AT_PHDR` 程序头）、仅 `PF_X` 与未覆盖三种程序头表布局，并验证跨页文件数据、页首/BSS/页尾全清零及数据段可写性。
测试用容量为 `0..needed` 的小页框池逐一验证中途 OOM 返回 `OutOfMemory` 并回收已分配页框；另验证无效 ELF、W+X 权限、越界段、启动栈容量不足、参数内嵌空字节及活动态下再次装载分别返回对应的装载错误，执行前后均核验地址转换状态与页框回收。
整次自检以总成功标记为准。

---

## 4. 手动检查与故障注入验证

以下场景涉及观察不同内存配置下的启动日志或故意触发内核致命 panic，需在独立 QEMU 实例中手动执行（不纳入顺序运行的 `just test`）。

### 4.1 核对不同内存配置与 initrd 的物理区间日志

在**开发容器内**执行以下命令，核对启动日志中的 `RAM`、`reserved` 与 `usable` 区间：

| 命令 | 预期日志中的 `RAM` 区间 |
| --- | --- |
| `MEM=128M just run rv` | `[0x80000000, 0x88000000)` |
| `MEM=1G just run rv` | `[0x80000000, 0xc0000000)` |
| `MEM=768M just run la` | `[0x0, 0x10000000)`、`[0x80000000, 0xa0000000)` |
| `MEM=1G just run la` | `[0x0, 0x10000000)`、`[0x80000000, 0xb0000000)` |

验证携带 `initrd` 时，其物理范围会被自动加入 `reserved`：

```bash
MEM=128M just run rv -initrd docs/architecture/boot.md
MEM=3G just run la -initrd docs/architecture/boot.md
```

> **说明：** LoongArch 传入 `-initrd` 时指定 `MEM=3G` 是因为 QEMU 10.0.2 的加载器在校验 initrd 放置地址时，会将高地址 RAM 中的 initrd 物理末端直接与 `-m` 指定的总内存字节数比较。

### 4.2 注入非法的设备树保留区（验证启动失败停机）

在**宿主机**仓库根目录执行以下命令（需宿主机已安装 `device-tree-compiler` 提供的 `fdtput` 工具）：先从 QEMU 导出一份正常的 DTB，注入一个不受支持的动态 `/reserved-memory` 节点，再传入内核启动：

```bash
mkdir -p artifacts/memory-negative
just container qemu-system-riscv64 -machine virt,dumpdtb=artifacts/memory-negative/base.dtb -m 128M -smp 1 -nographic
cp artifacts/memory-negative/base.dtb artifacts/memory-negative/dynamic.dtb
fdtput -p -t x artifacts/memory-negative/dynamic.dtb /reserved-memory '#address-cells' 2
fdtput -t x artifacts/memory-negative/dynamic.dtb /reserved-memory '#size-cells' 2
fdtput artifacts/memory-negative/dynamic.dtb /reserved-memory ranges
fdtput -p -t x artifacts/memory-negative/dynamic.dtb /reserved-memory/dynamic size 0 1000
just container just build rv release ktest
just container timeout 5s qemu-system-riscv64 -machine virt -bios default -kernel kernel-rv -m 128M -smp 1 -nographic -no-reboot -dtb artifacts/memory-negative/dynamic.dtb
```

预期串口应打印 `memory initialization failed` 与 `dynamic reserved-memory allocation is unsupported` 的 panic 信息并停机，不得出现 `booted on riscv64` 或 `all kernel tests passed`。

### 4.3 注入致命内核态 Trap（验证诊断输出与停机）

可恢复的内核断点探针不会覆盖 `trap::handle_kernel` 的致命异常 panic 分支。可在独立 debug QEMU 会话中通过 GDB 手动注入未处理的内核态异常（双架构通用，以下以 RISC-V 为例）：

1. 终端 1 启动 `just debug rv`，终端 2 进入同一容器执行 `gdb-multiarch kernel-rv`。
2. 在 GDB 中连接并断在 `trap::init` 之后，将当前指令指针篡改为非法的未对齐或未映射地址：
   ```gdb
   target remote :1234
   break impl_os::mm::init
   continue
   set $pc = 0x2
   continue
   ```
3. 预期串口打印形如 `kernel trap: <cause>: pc=..., status=..., raw_cause=..., bad_addr=..., sp=...` 的完整诊断信息，并调用 `arch::halt()` 原地挂起 CPU（严禁触发正常关机退出或输出后续启动成功日志）。

### 4.4 注入 `Task::run` 系统调用 PC 溢出（验证故障收敛）

白盒单元测试已直接验证 `syscall::complete` 对 `PC = usize::MAX - 3` 的溢出拒绝；若需进一步验证 `Task::run` 循环将该溢出收敛为 `ExitStatus::Faulted` 的集成路径，可在独立 debug 会话中注入：

1. 先编译带 `ktest` 的 debug 内核并以调试模式启动（或在容器中运行带 `-S -s` 的 `ktest` 二进制），使用 `gdb-multiarch` 连接。
2. 在 `os/src/task.rs` 的 `Task::run` 收到 `Outcome::Return`、即将调用 `syscall::complete(&mut self.context, result)` 行下断点。
3. 命中断点后，在 GDB 中将该任务的保存 PC 改为 `usize::MAX - 3`（如 `set self.context.sepc = 0xfffffffffffffffc` 或 LoongArch 下 `set self.context.era = 0xfffffffffffffffc`），单步执行通过 `syscall::complete`。
4. 确认 `syscall::complete` 返回 `false`、`Task::run` 立即跳出循环并将任务状态置为 `TaskState::Finished(ExitStatus::Faulted)`，且 `self.context` 中的 PC 与通用寄存器保持原样，绝不携带溢出地址再次调用 `run_user`。
