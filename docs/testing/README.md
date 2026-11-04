# 开发检查

## 命令覆盖范围

| 命令 | 检查范围 | 不覆盖 |
| --- | --- | --- |
| 容器内 `just check` | Rust 格式、双架构 Clippy | 链接产物、启动行为、内核功能 |
| 容器内 `just build` / `make all` | 两架构 ELF 构建与导出 | 启动及运行时语义 |
| 宿主 `just judge-build` | 原始比赛镜像断网，过滤隐藏文件和目录，以空 Cargo 缓存及独立构建目录构建双架构 ELF | 启动、评分 |
| 容器内 `just run rv` / `just run la` | 手动观察启动日志和进程退出 | 官方用户程序的正确性 |
| 容器内 `just test [rv\|la] [release\|debug]` | 构建启用 `ktest` 的 ELF 并执行全部已接入内核自检；目前覆盖 trap 和 heap | 未实现的测试功能、官方用户程序 |

当前有统一的启动期内核自检入口，通过静态具名测试组列表组织各模块测试，没有自动注册、名称筛选或官方测试集运行入口。格式和构建检查不能替代运行时功能测试。

`judge-build` 在容器临时目录中复制源码，排除隐藏文件和目录、本地 `target/`、`artifacts/` 及根目录已有 ELF；只将成功构建的两个 ELF 复制回仓库。原始镜像必须预先存在，并提供固定工具链及两个目标的标准库；命令不会拉取镜像或安装工具链。

## 当前运行行为

- 共享入口清零 BSS，初始化日志、trap 和内核堆，输出架构，然后调用 `arch::shutdown()`。
- RISC-V 使用 SBI System Reset 关机；固件不支持时停放 CPU。
- LoongArch 输出未实现关机提示并停放 CPU，需要手动退出 QEMU。
- panic 输出错误并停放 CPU，不走正常关机路径。

RISC-V 正常启动应输出日志后退出 QEMU。LoongArch 的持续运行是当前停放逻辑，不代表已经完成主动关机。

## 统一内核自检入口

命令格式为 `just test [架构] [构建模式]`，默认架构 `rv`、模式 `release`。该命令统一启用 Cargo feature `ktest`，经现有 Make 构建入口导出 ELF，再调用 QEMU；`MEM`、`SMP` 环境变量与 `just run` 一致。一次运行执行所选架构的全部已接入测试；验证双架构时分别运行两个 QEMU 实例。

`ktest` 只控制测试代码，不控制 trap 等正式能力。共享入口先正常初始化内核，再调用 `os/src/ktest/mod.rs` 中的 `run()`；默认构建不编译该模块和各架构的 trap 测试探针。这里使用 `#[cfg(feature = "ktest")]`，不是 Rust 的 `#[cfg(test)]` 或自定义 `#[cfg(ktest)]`，也不依赖 Cargo 测试 harness。

新增自检时，将测试代码放在所属模块的 `tests/` 子目录，由父模块用同一个 `ktest` feature 控制；仅供自检使用的汇编也放在该子目录。然后在 `ktest/mod.rs` 的 `SUITES` 列表中增加一个 `Suite { name, run }`，其中 `run` 是无参数、无返回值的测试组函数。列表顺序就是执行顺序；通用执行循环在调用前输出测试组名称，只有所有测试组返回后才输出总成功标记。无需修改 `kernel_main` 或执行循环，不新增测试专用 feature 或 Just recipe。测试依赖的基础能力应由正常初始化提供，不能把另一组自检当作初始化入口。

测试在启动 CPU 上关闭中断、依次执行，每组测试返回前必须恢复临时映射、机器状态并释放测试资源。失败直接走现有 panic 停机，不捕获后继续执行；故意触发致命异常或无法恢复状态的测试应在独立 QEMU 实例中验证，不加入这条顺序执行路径。

## Trap 内核测试

在开发容器内运行：

```sh
just test
just test la
```

两者分别构建启用 `ktest` 的 RISC-V / LoongArch release ELF，再启动 QEMU。trap 内核测试完成后输出 `<架构>: trap kernel tests passed`；只有全部测试完成，统一入口才输出 `<架构>: all kernel tests passed`。断言失败走 panic，不输出总成功标记。LoongArch 成功后仍停放，需 `Ctrl-a`、`x` 退出；自动运行时的超时本身既不是成功也不是失败，必须检查串口的总成功标记。

验证内容：

- 内核 breakpoint 的整数上下文保存/恢复，以及处理期间的同步嵌套异常。LoongArch handler 修改全部非零、非 SP GPR 后，汇编逐个检查恢复值和原始 SP，再恢复探针调用者，避免探针自身恢复掩盖 trap 恢复错误。
- 用户态 `ecall` / `syscall` 返回内核，验证所有整数寄存器、异常 PC、原因和先前特权级。
- 修改用户 PC、整数寄存器和栈指针后重新进入，确认恢复的是修改后的上下文。
- 独立汇编包装经 C 调用边界执行 Rust `run_user`，在首次进入和修改现场后重入时检查内核调用 ABI。RISC-V 检查 s0–s11、有效的原始 gp/tp 和 SP；LoongArch 检查 fp/s0–s8、原始 tp/r21 和 SP。检查先于包装器自身恢复，返回控制流证明能够回到调用者，不要求 caller-saved 寄存器保持原值。
- 输入恶意特权状态位仍只能进入 U/PLV3；实际执行特权指令必须异常。
- 浮点关闭时执行浮点指令必须异常，而不是静默破坏未保存状态。
- LoongArch 内核嵌套 refill、用户未映射取指及其后的正常 syscall；临时 DMW 修改同步失效缓存翻译。
- 原因解码覆盖中断/异常编号空间分离、未知编号保留；LoongArch 额外覆盖多位待处理中断、异常子码及 refill 标记优先于旧 ESTAT。解码断言直接接入 `ktest`，不依赖禁用的 Cargo 测试 harness。

debug 模式直接使用 `just test rv debug` 或 `just test la debug`。自检映射只供固定可信探针使用，不提供进程隔离。恢复正常产物使用 `just build`；默认构建不启用自检。

致命诊断应在独立 QEMU 实例中验证，不能加入顺序自检。可在 debug 内核安装 trap 后用 GDB 将 PC 改为零，分别观察 RISC-V instruction access fault 和 LoongArch TLB refill 的 panic 输出及停机路径；默认日志级别下也必须能看到原因、PC、保存状态、原始原因、故障地址和原始 SP。

LoongArch 的 Rust 可调用汇编函数提供 ELF `FUNC` 类型和函数大小。构建 trap 内核测试后，可用 `readelf -Ws kernel-la` 检查 `__loongarch_run_user`、`__loongarch_kernel_test_kernel`、`__loongarch_kernel_test_nested`、`__loongarch_kernel_test_refill` 的类型及范围；GDB 中 `disassemble __loongarch_run_user` 应止于恢复路径的 `ertn`，不包含后续异常入口前的对齐填充。

## Heap 内核测试

在容器内执行 `just test rv` 或 `just test la`，统一入口会在 trap 测试后执行 heap 测试。该测试验证 `Box` 的页对齐、`Vec` 扩容与数据保持、超过堆总容量的 `try_reserve_exact` 返回错误，以及对象销毁和反复申请后全局堆实际占用恢复。

布局和耗尽检查使用单独的、按 8 KiB 对齐的 8 KiB 静态内存池，不耗尽全局堆。测试检查不同大小与对齐的分配互不覆盖、写入数据保持、满堆返回失败、释放后的块复用，以及两个 4 KiB 伙伴块释放后能够满足 8 KiB 申请。全部通过后输出 `<架构>: heap kernel tests passed`，最后仍以统一入口的总成功标记为准。

独立堆耗尽检查不触发全局分配失败处理；实际不可恢复的分配失败会 panic 停机，必须在独立 QEMU 实例中验证，不能加入顺序自检。

## 启用自检时的静态检查

特性分支的静态检查需显式传入功能：

```sh
make prepare-vendor
cargo clippy --config cargo/config.toml --locked --offline -p impl-os --target riscv64gc-unknown-none-elf --features ktest -- -D warnings
cargo clippy --config cargo/config.toml --locked --offline -p impl-os --target loongarch64-unknown-none --features ktest -- -D warnings
```
