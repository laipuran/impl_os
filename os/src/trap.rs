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
    #[cfg(feature = "ktest")]
    if trap::tests::handle_smoke_kernel(context) {
        return;
    }

    panic!(
        "kernel trap: {}: pc={:#x}, status={:#x}, raw_cause={:#x}, bad_addr={:#x}, sp={:#x}",
        context.cause(),
        context.pc(),
        context.raw_status(),
        context.raw_cause(),
        context.bad_addr(),
        context.sp(),
    );
}
