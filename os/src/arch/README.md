# 架构与平台边界

- `mod.rs`：按 Rust target 导出所选架构，通用代码不应直接引用另一个架构。
- `riscv64/`：QEMU virt、S-mode 入口、链接地址、SBI 和早期 UART；`trap/` 保存和恢复 S/U 态整数现场。
- `loongarch64/`：QEMU virt、PLV0/DMW 启动、链接地址和早期 UART；`trap/` 保存和恢复 PLV0/PLV3 整数现场并捕获 TLB refill；平台关机未实现。

两个架构导出同一组接口：`NAME`、`putchar`、`shutdown`、`halt`。UART 地址由 QEMU virt 平台固定；RISC-V 直接访问物理地址，LoongArch 经 DMW0 访问。

两个架构独立提供 `trap::init`、`TrapFrame::user(entry, stack)` 和 `unsafe enter_user(frame) -> !`，目录均包含 `cause.rs`、`entry.S`、`return.S` 和 `mod.rs`，尚未抽象共享 Trap 逻辑。用户异常经 handler 处理后直接返回用户态，不返回 `enter_user` 的调用者；默认启动不进入用户态。

Trap 支持断点恢复、可信内核栈切换和现场保存完成后的可处理同步嵌套异常。仅保存整数现场，中断和扩展保持关闭，不提供用户映射、内存隔离或调度。现场布局由 Rust 定义并传给汇编；修改入口时须保留“先保存被打断寄存器，再重建内核 ABI、调用 Rust”的顺序。
