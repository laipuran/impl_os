# 启动流程与当前限制

## 共享部分

1. QEMU 加载 ELF，进入 `_start`。
2. 架构汇编停放非启动 CPU，设置启动栈，跳转 `kernel_main`。
3. Rust 用 volatile stores 清零 `sbss..ebss`，避免依赖尚未提供的 memset。
4. 输出架构和“仅为基架”提示，调用架构关机入口。

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
