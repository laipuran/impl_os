# 架构抽象层（`arch`）

`arch` 模块将硬件平台差异隔离在架构边界内，按当前编译 target（`riscv64` 或 `loongarch64`）向内核上层导出统一的接口。共享内核代码应始终通过 `crate::arch` 访问硬件能力，避免在业务模块中散落 `#[cfg(target_arch = ...)]`。

## 导出接口

[`arch.rs`](../arch.rs) 对外导出以下统一接口：

- 基础平台操作：`NAME`（架构名称字符串）、`putchar`（早期串口单字节输出）、`shutdown`（正常关机）、`halt`（死循环挂起当前核）。
- ELF 架构约定：`ELF_MACHINE`（ELF 文件头机器号：RISC-V 为 `243` / `EM_RISCV`，LoongArch 为 `258` / `EM_LOONGARCH`）与 `ELF_ENTRY_ALIGN`（程序入口指令对齐字节数：RISC-V 为 `2`，LoongArch 为 `4`）。
- 启动信息提取：`boot::memory(boot_arg)`（解析启动参数中的 RAM 与保留区）和 `boot::kernel_region()`（获取内核镜像占用的物理地址区间）。
- 系统调用寄存器约定：`syscall::{NUMBER_REGISTER, ARGUMENT_START, is_syscall}`。
- Trap 与特权级切换：`trap::{init, TrapContext, run_user}`。
- 用户分页：`user_paging` 导出当前目标的用户根页表类型（`UserRoot`，不含 `unmap`）、用户虚拟地址上界（`USER_END`）及物理页框到内核裸指针的转换（`frame_ptr`）；在 `ktest` 配置下另提供 `is_user_inactive` 用于核验地址转换恢复状态。
  目前仅 RISC-V 支持；`mm` 通过跨架构权限枚举 `Access` 调用该接口，不接触 Sv39 硬件权限位与 CSR 细节。
  底层 `PageTable` 仅在 RISC-V `ktest` 配置下经 `arch::paging` 重导出供白盒分页自检使用，常规内核代码不直接使用 `PageTable`。

## 子模块说明

- `user_paging.rs`：汇总当前架构的用户分页接口与物理页框地址转换，隔离共享内存层与具体 ISA。
- `riscv64.rs`：面向 QEMU `virt`（S-mode 启动），通过 OpenSBI 执行关机，直接用物理地址访问 16550 UART。
  该目录通过 `riscv` crate（关闭默认特性，启用 `s-mode`）封装 `wfi`、`fence.i`、`stvec` 写入、`satp` 读取与编码、`scause` 拆分及 `sstatus` 常量。
  在 `ktest` 配置下另用于读取活态 `sstatus` 与 `sscratch`。
- `riscv64/paging.rs`：`PageTable` 管理 Sv39 4 KiB 映射、软件查表、表页所有权和作用域激活；`riscv64/paging/sv39_user_root.rs` 的 `Sv39UserRoot` 负责用户根页表的平台映射、用户叶页映射与激活，同模块的 `frame_ptr` 在 Bare 下返回物理页框的同址裸指针，由 `mm` 清零和初始化；`riscv64/paging/pte.rs` 封装硬件编码，`riscv64/paging/csr.rs` 封装 `satp` 读取、切换前后的 `sfence.vma` 同步与回读确认。
- `loongarch64.rs`：面向 QEMU `virt` 直接加载 ELF 启动（PLV0），在入口汇编中配置 `DMW0`（`0x9000_0000_0000_0000`）高地址直接映射窗口，通过 QEMU 传入的 EFI 系统表定位设备树（FDT）与 initrd。
- `boot_data.rs`：双架构共用的 FDT 原始内存借用封装 `MappedFdt`。负责将“从裸指针验证头部与容量上限并构造切片”的 `unsafe` 操作集中在一处，随后交由纯安全的 `mm::fdt` 解析，并自动将 FDT 自身占用的物理区间加入保留列表。

## Trap 与上下文切换设计

两个架构的 Trap 模块文件布局一致：

- `trap.rs`：定义保存通用寄存器与关键异常 CSR 的 `TrapContext`，提供访问 PC、SP、通用寄存器和异常原因的统一方法。结构体字段保持私有，防止外部直接篡改特权级或中断状态位。
- `trap/cause.rs`：将架构异常寄存器（RISC-V 的 `scause`、LoongArch 的 `ESTAT` 及软件标记的 TLB 重填）解码为结构化的 `TrapCause` 枚举。
- `trap/entry.S` 与 `trap/return.S`：实现内核态 Trap 与用户态往返切换（`run_user`）。
  - 执行 `run_user` 时，先把当前内核调用栈的被调用者保存寄存器存入栈帧，再加载 `TrapContext` 降权进入用户态。
  - 用户态触发任何异常或系统调用时，汇编直接把用户寄存器写回调用方传入的 `TrapContext`，随后恢复内核被调用者保存寄存器并从 `run_user` 返回。
  - 内核态触发异常时，在当前内核栈上开辟对齐的 `TrapContext` 栈帧，调用 `crate::trap::handle_kernel`。

调用方负责在切换前准备并激活用户映射。RISC-V 的 `mm::address_space::AddressSpace` 使用架构层准备映射并作用域激活 Sv39；用户 Trap 往返沿用活动根，正常启动保持裸地址模式。寄存器约定和安全前提见 [`docs/architecture/boot.md`](../../../docs/architecture/boot.md)。
