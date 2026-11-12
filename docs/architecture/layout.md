# 内核模块与执行关系

## 构建与入口

`just` 将内核构建交给 Make，Make 调用 workspace 中的 `os` crate；`os/build.rs` 按目标选择链接脚本，Rust 按目标选择架构实现。比赛构建入口为 `make all`。开发命令和目标见[任务分工](../development/targets.md)。

两架构的入口汇编设置启动 CPU 和栈，将平台启动参数传给 `kernel_main`。共享入口清零 BSS，初始化日志、trap 和内存管理，按配置运行自检后调用架构关机入口；LoongArch 当前仅停放 CPU。panic 输出诊断后停机，不报告成功关机。非启动 CPU 被停放，当前代码假设单 CPU、关闭中断；控制台不提供并发保护。

## 模块边界

- `arch` 按编译目标显式提供启动、串口、停机、trap 和 syscall ABI 接口。平台代码负责启动协议、原始地址访问和地址转换；各架构保留独立的寄存器现场与汇编约定。RISC-V 另提供 `paging::PageTable`，管理 Sv39 4 KiB 映射、软件查表和页表页所有权；`pte.rs` 封装硬件编码，`csr.rs` 封装作用域激活与 TLB 失效。详见[架构与平台边界](../../os/src/arch/README.md)。
- `mm` 负责初始化堆和物理内存。平台读取已映射启动存储并记录其占用；`mm::fdt` 只安全解析字节切片，不能保证原始地址可读。格式无关的 `BootMemory` 保存未归一化的 RAM 和保留区，`mm::regions` 建立内存地图不变量，`mm::frame` 管理可用页框及其独占句柄。
- `trap` 处理共享异常策略；架构代码保存和恢复整数现场。`syscall` 解码 Linux 调用 ABI，`task` 同步执行用户现场直至退出或故障，不提供调度或地址空间管理。
- `ktest` 是启动期自检的统一入口，按静态列表执行测试组；默认构建不包含自检。测试使用可信用户探针，并不提供用户隔离。Sv39 测试及探针位于 `os/src/ktest/arch/riscv64/paging/`，通过原模块的显式路径声明接入。测试组织、运行及机器状态约束见[开发检查](../testing/README.md)。

## 关键执行关系

```text
_start → kernel_main → clear_bss → 日志和 trap 初始化
                    → mm::init → heap::init
                               → arch::boot::memory → MappedFdt → fdt::parse
                               → discover → regions::MemoryMap → FrameAllocator
                    → ktest::run（仅 ktest）→ arch::shutdown
task → arch::trap::run_user → 用户陷阱返回 → syscall 解码与分发
panic → console → arch::halt
```

平台必须保证启动数据完整、可读且在解析期间不变；内存地图负责排序、合并、扣除和页对齐，分配器依赖其结果。详细启动顺序、内存保留、异常和架构限制见[启动流程](boot.md)。
