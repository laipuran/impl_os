# 架构与平台边界

- `mod.rs`：按 Rust target 导出所选架构，通用代码不应直接引用另一个架构。
- `riscv64/`：QEMU virt、S-mode 入口、链接地址、SBI 和早期 UART。
- `loongarch64/`：QEMU virt、PLV0/DMW 启动、链接地址和早期 UART；平台关机未实现。

未来在这里实现寄存器、页表格式、trap frame、上下文切换、TLB/屏障和硬件时钟的架构部分。不要将 VFS、通用调度策略或 Linux syscall 分发放入架构模块。

目前硬编码 UART 地址只服务 bring-up；设备发现和正式驱动建立后，应收拢到明确的平台描述/驱动边界。
