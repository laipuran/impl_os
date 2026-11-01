# 启动流程与当前限制

## 共享部分

1. QEMU 加载 ELF，进入 `_start`。
2. 架构汇编停放非启动 CPU，设置启动栈，跳转 `kernel_main`。
3. Rust 用 volatile stores 清零 `sbss..ebss`，避免依赖尚未提供的 memset。
4. 初始化日志和 trap 向量，输出架构；启用 `trap-smoke` 时执行异常及降权往返检查。
5. 调用架构关机入口；未预期的内核异常输出保存的异常状态并 panic 停机。

链接脚本把 `.bss.stack` 放在 `sbss` 之前，不能在 Rust 已经使用启动栈后将它清零。启动栈固定 64 KiB。

## RISC-V

- QEMU virt + 默认 SBI 固件，ELF 链接/入口位于 `0x80200000`。
- 固件通过 `a0` 传入 hart ID；只有 hart 0 进入 Rust，其他 hart 停放。
- 在分页关闭状态下访问 UART `0x10000000`。
- 正常结束调用 SBI v0.2 System Reset 的 shutdown；不支持该扩展时停放 CPU。
- panic 输出后停放，不伪装成功退出。

UART 地址依赖当前分页关闭的状态；启动过程固定选择 hart 0。

## LoongArch

- ELF 虚拟起始地址 `0x9000000090000000`，物理加载地址 `0x90000000`。
- QEMU 从物理地址开始执行；入口先建立 DMW0 高地址窗口及临时 DMW1 低地址窗口，再开启分页并跳到高地址，撤销临时窗口。不能先开启分页再从未映射的低地址继续取指。
- 使用 `la.local` 的 PC-relative 地址生成，设置栈后进入共享 Rust 入口。
- 早期 UART 假定 QEMU virt 的 `0x1fe001e0`，经 DMW0 访问。
- 关机入口输出提示后停放 CPU，不会主动退出 QEMU。

LoongArch 缺少平台电源管理实现，因此当前启动流程不满足测试结束后主动关机的要求。

## Trap 与特权级切换

`arch::trap` 在两个目标上提供相同的整数上下文接口：

- `init()`：启动 CPU 安装异常向量，关闭中断源及浮点/向量扩展，初始化保留的 scratch CSR。
- `TrapContext::user(entry, stack)`：构造用户态上下文。`regs` 按硬件寄存器编号排列，另保存 `pc`、`status`、`cause`、`bad_addr`；入口和栈的对齐要求见对应 Rust API。
- `unsafe run_user(&mut context)`：保存调用者内核 ABI 状态，恢复用户整数寄存器并降权执行；用户异常直接写回该上下文，再返回原内核调用栈。可以检查异常、修改 PC/寄存器，然后再次调用。入口强制用户权限并关闭中断/扩展，不信任传入的特权状态位。

| 架构 | 用户态返回指令 | 普通异常状态 | 内核/用户栈区分 |
| --- | --- | --- | --- |
| RISC-V | `sret`，S → U | `sepc`、`sstatus`、`scause`、`stval` | 内核 `sscratch=0`；用户执行时指向可信内核切换帧 |
| LoongArch | `ertn`，PLV0 → PLV3 | `ERA`、`PRMD`、`ESTAT`、`BADV` | 根据保存的特权级选择当前内核栈或 SAVE2 中的切换帧；保留 SAVE0..SAVE3 |

所有整数寄存器在调用 Rust 前保存，用户栈不会用于执行内核处理代码。内核异常在当前栈上分配独立、16 字节对齐的帧，允许保存完成后的同步嵌套异常；处理结束恢复帧并返回内核。共享 `trap::handle_kernel` 默认对所有内核异常 panic；只有显式 smoke 功能识别并恢复其自身的探针。

LoongArch 另安装物理 `TLBRENTRY`，处理 refill 时从直接地址模式回到内核 DMW。此类上下文的 `cause` 最高位为 1，其余位保留 ESTAT（其中 Ecode 不是此次 refill 的原因）；PC/故障地址来自 `TLBRERA`/`TLBRBADV`，`status` 统一为 PRMD 位布局。当前只报告 refill，不遍历页表或填充 TLB。

### 边界

- 仅支持启动 CPU、关闭中断的同步执行；没有调度、时钟/外设中断处理或系统调用分发。
- 只保存整数状态；浮点、向量指令会异常，不提供相应上下文切换。
- 特权级切换不等于内存隔离。尚无用户页表、进程或 ELF 加载器；调用者负责映射及保护上下文、内核栈和内核数据。
- RISC-V 当前使用 bare 地址空间，用户访问依赖固件 PMP。LoongArch 默认 DMW0 只允许 PLV0；仅 `trap-smoke` 临时开启 PLV3 低地址直映并在结束时恢复、失效缓存翻译。该测试映射不能用于不可信程序。
- 异常入口保存完成之前的故障、内核栈溢出和不可恢复硬件异常不提供恢复保证。
