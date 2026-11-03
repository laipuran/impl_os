# 启动流程与当前限制

## 共享部分

1. QEMU 加载 ELF，进入 `_start`。
2. 架构汇编停放非启动 CPU，设置启动栈，跳转 `kernel_main`。
3. Rust 用 volatile stores 清零 `sbss..ebss`，避免依赖尚未提供的 memset。
4. 初始化日志，安装所选架构的 Trap 入口，然后输出架构并调用架构关机入口。

链接脚本把 `.bss.stack` 放在 `sbss` 之前，不能在 Rust 已经使用启动栈后将它清零。启动栈固定 64 KiB。

## RISC-V

- QEMU virt + 默认 SBI 固件，ELF 链接/入口位于 `0x80200000`。
- 固件通过 `a0` 传入 hart ID；只有 hart 0 进入 Rust，其他 hart 停放。
- 入口设置内核 `gp`、`tp=0` 和启动栈；当前不使用内核 TLS。
- 日志初始化后安装 Direct 模式的 Trap 入口，初始化 `sscratch=0`，关闭 S 态中断及浮点/向量等扩展状态。
- `trap/entry.S` 区分来源、切栈并保存整数寄存器和异常 CSR，随后重建内核 `gp/tp` 进入 Rust；`trap/return.S` 恢复现场并执行 `sret`。
- 内核执行时 `sscratch=0`，S 态 Trap 使用被打断的内核栈；用户执行时 `sscratch` 保存可信内核栈顶，U 态 Trap 先交换栈指针，不依赖用户 `sp` 的有效性。内核栈须 16 字节对齐并留足空间。
- `TrapFrame::user` 构造初始用户现场，`enter_user` 消费当前内核调用栈进入 U 态，不返回调用者。用户异常经 handler 处理后直接返回 U 态；用户返回特权状态由内核固定生成，中断和扩展保持关闭。
- 当前 S/U 态均处理 `ebreak` 和 `c.ebreak`，按指令长度推进 PC；其他异常记录错误后停放。支持现场保存完成后、栈有效时的可处理同步嵌套异常，不支持入口/返回切换窗口故障恢复。
- 仅有整数现场切换，不包含异步中断、系统调用分发、调度或用户地址空间隔离。用户代码必须可信，不能破坏内核内存；指令须可供内核读取，且固件 PMP 须允许 U 态访问所需 RAM。默认启动不进入用户态。
- 在分页关闭状态下访问 UART `0x10000000`。
- 正常结束调用 SBI v0.2 System Reset 的 shutdown；不支持该扩展时停放 CPU。
- panic 输出后停放，不伪装成功退出。

UART 地址依赖当前分页关闭的状态；启动过程固定选择 hart 0。

## LoongArch

- ELF 虚拟起始地址 `0x9000000090000000`，物理加载地址 `0x90000000`。
- QEMU 从物理地址开始执行；入口先建立 DMW0 高地址窗口及临时 DMW1 低地址窗口，再开启分页并跳到高地址，撤销临时窗口。不能先开启分页再从未映射的低地址继续取指。
- 使用 `la.local` 的 PC-relative 地址生成，设置栈后进入共享 Rust 入口。
- 启动入口固定 `tp=0`，关闭 EUEN 扩展；日志初始化后安装 EENTRY/TLBRENTRY，关闭中断，ECFG.VS=0。两个异常入口均按 4096 字节对齐，TLBRENTRY 使用物理地址。
- `trap/` 与 RISC-V 保持独立但对应的结构：`cause.rs` 解析原因，`entry.S` 保存现场和切栈，`return.S` 恢复现场并 `ertn`，`mod.rs` 提供 `init`、`TrapFrame::user` 和 `enter_user`。
- SAVE0/1 暂存入口 t0/sp，SAVE2 保存用户执行时的可信内核栈顶；按 PRMD.PPLV 区分 PLV0/PLV3，不把用户 sp 当成可信栈。整数现场按 r0..r31 保存，r3 是 sp，r2 是 tp。
- PLV0/PLV3 的 `break` 均推进 ERA 四字节后恢复；支持现场保存完成后的可处理同步嵌套异常。`enter_user` 消费当前内核调用栈，用户异常处理后直接返回 PLV3；返回时固定用户 PRMD，中断和扩展保持关闭。
- TLB refill 使用专用入口和 TLBRERA/TLBRPRMD/TLBRBADV，在现场的 estat bit63 添加标记，避免误用过期的 ESTAT.Ecode。默认仅诊断并停放，不填充 TLB，也不处理其他未预期异常。
- 默认 DMW0 仅供 PLV0。调用者必须提供 PLV3 可执行/可访问映射，不能直接用内核高地址作为用户入口。当前没有内存隔离，只允许可信用户代码；不支持调度、系统调用分发、异步中断或浮点/向量现场。默认启动不进入用户态。
- 早期 UART 假定 QEMU virt 的 `0x1fe001e0`，经 DMW0 访问。
- 关机入口输出提示后停放 CPU，不会主动退出 QEMU。

LoongArch 缺少平台电源管理实现，因此当前启动流程不满足测试结束后主动关机的要求。
