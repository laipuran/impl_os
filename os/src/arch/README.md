# 架构与平台边界

- `mod.rs`：按 Rust target 导出所选架构，通用代码不应直接引用另一个架构。
- `riscv64/`：QEMU virt、S-mode 入口、链接地址、SBI 和早期 UART。
- `loongarch64/`：QEMU virt、PLV0/DMW 启动、链接地址和早期 UART；平台关机未实现。

两个架构导出同一组接口：`NAME`、`putchar`、`shutdown`、`halt`，以及 `trap::{init, TrapContext, run_user}`。UART 地址由 QEMU virt 平台固定；RISC-V 直接访问物理地址，LoongArch 经 DMW0 访问。

各架构的 `trap/mod.rs` 定义字段私有、保留硬件命名的整数上下文，通过共同访问方法公开 PC/GPR 编辑及异常信息读取；`trap/cause.rs` 独立解析架构原因，不决定异常策略。`trap/entry.S` 按 Rust 提供的字段偏移及字长保存/恢复上下文并切换特权级；测试代码及汇编探针独立放在 `trap/tests/` 中，仅在显式开启 `ktest` 时参与构建，由共享 `ktest::run` 按具名测试组列表调用。用户异常返回 `run_user` 调用者，内核异常进入共享 `crate::trap::handle_kernel`。当前无用户地址空间隔离，详见 `docs/architecture/boot.md`。
