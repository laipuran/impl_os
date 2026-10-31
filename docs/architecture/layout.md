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
  arch/
    mod.rs
    riscv64/
      mod.rs
      entry.S
      linker.ld
    loongarch64/
      mod.rs
      entry.S
      linker.ld
```

| 路径 | 当前职责 |
| --- | --- |
| `main.rs` | Rust 入口、BSS 清零、启动输出、panic 处理 |
| `console.rs` | 通过 `core::fmt::Write` 格式化文本，逐字节调用架构串口输出 |
| `arch/mod.rs` | 按编译目标导出 `NAME`、`putchar`、`shutdown`、`halt` |
| `arch/*/entry.S` | CPU 初始设置、停放非启动 CPU、设置 64 KiB 启动栈、进入 Rust |
| `arch/*/linker.ld` | ELF 入口、代码/数据加载段、启动栈和 BSS 边界 |
| `arch/*/mod.rs` | 嵌入入口汇编、访问平台 UART、提供停机操作 |

## 执行关系

```text
_start → kernel_main → clear_bss
                    → console → arch::putchar
                    → arch::shutdown
panic → console → arch::halt
```

两架构共用 Rust 入口和格式化输出，各自处理启动地址、CPU 状态、MMIO 地址及停机机制。控制台没有锁，非启动 CPU 被停放，所有 Rust 启动代码由单个 CPU 执行。

构建目标和开发命令见[任务分工](../development/targets.md)，地址与启动细节见[启动流程](boot.md)。
