# 代码结构与模块职责

## 构建与启动入口

- **构建链路**：日常开发使用的 `just build` 会调用根目录 `Makefile`（比赛评测入口为 `make all`）。`Makefile` 针对两个裸机 target 分别调用 Cargo 编译 `os` crate，并由 [`os/build.rs`](../../os/build.rs) 根据目标架构注入对应的链接脚本（`linker.ld`）。
- **启动链路**：各架构的 `entry.S` 仅放行主核（hart 0 / core 0），将其余从核挂起；在设置好 64 KiB 启动栈后，将固件或 QEMU 传入的启动参数放入第一个参数寄存器，跳转至共享的 Rust 入口 `kernel_main(boot_arg)`。
- **运行模型**：当前内核运行在单核、关中断环境下。`kernel_main` 依次完成 BSS 清零、日志初始化、Trap 向量安装与内存管理初始化；若启用了 `ktest` feature 则运行内核自检，最后调用 `arch::shutdown()`（panic 路径则打印诊断信息并调用 `arch::halt()` 停机，绝不伪造正常关机）。

## 核心模块划分

| 模块 | 路径 | 职责说明 |
| --- | --- | --- |
| `arch` | `os/src/arch/` | 按编译目标导出统一的平台接口：早期串口输出、关机/停机、ELF 机器号与入口对齐约定、启动参数与内存发现（`boot`）、Trap 上下文与用户态切换（`trap`）、Linux 系统调用寄存器约定（`syscall`）。RISC-V 另提供 Sv39 页表（`paging`）。详见 [`os/src/arch/README.md`](../../os/src/arch/README.md)。 |
| `mm` | `os/src/mm/` | 内存管理：初始化 1 MiB 静态伙伴系统堆（`heap`），解析设备树内存信息（`fdt`），对 RAM 与保留区排序、合并、扣除并按页对齐（`regions`），最终构建基于位图的 4 KiB 物理页框分配器（`frame`）。 |
| `trap` | `os/src/trap.rs` | 共享的内核态异常处理入口 `handle_kernel`。用户态 Trap 直接从 `arch::trap::run_user` 返回给调用者；内核态未预期 Trap 则在此打印现场并 panic。 |
| `syscall` | `os/src/syscall.rs` | Linux 系统调用解码、分发与返回写回。目前已接入 `exit`（93）和 `exit_group`（94），其余调用号返回 `-ENOSYS`（-38）。 |
| `task` | `os/src/task.rs` | 最小用户任务封装 `Task`。持有一个 `TrapContext`，在当前内核栈上同步循环调用 `run_user` 执行用户态代码，直至任务调用 `exit` 或触发未处理异常。 |
| `task::elf` | `os/src/task/elf.rs` | 校验当前架构的 ELF64 可执行镜像，提供用户入口、程序头表元信息及借用原镜像数据的加载段描述。 |
| `task::user_stack` | `os/src/task/user_stack.rs` | 在调用方提供的字节缓冲区中构造 Linux 64 位初始栈，返回 16 字节对齐的用户栈指针。 |
| `ktest` | `os/src/ktest/` | 启动期内核白盒自检框架（仅在启用 `ktest` feature 时参与编译），详见 [内核自检与测试验证](../testing/README.md)。 |

## 核心调用流

```text
_start (entry.S)
  └─> kernel_main(boot_arg)
        ├─> clear_bss()
        ├─> utils::logging::init()
        ├─> trap::init()
        ├─> mm::init(boot_arg)
        │     ├─> heap::init()
        │     ├─> arch::boot::memory(boot_arg)
        │     │     └─> MappedFdt::from_raw() -> fdt::parse()
        │     ├─> discover() -> regions::MemoryMap::new()
        │     └─> frame::FrameAllocator::new()
        ├─> ktest::run()                 [仅 feature = "ktest"]
        └─> arch::shutdown()

Task::run()
  └─> loop {
        arch::trap::run_user(&mut ctx)   [降权进入 U-mode / PLV3，触发 Trap 后返回]
        syscall::decode(&ctx)
        syscall::dispatch(call)
        syscall::complete(&mut ctx, ret) [写回返回值并令 PC + 4]
      }

panic -> console::println! -> arch::halt()
```

各子系统的设计细节与当前边界见 [启动流程、内存管理与异常处理](boot.md)。
