# 开发检查

## 命令覆盖范围

| 命令 | 检查范围 | 不覆盖 |
| --- | --- | --- |
| 容器内 `just check` | Rust 格式、双架构 Clippy | 链接产物、启动行为、内核功能 |
| 容器内 `just build` / `make all` | 两架构 ELF 构建与导出 | 启动及运行时语义 |
| 宿主 `just judge-build` | 原始比赛镜像断网，使用独立 Cargo 缓存构建 | 源码过滤、启动、评分 |
| 容器内 `just run rv` / `just run la` | 手动观察启动日志和进程退出 | 官方用户程序的正确性 |
| 容器内 `just test [rv\|la] [release\|debug]` | 构建启用 `ktest` 的 ELF 并执行全部已接入内核自检；目前覆盖 trap | 未实现的测试功能、官方用户程序 |

当前有统一的启动期内核自检入口，显式调用各模块测试，没有自动注册、名称筛选或官方测试集运行入口。格式和构建检查不能替代运行时功能测试。

## 当前运行行为

- 共享入口清零 BSS，初始化日志和 trap，输出架构，然后调用 `arch::shutdown()`。
- RISC-V 使用 SBI System Reset 关机；固件不支持时停放 CPU。
- LoongArch 输出未实现关机提示并停放 CPU，需要手动退出 QEMU。
- panic 输出错误并停放 CPU，不走正常关机路径。

RISC-V 正常启动应输出日志后退出 QEMU。LoongArch 的持续运行是当前停放逻辑，不代表已经完成主动关机。

## 统一内核自检入口

命令格式为 `just test [架构] [构建模式]`，默认架构 `rv`、模式 `release`。该命令统一启用 Cargo feature `ktest`，经现有 Make 构建入口导出 ELF，再调用 QEMU；`MEM`、`SMP` 环境变量与 `just run` 一致。一次运行执行所选架构的全部已接入测试；验证双架构时分别运行两个 QEMU 实例。

`ktest` 只控制测试代码，不控制 trap 等正式能力。共享入口先正常初始化内核，再调用 `os/src/ktest.rs` 中的 `run_all()`；默认构建不编译该模块和各架构的 trap 测试探针。这里使用 `#[cfg(feature = "ktest")]`，不是 Rust 的 `#[cfg(test)]` 或自定义 `#[cfg(ktest)]`，也不依赖 Cargo 测试 harness。

新增自检时，将测试代码放在所属模块并用同一个 `ktest` feature 控制，在 `run_all()` 中显式增加调用，不新增测试专用 feature 或 Just recipe。测试依赖的基础能力应由正常初始化提供，不能把另一组自检当作初始化入口。所有调用都必须位于总成功标记之前。

测试在启动 CPU 上关闭中断、依次执行，每组测试返回前必须恢复临时映射、机器状态并释放测试资源。失败直接走现有 panic 停机，不捕获后继续执行；故意触发致命异常或无法恢复状态的测试应在独立 QEMU 实例中验证，不加入这条顺序执行路径。

## Trap 运行验证

在开发容器内运行：

```sh
just test
just test la
```

两者分别构建启用 `ktest` 的 RISC-V / LoongArch release ELF，再启动 QEMU。trap 自检完成后输出 `<架构>: trap smoke passed`；只有全部自检完成，统一入口才输出 `<架构>: all kernel tests passed`。断言失败走 panic，不输出总成功标记。LoongArch 成功后仍停放，需 `Ctrl-a`、`x` 退出；自动运行时的超时本身既不是成功也不是失败，必须检查串口的总成功标记。

验证内容：

- 内核 breakpoint 的整数上下文保存/恢复，以及处理期间的同步嵌套异常。
- 用户态 `ecall` / `syscall` 返回内核，验证所有整数寄存器、异常 PC、原因和先前特权级。
- 修改用户 PC、整数寄存器和栈指针后重新进入，确认恢复的是修改后的上下文。
- 输入恶意特权状态位仍只能进入 U/PLV3；实际执行特权指令必须异常。
- 浮点关闭时执行浮点指令必须异常，而不是静默破坏未保存状态。
- LoongArch 内核嵌套 refill、用户未映射取指及其后的正常 syscall；临时 DMW 修改同步失效缓存翻译。

debug 模式直接使用 `just test rv debug` 或 `just test la debug`。自检映射只供固定可信探针使用，不提供进程隔离。恢复正常产物使用 `just build`；默认构建不启用自检。

LoongArch 的 Rust 可调用汇编函数提供 ELF `FUNC` 类型和函数大小。构建 trap smoke 后，可用 `readelf -Ws kernel-la` 检查 `__loongarch_run_user`、`__loongarch_smoke_kernel`、`__loongarch_smoke_nested`、`__loongarch_smoke_refill` 的类型及范围；GDB 中 `disassemble __loongarch_run_user` 应止于恢复路径的 `ertn`，不包含后续异常入口前的对齐填充。

特性分支的静态检查需显式传入功能：

```sh
cargo clippy --locked --offline -p impl-os --target riscv64gc-unknown-none-elf --features ktest -- -D warnings
cargo clippy --locked --offline -p impl-os --target loongarch64-unknown-none --features ktest -- -D warnings
```
