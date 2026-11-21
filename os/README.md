# `impl-os` 内核 Crate

本目录是内核的 Rust 实现。请从仓库根目录执行 `make all` 或 `just` 命令进行编译，不要在此目录下单独执行无 target 的 `cargo build`。

## 代码概览

- `build.rs`：根据当前编译目标（`riscv64` 或 `loongarch64`）向链接器传入对应的 `linker.ld` 与 `--no-relax` 参数。
- `src/main.rs`：内核 Rust 入口 `kernel_main`。启动时依次清零 BSS、初始化日志与异常入口、初始化内核堆与物理内存分配器；若开启了 `ktest` 特性则运行内核自检，最后调用关机接口。
- `src/arch/`：架构相关实现，封装启动汇编、早期串口输出、关机/停机、ELF 架构约定、Trap 上下文保存恢复、系统调用寄存器约定及 RISC-V Sv39 页表（详见 [`src/arch/README.md`](src/arch/README.md)）。
- `src/mm/`：内存管理模块，包含基于静态 BSS 的伙伴系统堆（`heap`）、设备树内存解析（`fdt`）、物理地址区间整理（`regions`）以及基于位图的 4 KiB 物理页框分配器（`frame`）。
- `src/trap.rs`：共享的内核态异常处理入口。用户态触发的 Trap 会直接从 `arch::trap::run_user` 返回给调用方，而内核态意外触发的 Trap 会在此打印现场并 panic 停机。
- `src/syscall.rs` 与 `src/task.rs`：Linux 系统调用分发与最小任务执行循环。`Task` 封装单次同步的用户态执行上下文，目前支持 `exit`（93）和 `exit_group`（94），未实现的调用统一返回 `-ENOSYS`。
- `src/task/elf.rs` 与 `src/task/user_stack.rs`：纯安全解析不带程序解释器的用户态 ELF64 `ET_EXEC` 镜像、提取加载段描述，并在调用方提供的缓冲区中构造 Linux 64 位初始用户栈。
- `src/ktest/`：由 `ktest` feature 启用的启动期白盒自检套件，覆盖 Trap 上下文切换、系统调用 ABI、任务生命周期、ELF64 解析、初始用户栈布局、内核堆、设备树解析、物理页框分配与 RISC-V Sv39 页表。

当前仅 RISC-V 实现了 Sv39 页表并在自检中验证了用户态映射；正常启动仍不激活页表，通用用户地址空间、LoongArch 页表、ELF 映射与运行接入及多任务调度尚未实现，具体架构设计见 [`docs/architecture/`](../docs/architecture/layout.md)。
