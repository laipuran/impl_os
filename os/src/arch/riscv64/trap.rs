//! RISC-V 64 单核关中断环境下的 S-mode / U-mode 整数上下文切换。
//!
//! 寄存器与栈切换约定：
//! - 正常内核态执行期间，`sscratch` 恒保持为 `0`；
//! - 调用 [`run_user`] 进入用户态时，汇编在 `sret` 前将内核栈上的切换帧地址写入 `sscratch`
//!   （从写 `sscratch` 到 `sret` 完成前的短暂 S-mode 过渡窗口内 `sscratch` 非零，该窗口内的 `TrapContext` 访存绝不能触发异常）；
//! - 浮点、向量与扩展状态位（`FS`/`VS`/`XS`）始终保持关闭（`Off`），用户或内核执行扩展指令会触发非法指令异常。

use core::{
    arch::{asm, global_asm},
    fmt,
};
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        scause::Scause,
        sstatus::XLEN,
        stvec::{self, Stvec, TrapMode},
    },
};

// UXL = 64 位；SPP、SPIE、SIE、FS、VS、XS 均置 0。
const USER_STATUS: usize = XLEN::XLEN64.into_usize() << 32;
const DISABLED_STATUS: usize = (1 << 1) | (1 << 5) | (3 << 9) | (3 << 13) | (3 << 15);
const SWITCH_FRAME_SIZE: usize = 18 * core::mem::size_of::<usize>();

/// 保存 32 个通用寄存器（`x0..x31`）与 S-mode 异常 CSR 的上下文结构。
///
/// - `gpr[0]` 对应硬件零寄存器 `x0`（恒存为 0）；`gpr[2]` 为被打断时的栈指针 `sp`。
/// - `sepc`、`sstatus`、`scause`、`stval` 字段设为私有，仅对外暴露通用寄存器与 PC 的修改接口。
/// - 内核 Trap 返回时强制恢复为 S-mode 并关闭中断与扩展；[`run_user`] 进入用户态前也会强制重置 `sstatus`。
#[repr(C)]
pub struct TrapContext {
    gpr: [usize; 32],
    sepc: usize,
    sstatus: usize,
    scause: usize,
    stval: usize,
}

// 编译期静态断言：确保 `TrapContext` 的大小、字段偏移与对齐方式满足汇编代码的栈帧与 12 位有符号立即数约束。
const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, gpr) == 0);
    assert!(core::mem::size_of::<TrapContext>() == 288);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
    assert!(core::mem::align_of::<TrapContext>() == core::mem::align_of::<usize>());
    // RISC-V `addi` 指令立即数范围为 `-2048..=2047`；入口汇编既用 `-context_size` 分配栈帧，
    // 又用 `+context_size` 还原原始 `sp`，因此结构体大小不能超过 `2047`。
    assert!(core::mem::size_of::<TrapContext>() <= 2047);
    assert!(core::mem::offset_of!(TrapContext, stval) <= 2047);
    assert!(SWITCH_FRAME_SIZE % 16 == 0 && SWITCH_FRAME_SIZE <= 2047);
};

global_asm!(
    include_str!("trap/entry.S"),
    include_str!("trap/return.S"),
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

#[allow(dead_code)] // 统一的跨架构上下文接口，部分方法仅在特定路径或自检中调用。
impl TrapContext {
    /// 构造初始 U-mode 上下文（关闭中断与浮点/向量扩展）。
    ///
    /// `entry` 为用户程序入口地址（至少 2 字节对齐），`stack` 为 16 字节对齐的用户栈顶地址。
    /// 本构造函数仅初始化寄存器数值，不检查地址有效性，也不建立页表映射。
    #[allow(dead_code)]
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

    /// 返回按 `x0..x31` 排列的通用寄存器只读切片。
    pub fn registers(&self) -> &[usize; 32] {
        &self.gpr
    }

    /// 返回按 `x0..x31` 排列的通用寄存器可变切片（修改 `gpr[0]` 在硬件恢复时仍会被忽略）。
    pub fn registers_mut(&mut self) -> &mut [usize; 32] {
        &mut self.gpr
    }

    /// 返回捕获的异常程序计数器（`sepc`）。
    pub fn pc(&self) -> usize {
        self.sepc
    }

    /// 设置下次返回时的程序计数器（`sepc`）。
    ///
    /// 本方法仅更新上下文中保存的寄存器值，不检查地址对齐、映射、权限或可执行性；
    /// 若期望恢复后正常取指执行，调用方必须在恢复前保证目标地址满足指令对齐（至少 2 字节对齐）及对应特权级的访问与内存安全条件。
    pub fn set_pc(&mut self, pc: usize) {
        self.sepc = pc;
    }

    /// 返回被打断时的栈指针（`x2` / `sp`）。
    pub fn sp(&self) -> usize {
        self.gpr[2]
    }

    /// 将保存的 `scause` 解码为标准 S-mode 中断或异常。
    ///
    /// # Errors
    ///
    /// 编码不属于标准枚举时，返回原始原因码及其中断或异常分类。
    pub fn cause(&self) -> Result<Trap<Interrupt, Exception>, Trap<usize, usize>> {
        let raw = Scause::from_bits(self.scause).cause();
        raw.try_into().map_err(|_| raw)
    }

    /// 返回用于内核 Trap 日志的原因描述。
    pub(crate) fn cause_for_log(&self) -> impl fmt::Display + '_ {
        CauseDisplay(self)
    }

    /// 判断保存的 Trap 是否为断点异常。
    #[cfg(feature = "ktest")]
    pub(crate) fn is_breakpoint(&self) -> bool {
        self.cause() == Ok(Trap::Exception(Exception::Breakpoint))
    }

    /// 返回原始的 `scause` 寄存器值（含最高位中断标志）。
    pub fn raw_cause(&self) -> usize {
        self.scause
    }

    /// 返回原始的 `sstatus` 寄存器值。
    pub fn raw_status(&self) -> usize {
        self.sstatus
    }

    /// 返回保存的 `stval` 寄存器值（故障地址或非法指令编码，无意义时为 0）。
    pub fn bad_addr(&self) -> usize {
        self.stval
    }
}

struct CauseDisplay<'a>(&'a TrapContext);

impl fmt::Display for CauseDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.cause() {
            Ok(Trap::Interrupt(cause)) => write!(f, "interrupt: {cause:?}"),
            Ok(Trap::Exception(cause)) => write!(f, "exception: {cause:?}"),
            Err(Trap::Interrupt(code)) => write!(f, "interrupt: unknown interrupt code {code}"),
            Err(Trap::Exception(code)) => write!(f, "exception: unknown exception code {code}"),
        }
    }
}

/// 安装 S-mode Direct 模式异常入口（`stvec`），关闭中断与扩展并清零 `sscratch`。
///
/// 必须在主核启动阶段调用一次，且早于任何用户态切换。
pub fn init() {
    // SAFETY: 仅启动主核在 S-mode 下执行，此时没有活跃的用户态现场。
    unsafe {
        asm!(
            "csrc sstatus, {disabled}",
            "csrw sie, zero",
            "csrw sscratch, zero",
            disabled = in(reg) DISABLED_STATUS,
            options(nostack),
        );
    }

    let vector = riscv_trap_entry as *const () as usize;
    let stvec = Stvec::new(vector, TrapMode::Direct);
    // SAFETY: 中断已关闭，`sscratch` 已清零；`Stvec::new` 已校验 4 字节对齐，入口位于常驻内核代码段。
    unsafe { stvec::write(stvec) };
}

/// 降权进入 U-mode 执行 `context`，当用户态触发任何 Trap 时保存完整整数现场并返回。
///
/// 进入用户态前会将 `context.sstatus` 强制覆写为 [`USER_STATUS`]，防止外部传入非法的特权级或扩展使能位。
/// 往返过程中内核的被调用者保存寄存器（`s0..s11`）、`ra`、`gp`、`tp` 与 `sp` 均保持不变。
/// 如需本调用返回，须安排用户态触发 Trap。
///
/// # Safety
///
/// - 调用前当前核必须已在 S-mode 执行 [`init`]，且保持单核、关中断、无其他活跃的用户态切换。
/// - `context` 在整个切换与 Trap 往返期间必须持续有效、可读可写且由当前调用独占（尤其在发布 `sscratch` 后至 `sret` 前的过渡窗口内读取 `context` 绝不能触发异常）。
/// - 调用方须提供对齐的用户入口 PC 和符合 ABI 的栈，并将用户访问限制在其拥有的内存内。使用分页时，可以故意触发未映射或权限不足的用户页故障；内核代码、栈、数据、现场及页表须始终受到保护。
/// - 本函数只切换特权级。调用方须激活所需的页表；在依赖固件 PMP 的裸地址模式下，须使用不会修改内核存储的可信代码。
#[allow(dead_code)]
pub unsafe fn run_user(context: &mut TrapContext) {
    context.sstatus = USER_STATUS;
    // SAFETY: 调用方保证单核关中断、`context` 持续有效且独占，并保护切换所需的内核存储；汇编先在内核栈保存调用现场，再加载 `context` 执行 `sret`。
    unsafe { riscv_run_user(context) };
}

#[cfg(feature = "ktest")]
#[path = "../../ktest/arch/riscv64.rs"]
pub(crate) mod tests;
