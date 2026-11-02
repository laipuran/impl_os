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
  lang_items.rs
  utils.rs
  utils/
    logging.rs
  arch/
    mod.rs
    riscv64/
      mod.rs
      entry.S
      linker.ld
      trap/
        mod.rs
        cause.rs
        entry.S
    loongarch64/
      mod.rs
      entry.S
      linker.ld
```

| 路径 | 当前职责 |
| --- | --- |
| `main.rs` | Rust 入口、BSS 清零、RISC-V Trap 与日志初始化、启动输出 |
| `lang_items.rs` | panic 输出与停放 |
| `utils/logging.rs` | 早期日志器 |
| `console.rs` | 通过 `core::fmt::Write` 格式化文本，逐字节调用架构串口输出 |
| `arch/mod.rs` | 按编译目标导出 `NAME`、`putchar`、`shutdown`、`halt` |
| `arch/*/entry.S` | CPU 初始设置、停放非启动 CPU、设置 64 KiB 启动栈、进入 Rust |
| `arch/*/linker.ld` | ELF 入口、代码/数据加载段、启动栈和 BSS 边界 |
| `arch/*/mod.rs` | 嵌入入口汇编、访问平台 UART、提供停机操作 |
| `arch/riscv64/trap/mod.rs` | 整数现场布局、Trap 初始化与致命诊断 |
| `arch/riscv64/trap/cause.rs` | 同步异常编号枚举、原始编号转换与诊断文本 |
| `arch/riscv64/trap/entry.S` | 在当前内核栈保存现场，重建内核 ABI 后调用 Rust handler |

## 执行关系

```text
_start → kernel_main → clear_bss
                    → arch::trap::init（仅 RISC-V）
                    → utils::logging::init
                    → console → arch::putchar
                    → arch::shutdown
RISC-V Trap → arch/riscv64/trap/entry.S → handle_trap → arch::halt
panic → console → arch::halt
```

两架构共用 Rust 入口和格式化输出，各自处理启动地址、CPU 状态、MMIO 地址及停机机制。控制台没有锁，非启动 CPU 被停放，所有 Rust 启动代码由单个 CPU 执行。

RISC-V Trap 目前仅诊断并停机，不切换用户栈或恢复执行。`TrapFrame` 的大小、字段偏移和寄存器字长由 Rust 传给汇编，汇编仅指定架构寄存器编号。入口要求有效且 16 字节对齐的内核栈；保存现场后重建内核 `gp` 和固定为零的 `tp`，再通过 `sym` 绑定调用 Rust。汇编入口声明函数类型和大小，供调试工具识别。

构建目标和开发命令见[任务分工](../development/targets.md)，地址与启动细节见[启动流程](boot.md)。
