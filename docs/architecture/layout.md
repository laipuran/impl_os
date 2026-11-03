# 当前代码结构

## 构建关系

```text
Just 开发命令 → Make → Cargo workspace → os/build.rs 选择链接脚本
比赛 make all ──┘
```

workspace 只有 `os` 一个 crate。`build.rs` 按 target 选择对应架构的链接脚本；Rust 使用 `cfg(target_arch)` 选择架构模块。

## 内核源码

```text
os/src/
  main.rs
  console.rs
  trap.rs
  ktest/
    mod.rs
  arch/
    mod.rs
    riscv64/
      mod.rs
      entry.S
      trap/
        mod.rs
        cause.rs
        entry.S
        tests/
          mod.rs
          probes.S
      linker.ld
    loongarch64/
      mod.rs
      entry.S
      trap/
        mod.rs
        cause.rs
        entry.S
        tests/
          mod.rs
          probes.S
      linker.ld
```

| 路径 | 当前职责 |
| --- | --- |
| `main.rs` | Rust 入口、BSS 清零、日志及 trap 初始化、启动输出 |
| `console.rs` | 通过 `core::fmt::Write` 格式化文本，逐字节调用架构串口输出 |
| `arch/mod.rs` | 按编译目标导出平台操作及 `trap` 接口 |
| `arch/*/entry.S` | CPU 初始设置、停放非启动 CPU、设置 64 KiB 启动栈、进入 Rust |
| `arch/*/linker.ld` | ELF 入口、代码/数据加载段、启动栈和 BSS 边界 |
| `arch/*/mod.rs` | 嵌入入口汇编、访问平台 UART、提供停机操作 |
| `trap.rs` | 共享 trap 初始化与致命内核异常策略 |
| `ktest/mod.rs` | 仅在 `ktest` 构建中声明具名测试组列表，按顺序执行并在全部完成后输出总成功标记 |
| `arch/*/trap/mod.rs`、`entry.S` | 私有整数现场、受控访问接口及 CSR 初始化；汇编按 Rust 字段偏移、字长和架构寄存器编号保存/恢复现场并切换特权级 |
| `arch/*/trap/cause.rs` | 架构相关的中断/异常解码及显示，保留未知原因，不决定处理策略 |
| `arch/*/trap/tests/mod.rs`、`probes.S` | `ktest` 功能控制的实际异常和特权级往返自检，与正式实现分目录存放 |

## 执行关系

```text
_start → kernel_main → clear_bss
                    → utils::logging::init
                    → trap::init → arch::trap::init
                    → console → arch::putchar
                    → ktest::run → SUITES → arch::trap::tests::smoke_test（仅 ktest）
                    → arch::shutdown
panic → console → arch::halt
```

两架构共用 Rust 入口和格式化输出，各自处理启动地址、CPU 状态、MMIO 地址及停机机制。控制台没有锁，非启动 CPU 被停放，所有 Rust 启动代码由单个 CPU 执行。

构建目标和开发命令见[任务分工](../development/targets.md)，地址与启动细节见[启动流程](boot.md)。
