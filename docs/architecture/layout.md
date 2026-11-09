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
  lang_items.rs
  console.rs
  trap.rs
  mm/
    mod.rs
    fdt/
      mod.rs
      header.rs
      tree.rs
      memory.rs
      wire.rs
      tests/
        mod.rs
        builder.rs
        valid.rs
        invalid.rs
    frame.rs
    frame/
      tests/
        mod.rs
    regions.rs
    regions/
      tests/
        mod.rs
    heap/
      mod.rs
      tests/
        mod.rs
  syscall.rs
  syscall/
    tests/
      mod.rs
      riscv64.S
      loongarch64.S
  task.rs
  task/
    tests/
      mod.rs
      riscv64.S
      loongarch64.S
  ktest/
    mod.rs
  utils.rs
  utils/
    logging.rs
  arch/
    mod.rs
    riscv64/
      mod.rs
      boot.rs
      entry.S
      syscall.rs
      trap/
        mod.rs
        cause.rs
        entry.S
        return.S
        tests/
          mod.rs
          probes.S
      linker.ld
    loongarch64/
      mod.rs
      boot.rs
      boot/
        tables.rs
        tests/
          mod.rs
      entry.S
      syscall.rs
      trap/
        mod.rs
        cause.rs
        entry.S
        return.S
        tests/
          mod.rs
          probes.S
      linker.ld
```

| 路径 | 当前职责 |
| --- | --- |
| `main.rs` | Rust 入口、BSS 清零、日志、trap 及内存管理初始化、启动输出 |
| `lang_items.rs` | panic 与不可恢复分配失败的处理；输出诊断后停机，不报告成功关机 |
| `console.rs` | 通过 `core::fmt::Write` 格式化文本，逐字节调用架构串口输出 |
| `utils.rs`、`utils/logging.rs` | 导出日志模块，注册串口日志器并设置启动日志级别 |
| `arch/mod.rs` | 按编译目标导出平台操作、启动信息和异常处理接口 |
| `arch/*/entry.S` | CPU 初始设置、停放非启动 CPU、设置 64 KiB 启动栈、进入 Rust |
| `arch/*/linker.ld` | ELF 入口、代码/数据加载段、启动栈和 BSS 边界 |
| `arch/*/mod.rs` | 嵌入入口汇编、访问平台 UART、提供停机操作 |
| `arch/*/boot.rs` | 解释平台启动参数，读取 FDT 和平台保留区，将内核链接边界转换为物理区间 |
| `arch/loongarch64/boot/tables.rs` | 按 GUID 解析 QEMU 直接加载 ELF 时提供的系统配置表 |
| `trap.rs` | 共享 trap 初始化与致命内核异常策略 |
| `mm/mod.rs` | 统一初始化内核堆、发现物理内存并返回页框分配器，封装内部初始化顺序 |
| `mm/heap/mod.rs` | 固定 1 MiB 静态 buddy 堆及全局分配器 |
| `mm/fdt/mod.rs` | FDT 读取和启动内存解析入口；只暴露原始内存描述与解析接口 |
| `mm/fdt/header.rs`、`wire.rs` | 校验 FDT 头、保留表和数据块布局，以及有边界检查的大端整数和字符串读取 |
| `mm/fdt/tree.rs`、`memory.rs` | 前者检查结构 token、属性顺序与递归深度，后者解释节点内存属性并收集 RAM、静态保留区和 initrd |
| `mm/fdt/tests/` | 构造器、正常输入及错误输入测试分别存放，经同一解析入口验证 |
| `mm/regions.rs` | 归一化 RAM/保留区并计算可用完整页，通过只读访问维护地图不变量 |
| `mm/frame.rs` | 位图页框分配器及独占句柄，按 4 KiB 分配并在句柄销毁时回收 |
| `mm/heap/tests/mod.rs` | 全局堆集合测试，以及独立内存池上的对齐、回收、耗尽和伙伴合并检查 |
| `syscall.rs` | 共享 Linux syscall ABI 解码、正常返回处理及最小调用分发；不访问用户内存 |
| `arch/*/syscall.rs` | 调用号、参数和返回值的寄存器位置，以及用户 syscall 来源判断 |
| `syscall/tests/` | 连续用户 syscall 探针及共享行为断言，在 trap 自检提供的可信用户执行环境中运行 |
| `task.rs` | 独占用户整数现场与生命周期，同步执行至退出或未处理的用户陷阱；不拥有地址空间或独立内核栈 |
| `task/tests/` | 未知 syscall 恢复、退出和用户故障的真实任务探针，复用 trap 自检的可信执行环境 |
| `ktest/mod.rs` | 仅在 `ktest` 构建中声明具名测试组列表，按顺序执行并在全部完成后输出总成功标记 |
| `arch/*/trap/mod.rs`、`entry.S`、`return.S` | 私有整数现场、受控访问接口及 CSR 初始化；`entry.S` 保存异常现场并进入处理函数，`return.S` 恢复现场并负责用户态进入与返回；偏移由 Rust 字段布局和寄存器字长提供 |
| `arch/*/trap/cause.rs` | 架构相关的中断/异常解码，使用 `thiserror` 在枚举变体上声明显示描述，保留未知原因，不决定处理策略 |
| `arch/*/trap/tests/mod.rs`、`probes.S` | `ktest` 功能控制的实际异常和特权级往返自检，与正式实现分目录存放 |

## 执行关系

```text
_start → kernel_main → clear_bss
                    → utils::logging::init
                    → trap::init → arch::trap::init
                    → mm::init(boot_arg) → heap::init
                                        → discover → arch::boot::memory / kernel_region
                                                   → fdt / regions → 物理内存地图
                                        → FrameAllocator::new → 返回页框分配器
                    → console → arch::putchar
                    → ktest::run → SUITES → 各模块自检（仅 ktest）
                    → arch::shutdown
panic → console → arch::halt
Box / Vec → 全局 LockedHeap → 静态堆空间
```

两架构共用 Rust 入口和格式化输出，各自处理启动地址、CPU 状态、MMIO 地址及停机机制。控制台没有锁，非启动 CPU 被停放，所有 Rust 启动代码由单个 CPU 执行。

构建目标和开发命令见[任务分工](../development/targets.md)，地址与启动细节见[启动流程](boot.md)。
