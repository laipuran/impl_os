//! Single-hart supervisor/user integer context switching with interrupts disabled.
//!
//! `sscratch` is zero while executing kernel code and points at a kernel-owned
//! switch frame while executing user code. Floating-point/vector state is never
//! saved: FS/VS/XS stay Off, so attempts to use those extensions trap explicitly.

use core::arch::{asm, global_asm};

pub mod cause;
use cause::TrapCause;

const USER_STATUS: usize = 2 << 32; // UXL=64; SPP, SPIE, SIE, FS, VS and XS are zero.
const DISABLED_STATUS: usize = (1 << 1) | (1 << 5) | (3 << 9) | (3 << 13) | (3 << 15);
const SWITCH_FRAME_SIZE: usize = 18 * core::mem::size_of::<usize>();

/// Integer registers and supervisor trap CSRs, indexed by architectural x number.
///
/// `gpr[0]` is always saved as zero; `gpr[2]` is the interrupted stack pointer.
/// The private CSR fields retain their hardware meanings: `sepc`, `sstatus`,
/// `scause`, and `stval`. Callers may edit only integer registers and PC.
/// Return forces supervisor privilege for kernel traps and keeps interrupts and
/// extensions disabled; user entry replaces all incoming status control bits.
#[repr(C)]
pub struct TrapContext {
    gpr: [usize; 32],
    sepc: usize,
    sstatus: usize,
    scause: usize,
    stval: usize,
}

const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, gpr) == 0);
    assert!(core::mem::size_of::<TrapContext>() == 288);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
    assert!(core::mem::align_of::<TrapContext>() == core::mem::align_of::<usize>());
    assert!(core::mem::size_of::<TrapContext>() <= 2047);
    assert!(core::mem::offset_of!(TrapContext, stval) <= 2047);
    assert!(SWITCH_FRAME_SIZE % 16 == 0 && SWITCH_FRAME_SIZE <= 2047);
};

global_asm!(
    include_str!("entry.S"),
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

#[allow(dead_code)] // Public context API; consumers use architecture-specific subsets.
impl TrapContext {
    /// Construct an RV64 user context with interrupts and extensions disabled.
    ///
    /// `entry` must address executable user-accessible code (at least two-byte
    /// aligned); `stack` must be 16-byte aligned and delimit writable user memory.
    /// This constructor does not validate addresses or provide memory isolation.
    #[allow(dead_code)] // Public entry API; the default boot does not run user code.
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

    /// Integer registers indexed by architectural x number, including saved x0.
    pub fn registers(&self) -> &[usize; 32] {
        &self.gpr
    }

    /// Edit integer state; x0 remains hardwired to zero on hardware restoration.
    pub fn registers_mut(&mut self) -> &mut [usize; 32] {
        &mut self.gpr
    }

    /// Saved exception PC (`sepc`), not an automatically advanced resume address.
    pub fn pc(&self) -> usize {
        self.sepc
    }

    /// Select the next PC; alignment and executable-memory validity are caller policy.
    pub fn set_pc(&mut self, pc: usize) {
        self.sepc = pc;
    }

    /// Interrupted stack pointer, architectural x2.
    pub fn sp(&self) -> usize {
        self.gpr[2]
    }

    /// Decode `scause` without choosing exception recovery policy.
    pub fn cause(&self) -> TrapCause {
        TrapCause::from_scause(self.scause)
    }

    /// Raw saved `scause`, including its high interrupt bit.
    pub fn raw_cause(&self) -> usize {
        self.scause
    }

    /// Raw saved `sstatus`; return paths sanitize privilege/extension control.
    pub fn raw_status(&self) -> usize {
        self.sstatus
    }

    /// Saved `stval`: a fault address or instruction bits, or zero when unavailable.
    pub fn bad_addr(&self) -> usize {
        self.stval
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
/// The PC and integer registers are restored; incoming `sstatus` is deliberately
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
    context.sstatus = USER_STATUS;
    // SAFETY: the caller guarantees the user memory and initialized single-hart
    // environment; assembly saves the kernel ABI before restoring user registers.
    unsafe { riscv_run_user(context) };
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
