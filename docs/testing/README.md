# 开发检查

## 命令覆盖范围

| 命令 | 检查范围 | 不覆盖 |
| --- | --- | --- |
| 容器内 `just check` | Rust 格式、双架构 Clippy | 链接产物、启动行为、内核功能 |
| 容器内 `just build` / `make all` | 两架构 ELF 构建与导出 | 启动及运行时语义 |
| 宿主 `just judge-build` | 原始比赛镜像断网，使用独立 Cargo 缓存构建 | 源码过滤、启动、评分 |
| 容器内 `just run rv` / `just run la` | 手动观察启动日志和进程退出 | 官方用户程序的正确性 |
| 容器内 `just test <测试名> [rv\|la] [release\|debug]` | 构建指定 smoke 功能并在 QEMU 中执行；当前提供 `trap` | 未实现的测试功能、官方用户程序 |

当前有可选的启动期 trap 自检，没有通用内核测试框架或官方测试集运行入口。格式和构建检查不能替代运行时功能测试。

## 当前运行行为

- 共享入口清零 BSS，初始化日志和 trap，输出架构，然后调用 `arch::shutdown()`。
- RISC-V 使用 SBI System Reset 关机；固件不支持时停放 CPU。
- LoongArch 输出未实现关机提示并停放 CPU，需要手动退出 QEMU。
- panic 输出错误并停放 CPU，不走正常关机路径。

RISC-V 正常启动应输出日志后退出 QEMU。LoongArch 的持续运行是当前停放逻辑，不代表已经完成主动关机。

## 统一 smoke 入口

命令格式为 `just test <测试名> [架构] [构建模式]`，默认架构 `rv`、模式 `release`。测试名自动映射到 `<测试名>-smoke` Cargo feature，经现有 Make 构建入口导出 ELF，再调用 QEMU；`MEM`、`SMP` 环境变量与 `just run` 一致。

新增 smoke 时，在 `os/Cargo.toml` 定义对应的 `<测试名>-smoke` feature，并接入受该 feature 控制的启动期检查。运行时只需更换 `just test` 的测试名，不需要新增或修改 Just recipe。当前已实现的测试名只有 `trap`；不存在的 feature 由 Cargo 报错，构建失败不会启动 QEMU。

## Trap 运行验证

在开发容器内运行：

```sh
just test trap rv
just test trap la
```

两者分别构建启用 `trap-smoke` 的 release ELF，再启动 QEMU。成功时输出 `riscv64: trap smoke passed` 或 `loongarch64: trap smoke passed`；断言失败走 panic，不输出成功标记。LoongArch 成功后仍停放，需 `Ctrl-a`、`x` 退出；自动运行时的超时本身既不是成功也不是失败，必须检查串口结果。

验证内容：

- 内核 breakpoint 的整数上下文保存/恢复，以及处理期间的同步嵌套异常。
- 用户态 `ecall` / `syscall` 返回内核，验证所有整数寄存器、异常 PC、原因和先前特权级。
- 修改用户 PC、整数寄存器和栈指针后重新进入，确认恢复的是修改后的上下文。
- 输入恶意特权状态位仍只能进入 U/PLV3；实际执行特权指令必须异常。
- 浮点关闭时执行浮点指令必须异常，而不是静默破坏未保存状态。
- LoongArch 内核嵌套 refill、用户未映射取指及其后的正常 syscall；临时 DMW 修改同步失效缓存翻译。

debug 模式直接使用 `just test trap rv debug` 或 `just test trap la debug`。自检映射只供固定可信探针使用，不提供进程隔离。恢复正常产物使用 `just build`；默认构建不启用自检。

LoongArch 的 Rust 可调用汇编函数提供 ELF `FUNC` 类型和函数大小。构建 trap smoke 后，可用 `readelf -Ws kernel-la` 检查 `__loongarch_run_user`、`__loongarch_smoke_kernel`、`__loongarch_smoke_nested`、`__loongarch_smoke_refill` 的类型及范围；GDB 中 `disassemble __loongarch_run_user` 应止于恢复路径的 `ertn`，不包含后续异常入口前的对齐填充。

特性分支的静态检查需显式传入功能：

```sh
cargo clippy --locked --offline -p impl-os --target riscv64gc-unknown-none-elf --features trap-smoke -- -D warnings
cargo clippy --locked --offline -p impl-os --target loongarch64-unknown-none --features trap-smoke -- -D warnings
```
