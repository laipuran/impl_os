//! Trap initialization and fatal kernel-exception policy.
//!
//! User exceptions return from the architecture's `run_user` with a saved frame;
//! they do not enter this kernel-exception handler.

use crate::arch::trap::{self, TrapContext};

/// Install the boot CPU's exception entry before entering user code.
pub fn init() {
    trap::init();
}

/// Handle an exception taken while already executing kernel code.
///
/// Interrupts stay disabled. There is no scheduler or recoverable kernel-fault
/// policy yet; unexpected traps report the saved machine state and halt via panic.
pub fn handle_kernel(context: &mut TrapContext) {
    #[cfg(feature = "trap-smoke")]
    if trap::handle_smoke_kernel(context) {
        return;
    }

    panic!(
        "kernel trap: pc={:#x}, status={:#x}, cause={:#x}, bad_addr={:#x}",
        context.pc, context.status, context.cause, context.bad_addr
    );
}
