//! 单核、禁用中断的监管态与用户态整数上下文切换。
//!
//! 执行内核代码时 `sscratch` 为零；执行用户代码时，它指向内核拥有的
//! 切换栈帧。不保存浮点和向量状态：FS/VS/XS 始终关闭，
//! 因此使用这些扩展会显式触发陷阱。

use core::arch::{asm, global_asm};

pub mod cause;
use cause::TrapCause;

const USER_STATUS: usize = 2 << 32; // UXL=64；SPP、SPIE、SIE、FS、VS、XS 均为零。
const DISABLED_STATUS: usize = (1 << 1) | (1 << 5) | (3 << 9) | (3 << 13) | (3 << 15);
const SWITCH_FRAME_SIZE: usize = 18 * core::mem::size_of::<usize>();

/// 整数寄存器与监管态陷阱 CSR，按架构 x 寄存器编号索引。
///
/// `gpr[0]` 始终保存为零；`gpr[2]` 是被中断时的栈指针。
/// 私有 CSR 字段保留 `sepc`、`sstatus`、`scause`、`stval`
/// 的硬件含义。调用者只能修改整数寄存器和程序计数器。
/// 内核陷阱返回时强制使用监管态，并保持中断和扩展关闭；
/// 进入用户态时替换传入状态中的所有控制位。
#[repr(C)]
pub struct TrapContext {
    gpr: [usize; 32],
    sepc: usize,
    sstatus: usize,
    scause: usize,
    stval: usize,
}

// 在编译期校验 Rust 现场布局与汇编栈帧约定。
const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, gpr) == 0);
    assert!(core::mem::size_of::<TrapContext>() == 288);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
    // 将上下文的对齐要求固定为机器字对齐（RV64 为 8 字节），
    // 因而汇编按 16 字节对齐的栈上现场满足 Rust 引用的对齐要求；
    // 若以后引入更严格的对齐要求，必须同时审查汇编的栈帧分配。
    assert!(core::mem::align_of::<TrapContext>() == core::mem::align_of::<usize>());
    // addi 的立即数是有符号 12 位数，范围为 -2048..=2047。
    // 入口既用 -context_size 分配现场，又用 +context_size 计算原始 sp，
    // 因此大小必须不超过正向上界 2047，而不能只满足负向的 2048。
    assert!(core::mem::size_of::<TrapContext>() <= 2047);
    // ld/sd 的字段偏移同样使用有符号 12 位立即数；stval 是最后一个字段。
    assert!(core::mem::offset_of!(TrapContext, stval) <= 2047);
    // 用户态切换栈帧也必须保持 ABI 对齐，且正负栈调整都能用 addi 编码。
    assert!(SWITCH_FRAME_SIZE % 16 == 0 && SWITCH_FRAME_SIZE <= 2047);
};

global_asm!(
    include_str!("entry.S"),
    include_str!("return.S"),
    context_size = const core::mem::size_of::<TrapContext>(),
    gpr = const core::mem::offset_of!(TrapContext, gpr),
    word_size = const core::mem::size_of::<usize>(),
    pc = const core::mem::offset_of!(TrapContext, sepc),
    status = const core::mem::offset_of!(TrapContext, sstatus),
    cause = const core::mem::offset_of!(TrapContext, scause),
    bad_addr = const core::mem::offset_of!(TrapContext, stval),
    switch_size = const SWITCH_FRAME_SIZE,
    disabled_status = const DISABLED_STATUS,
    kernel_handler = sym kernel_handler,
);

extern "C" {
    fn riscv_trap_entry();
    fn riscv_run_user(context: *mut TrapContext);
}

extern "C" fn kernel_handler(context: &mut TrapContext) {
    crate::trap::handle_kernel(context);
}

#[allow(dead_code)] // 公开上下文接口；使用方按架构选用部分方法。
impl TrapContext {
    /// 构造关闭中断和扩展的 RV64 用户上下文。
    ///
    /// `entry` 必须指向用户可执行代码，且至少两字节对齐；
    /// `stack` 必须按 16 字节对齐，并界定可写的用户内存。
    /// 本构造函数不校验地址，也不提供内存隔离。
    #[allow(dead_code)] // 公开进入接口；默认启动流程不执行用户代码。
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut context = Self {
            gpr: [0; 32],
            sepc: entry,
            sstatus: USER_STATUS,
            scause: 0,
            stval: 0,
        };
        context.gpr[2] = stack;
        context
    }

    /// 按架构 x 寄存器编号索引整数寄存器，包含保存的 x0。
    pub fn registers(&self) -> &[usize; 32] {
        &self.gpr
    }

    /// 修改整数状态；硬件恢复时 x0 仍恒为零。
    pub fn registers_mut(&mut self) -> &mut [usize; 32] {
        &mut self.gpr
    }

    /// 保存的异常程序计数器（`sepc`），并非自动推进后的恢复地址。
    pub fn pc(&self) -> usize {
        self.sepc
    }

    /// 设置下一程序计数器；对齐与可执行内存有效性由调用者保证。
    pub fn set_pc(&mut self, pc: usize) {
        self.sepc = pc;
    }

    /// 被中断时的栈指针，即架构寄存器 x2。
    pub fn sp(&self) -> usize {
        self.gpr[2]
    }

    /// 解码 `scause`，不决定异常恢复策略。
    pub fn cause(&self) -> TrapCause {
        TrapCause::from_scause(self.scause)
    }

    /// 原始保存的 `scause`，包含最高位的中断标志。
    pub fn raw_cause(&self) -> usize {
        self.scause
    }

    /// 原始保存的 `sstatus`；返回路径会约束特权级和扩展控制位。
    pub fn raw_status(&self) -> usize {
        self.sstatus
    }

    /// 保存的 `stval`：错误地址或指令位；不可用时为零。
    pub fn bad_addr(&self) -> usize {
        self.stval
    }
}

/// 安装直接模式监管态向量，并初始化单核暂存寄存器。
///
/// 在启动核上以监管态调用一次，且须早于进入用户代码。
/// 中断源及浮点、向量和其他扩展状态保持关闭。
pub fn init() {
    // SAFETY: 启动阶段仅一个核执行 S 模式；对齐的汇编向量和内核栈
    // 已经映射，且初始化期间没有正在进行的用户态切换。
    unsafe {
        asm!(
            "csrc sstatus, {disabled}",
            "csrw sie, zero",
            "csrw sscratch, zero",
            "csrw stvec, {vector}",
            disabled = in(reg) DISABLED_STATUS,
            vector = in(reg) riscv_trap_entry as *const () as usize,
            options(nostack),
        );
    }
}

/// 进入用户态，在任意用户陷阱发生时返回并保存全部整数状态。
///
/// 恢复程序计数器和整数寄存器；传入的 `sstatus` 会被刻意
/// 替换为 RV64 用户态状态，绝不信任其特权级或扩展
/// 控制位。返回时陷阱 CSR 描述异常。内核被调用者保存寄存器、
/// gp/tp 和调用栈在往返切换中保持不变。
///
/// # Safety
/// 必须已在本核执行 `init`，且中断仍关闭，没有其他正在进行的
/// 用户态切换。上下文必须始终有效，并由调用者独占。
/// 调用者必须提供有效的用户可执行程序计数器和可访问的用户栈、数据
/// 内存，满足 ABI 对齐；如需本调用返回，还必须安排触发陷阱。
/// 裸地址模式假设固件 PMP 允许用户访问内存。本机制不提供
/// 内存隔离：用户代码不得覆盖内核代码、栈或数据。
#[allow(dead_code)] // 公开进入接口；默认启动流程不执行用户代码。
pub unsafe fn run_user(context: &mut TrapContext) {
    context.sstatus = USER_STATUS;
    // SAFETY: 调用者保证用户内存有效，且单核环境已初始化；
    // 汇编先保存内核 ABI 状态，再恢复用户寄存器。
    unsafe { riscv_run_user(context) };
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
