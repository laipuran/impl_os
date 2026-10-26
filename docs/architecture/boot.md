# 启动流程与当前限制

## 共享部分

1. QEMU 加载 ELF，进入 `_start`。
2. 架构汇编停放非启动 CPU，设置启动栈，跳转 `kernel_main`。
3. Rust 用 volatile stores 清零 `sbss..ebss`，避免依赖尚未提供的 memset。
4. 输出架构和“仅为基架”提示，调用架构关机入口。

链接脚本把 `.bss.stack` 放在 `sbss` 之前，不能在 Rust 已经使用启动栈后将它清零。启动栈固定 64 KiB；没有内核堆、页分配器或用户态。

## RISC-V

- QEMU virt + 默认 SBI 固件，ELF 链接/入口位于 `0x80200000`。
- 固件传入 hart ID 的 `a0`；当前只允许 hart 0 继续。尚未做 HSM/SMP 启动。
- 在分页关闭状态下访问 UART `0x10000000`。
- 正常结束调用 SBI v0.2 System Reset 的 shutdown；不支持该扩展时停放 CPU。
- panic 输出后停放，不伪装成功退出。

启用分页后必须替换早期 MMIO 地址假设；启动 hart 的选择也应随正式多核设计调整。

## LoongArch

- ELF 虚拟起始地址 `0x9000000090000000`，物理加载地址 `0x90000000`。
- QEMU 从物理地址开始执行；入口先建立 DMW0 高地址窗口及临时 DMW1 低地址窗口，再开启分页并跳到高地址，撤销临时窗口。不能先开启分页再从未映射的低地址继续取指。
- 使用 `la.local` 的 PC-relative 地址生成，设置栈后进入共享 Rust 入口。
- 早期 UART 假定 QEMU virt 的 `0x1fe001e0`，经 DMW0 访问。
- 已在比赛指定镜像的 QEMU 10.0.2 完成串口冒烟，**关机未实现，会停放 CPU。**

不要把 LoongArch ELF 可编译视作已完成启动，更不能视为已满足比赛结束退出约定。验证记录统一写入 `docs/testing/README.md`。
