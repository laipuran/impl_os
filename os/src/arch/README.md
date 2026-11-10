# 架构与平台边界

`arch` 为共享内核代码提供平台操作、启动信息和异常处理接口。Rust target 决定使用哪个架构模块；共享代码通过 `arch` 调用所选实现，不应直接引用另一个架构。

- `mod.rs`：按 Rust target 导出所选架构。
- `riscv64/`：QEMU virt、S-mode 入口、链接地址、SBI 和早期 UART。
- `loongarch64/`：QEMU virt、PLV0/DMW 启动、链接地址和早期 UART；平台关机未实现。

两个架构导出同一组接口：`NAME`、`putchar`、`shutdown`、`halt`、`boot::{memory, kernel_region}`，以及 `trap::{init, TrapContext, run_user}`。入口汇编按 C ABI 将平台启动参数传给 `kernel_main`，`boot` 模块负责解释启动协议并转换地址。UART 地址由 QEMU virt 平台确定：RISC-V 直接访问物理地址，LoongArch 经 DMW0 访问。

`boot_data.rs` 供两架构复用：`MappedFdt::from_raw` 在明确的可读性和有效期前提下借用原始启动存储，随后通过安全接口解析并记录 FDT 本体占用。`mm::fdt` 只处理字节切片，不负责裸地址或平台映射；平台额外的启动块和 initrd 保留仍由各自的 `boot` 模块追加。

各架构的 `trap/mod.rs` 定义整数上下文，通过共用的方法提供 PC/GPR 编辑和异常信息读取。上下文字段保持私有，内部使用硬件寄存器名称；`trap/cause.rs` 负责解析异常原因，并用 `thiserror` 声明显示描述，处理策略由调用者决定。

`trap/entry.S` 按 Rust 提供的字段偏移和字长保存异常上下文，`trap/return.S` 负责恢复上下文和用户态切换。用户异常返回 `run_user` 调用者，内核异常进入共享的 `crate::trap::handle_kernel`。

测试代码及汇编探针放在 `trap/tests/` 中，仅在启用 `ktest` 时参与构建，由 `ktest::run` 按测试组列表调用。

当前特权级切换尚不提供用户地址空间隔离；平台约定和限制见[启动流程](../../../docs/architecture/boot.md)。

`syscall.rs` 提供 Linux 调用号和参数的寄存器位置及用户 syscall 来源判断，供共享 `crate::syscall` 解码和写回结果。
