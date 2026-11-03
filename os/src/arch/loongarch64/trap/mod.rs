//! Single-CPU integer-only exception handling. SAVE0..SAVE3 are reserved here.
//!
//! Interrupts and floating-point/vector execution stay disabled. Production
//! code must supply its own user mappings; the boot DMW is PLV0-only.

use core::arch::{asm, global_asm};

pub mod cause;
use cause::TrapCause;

/// Integer registers and exception CSRs captured before entering Rust.
///
/// GPR indices are architectural numbers (r0..r31); r3 is the stack pointer
/// and r2 is the thread pointer. Only GPRs and the exception PC are writable
/// through the public API. FP, LSX and LASX are unsupported.
///
/// Ordinary traps capture ERA, PRMD, ESTAT and BADV. Refill traps instead
/// capture TLBRERA/TLBRBADV, normalize TLBRPRMD.PWE into PRMD bit 3, and tag
/// ESTAT with bit 63 because ESTAT.Ecode is stale. Thus the saved `era`,
/// `prmd` and `estat` are normalized return state, not always raw CSR values.
/// Refill faults are reported, not resolved by walking page tables.
#[repr(C)]
pub struct TrapContext {
    gpr: [usize; 32],
    era: usize,
    prmd: usize,
    estat: usize,
    badv: usize,
}

impl TrapContext {
    /// Construct a PLV3 context with interrupts and extension state disabled.
    ///
    /// `entry` must be a four-byte-aligned user-executable address; `stack`
    /// must be a 16-byte-aligned writable user stack top. Neither is mapped
    /// or validated here, and this mechanism provides no memory isolation.
    #[allow(dead_code)] // Public execution API; boot only uses it in ktest.
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut gpr = [0; 32];
        gpr[3] = stack;
        Self {
            gpr,
            era: entry,
            prmd: 3,
            estat: 0,
            badv: 0,
        }
    }

    /// Return GPRs indexed by architectural register number.
    #[allow(dead_code)] // Public context inspection API.
    pub fn registers(&self) -> &[usize; 32] {
        &self.gpr
    }

    /// Edit GPRs; r0 remains hardwired to zero when execution resumes.
    #[allow(dead_code)] // Public execution API.
    pub fn registers_mut(&mut self) -> &mut [usize; 32] {
        &mut self.gpr
    }

    /// Return ERA, or the normalized TLBRERA PC for a refill.
    #[allow(dead_code)] // Public context inspection API.
    pub fn pc(&self) -> usize {
        self.era
    }

    /// Set the next exception-return PC without changing privilege state.
    #[allow(dead_code)] // Public execution API.
    pub fn set_pc(&mut self, pc: usize) {
        self.era = pc;
    }

    /// Return the saved architectural stack pointer (r3).
    #[allow(dead_code)] // Public context inspection API.
    pub fn sp(&self) -> usize {
        self.gpr[3]
    }

    /// Decode ESTAT, giving the synthetic refill tag priority over Ecode.
    #[allow(dead_code)] // Public context inspection API.
    pub fn cause(&self) -> TrapCause {
        TrapCause::from_estat(self.estat)
    }

    /// Return ESTAT with bit 63 set for a refill; not an unmodified CSR dump.
    #[allow(dead_code)] // Public context inspection API.
    pub fn raw_cause(&self) -> usize {
        self.estat
    }

    /// Return PRMD, or normalized TLBRPRMD with PWE moved to PRMD bit 3.
    #[allow(dead_code)] // Public context inspection API.
    pub fn raw_status(&self) -> usize {
        self.prmd
    }

    /// Return BADV, or TLBRBADV for a refill.
    #[allow(dead_code)] // Public context inspection API.
    pub fn bad_addr(&self) -> usize {
        self.badv
    }
}

// Context offsets feed every assembly GPR access; frames and addi.d/ld.d
// displacements must fit signed 12-bit immediates and keep the ABI alignment.
const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, gpr) % 8 == 0);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
    assert!(core::mem::size_of::<TrapContext>() <= 2047);
    assert!(core::mem::offset_of!(TrapContext, badv) <= 2047);
};

global_asm!(
    include_str!("entry.S"),
    frame_size = const core::mem::size_of::<TrapContext>(),
    gpr = const core::mem::offset_of!(TrapContext, gpr),
    word = const core::mem::size_of::<usize>(),
    era = const core::mem::offset_of!(TrapContext, era),
    prmd = const core::mem::offset_of!(TrapContext, prmd),
    estat = const core::mem::offset_of!(TrapContext, estat),
    badv = const core::mem::offset_of!(TrapContext, badv),
);

extern "C" {
    fn __loongarch_trap_entry();
    fn __loongarch_refill_entry();
    fn __loongarch_run_user(context: *mut TrapContext);
}

/// Install aligned direct/refill vectors and reserve SAVE0..SAVE3.
///
/// Call once at PLV0 with a valid kernel stack, before executing user code.
/// ECFG.VS and all interrupt enables are cleared; EUEN disables FP/LSX/LASX.
/// SAVE3 retains the kernel ABI thread pointer for Rust trap handlers.
pub fn init() {
    // SAFETY: early single-CPU kernel initialization owns these privileged
    // CSRs. The assembly vector is 4096-byte aligned and permanently mapped.
    unsafe {
        asm!(
            "li.w {tmp}, 4",
            "csrxchg $zero, {tmp}, 0x0",
            "csrwr $zero, 0x4",
            "csrwr $zero, 0x2",
            "csrwr $zero, 0x30",
            "csrwr $zero, 0x31",
            "csrwr $zero, 0x32",
            "move {tmp}, $tp",
            "csrwr {tmp}, 0x33",
            "csrwr {vector}, 0xc",
            "csrwr {refill}, 0x88",
            "csrwr $zero, 0x8a",
            "ibar 0",
            tmp = out(reg) _,
            vector = inout(reg) __loongarch_trap_entry as *const () as usize => _,
            refill = inout(reg) (
                __loongarch_refill_entry as *const () as usize - 0x9000_0000_0000_0000
            ) => _,
            options(nostack),
        );
    }
}

/// Enter PLV3 and return on any user exception with its complete integer state.
///
/// PRMD is overwritten with PPLV=3, PIE=0, PWE=0 even for a context containing
/// hostile saved state; privileged return state cannot leak into user execution.
/// The kernel stack, thread pointer and ABI callee-saved registers survive.
///
/// # Safety
/// Call at PLV0 on the initialized boot CPU with interrupts disabled. The
/// context must remain exclusively owned and mapped at PLV0 throughout the
/// transition. PC and stack must have instruction/ABI alignment; unmapped
/// accesses are reported as traps rather than resolved. The caller must
/// ensure user execution cannot overwrite kernel state (including this
/// context and the suspended kernel stack). No mappings
/// or memory isolation are created here. FP/vector instructions intentionally
/// trap because EUEN is zero; no extension register state is preserved.
#[allow(dead_code)] // Public execution API; boot only uses it in ktest.
pub unsafe fn run_user(context: &mut TrapContext) {
    context.prmd = 3;
    // SAFETY: the caller guarantees mapped state and exclusive single-CPU
    // execution; assembly saves the kernel ABI frame before changing privilege.
    unsafe { __loongarch_run_user(context) };
}

#[no_mangle]
extern "C" fn __loongarch_handle_kernel(context: &mut TrapContext) {
    crate::trap::handle_kernel(context);
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
