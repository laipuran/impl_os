//! Single-CPU integer-only exception handling. SAVE0..SAVE3 are reserved here.
//!
//! Interrupts and floating-point/vector execution stay disabled. Production
//! code must supply its own user mappings; the boot DMW is PLV0-only.

use core::arch::{asm, global_asm};

/// Integer registers and exception CSRs captured before entering Rust.
///
/// Register indices are architectural numbers (r0..r31); r3 is the stack
/// pointer and r2 is the thread pointer. `status` contains PRMD, not CRMD.
/// Kernel handlers may edit registers and PC, but must leave cause unchanged
/// and retain PLV0 with PIE clear. FP, LSX and LASX are unsupported.
///
/// `cause` is raw ESTAT for ordinary traps. Bit 63 additionally marks a TLB
/// refill (ESTAT.Ecode is stale in that case); PC/bad_addr then come from
/// TLBRERA/TLBRBADV and status normalizes TLBRPRMD.PWE into PRMD bit 3.
/// Refill faults are reported, not resolved by walking page tables.
#[repr(C)]
pub struct TrapContext {
    pub regs: [usize; 32],
    pub pc: usize,
    pub status: usize,
    pub cause: usize,
    pub bad_addr: usize,
}

impl TrapContext {
    /// Construct a PLV3 context with interrupts and extension state disabled.
    ///
    /// `entry` must be a four-byte-aligned user-executable address; `stack`
    /// must be a 16-byte-aligned writable user stack top. Neither is mapped
    /// or validated here, and this mechanism provides no memory isolation.
    #[allow(dead_code)] // Public execution API; boot only uses it in trap-smoke.
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut regs = [0; 32];
        regs[3] = stack;
        Self {
            regs,
            pc: entry,
            status: 3,
            cause: 0,
            bad_addr: 0,
        }
    }
}

// Every literal GPR slot in trap.S is eight bytes, beginning at offset zero.
// CSR offsets and total frame size are substituted directly into assembly.
const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, regs) == 0);
    assert!(core::mem::size_of::<[usize; 32]>() == 256);
    assert!(core::mem::size_of::<TrapContext>() == 288);
};

global_asm!(
    include_str!("trap.S"),
    frame_size = const core::mem::size_of::<TrapContext>(),
    pc = const core::mem::offset_of!(TrapContext, pc),
    status = const core::mem::offset_of!(TrapContext, status),
    cause = const core::mem::offset_of!(TrapContext, cause),
    bad_addr = const core::mem::offset_of!(TrapContext, bad_addr),
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
/// PRMD is overwritten with PPLV=3, PIE=0, PWE=0 even if the caller edited it;
/// privileged return state therefore cannot leak through a supplied context.
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
#[allow(dead_code)] // Public execution API; boot only uses it in trap-smoke.
pub unsafe fn run_user(context: &mut TrapContext) {
    context.status = 3;
    // SAFETY: the caller guarantees mapped state and exclusive single-CPU
    // execution; assembly saves the kernel ABI frame before changing privilege.
    unsafe { __loongarch_run_user(context) };
}

#[no_mangle]
extern "C" fn __loongarch_handle_kernel(context: &mut TrapContext) {
    crate::trap::handle_kernel(context);
}

#[cfg(feature = "trap-smoke")]
#[path = "trap_smoke.rs"]
mod smoke;
#[cfg(feature = "trap-smoke")]
pub use smoke::{handle_smoke_kernel, smoke_test};
