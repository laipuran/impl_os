# 架构与平台边界

- `mod.rs`：按 Rust target 导出所选架构，通用代码不应直接引用另一个架构。
- `riscv64/`：QEMU virt、S-mode 入口、链接地址、SBI 和早期 UART；`trap/` 收拢 S 态 Trap 的 Rust 实现与入口汇编。
- `loongarch64/`：QEMU virt、PLV0/DMW 启动、链接地址和早期 UART；平台关机未实现。

两个架构导出同一组接口：`NAME`、`putchar`、`shutdown`、`halt`。UART 地址由 QEMU virt 平台固定；RISC-V 直接访问物理地址，LoongArch 经 DMW0 访问。

目前只有 RISC-V 提供 `trap::init`。Trap 入口仅支持有效内核栈上的诊断和停机，不支持用户态、嵌套异常恢复或返回。现场布局由 Rust 定义并传给汇编；修改入口时须保留“先保存被打断寄存器，再重建内核 ABI、调用 Rust”的顺序。
