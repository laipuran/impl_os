//! Single-hart supervisor/user integer context switching with interrupts disabled.
//!
//! `sscratch` is zero while executing kernel code and points at a kernel-owned
//! switch frame while executing user code. Floating-point/vector state is never
//! saved: FS/VS/XS stay Off, so attempts to use those extensions trap explicitly.

use core::arch::{asm, global_asm};

const USER_STATUS: usize = 2 << 32; // UXL=64; SPP, SPIE, SIE, FS, VS and XS are zero.
const DISABLED_STATUS: usize = (1 << 1) | (1 << 5) | (3 << 9) | (3 << 13) | (3 << 15);

/// Integer registers and supervisor trap CSRs, indexed by architectural x number.
///
/// `regs[0]` is always saved as zero; `regs[2]` is the interrupted stack pointer.
/// Kernel handlers may edit integer registers and PC. Return forces supervisor
/// privilege for kernel traps and keeps interrupts and extensions disabled.
#[repr(C)]
pub struct TrapContext {
    pub regs: [usize; 32],
    pub pc: usize,
    pub status: usize,
    pub cause: usize,
    pub bad_addr: usize,
}

const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, regs) == 0);
    assert!(core::mem::size_of::<TrapContext>() == 288);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
};

global_asm!(
    include_str!("trap.S"),
    context_size = const core::mem::size_of::<TrapContext>(),
    pc = const core::mem::offset_of!(TrapContext, pc),
    status = const core::mem::offset_of!(TrapContext, status),
    cause = const core::mem::offset_of!(TrapContext, cause),
    bad_addr = const core::mem::offset_of!(TrapContext, bad_addr),
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

impl TrapContext {
    /// Construct an RV64 user context with interrupts and extensions disabled.
    ///
    /// `entry` must address executable user-accessible code (at least two-byte
    /// aligned); `stack` must be 16-byte aligned and delimit writable user memory.
    /// This constructor does not validate addresses or provide memory isolation.
    #[allow(dead_code)] // Public entry API; the default boot does not run user code.
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut context = Self {
            regs: [0; 32],
            pc: entry,
            status: USER_STATUS,
            cause: 0,
            bad_addr: 0,
        };
        context.regs[2] = stack;
        context
    }
}

/// Install the direct supervisor vector and initialize the single-hart scratch.
///
/// Call once on the boot hart, in supervisor mode, before entering user code.
/// Interrupt sources and floating-point/vector/extension state remain disabled.
pub fn init() {
    // SAFETY: boot executes in S mode on one hart; the aligned assembly vector
    // and kernel stack are mapped, and no user transition is active during init.
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

/// Enter user mode and return on any user trap with all integer state saved.
///
/// The PC and integer registers are restored; incoming `status` is deliberately
/// replaced with RV64 U-mode status, never trusted for privilege or extension
/// control. Trap CSRs describe the exception on return. Kernel callee-saved
/// registers, gp/tp, and the calling stack survive the round trip.
///
/// # Safety
/// `init` must have run on this hart, with interrupts still disabled and no other
/// user transition active. The context must remain valid and exclusively owned.
/// The caller must supply valid executable user PC and accessible user stack/data
/// memory, with ABI alignment, and arrange a trap if it needs this call to return.
/// Bare-mode execution assumes firmware PMP permits user access to RAM. There is
/// no memory isolation: user code must not overwrite kernel code, stack, or data.
#[allow(dead_code)] // Public entry API; the default boot does not run user code.
pub unsafe fn run_user(context: &mut TrapContext) {
    context.status = USER_STATUS;
    // SAFETY: the caller guarantees the user memory and initialized single-hart
    // environment; assembly saves the kernel ABI before restoring user registers.
    unsafe { riscv_run_user(context) };
}

#[cfg(feature = "trap-smoke")]
#[path = "trap_smoke.rs"]
mod smoke;
#[cfg(feature = "trap-smoke")]
pub use smoke::{handle_smoke_kernel, smoke_test};
