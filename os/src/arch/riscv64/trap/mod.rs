//! Single-hart S-mode trap diagnostics on the interrupted kernel stack.
//!
//! Entry requires a valid, 16-byte-aligned kernel sp with room for the frame
//! and Rust handler. Integer registers and trap CSRs are captured before Rust;
//! gp is restored to the kernel base and tp to zero (no kernel TLS). Interrupts
//! stay disabled. There is no user stack switch, nested-fault recovery or return.

use core::arch::{asm, global_asm};

mod cause;

use cause::Exception;

/// Integer-only diagnostic snapshot; no user stack switching or resume yet.
/// `gpr[n]` is architectural register xn; gpr[2] holds the interrupted sp.
#[repr(C)]
struct TrapFrame {
    gpr: [usize; 32],
    sepc: usize,
    sstatus: usize,
    scause: usize,
    stval: usize,
}

const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::size_of::<TrapFrame>() % 16 == 0);
    // Entry allocates the frame and recovers sp using signed 12-bit ADDI.
    assert!(core::mem::size_of::<TrapFrame>() < 2048);
};

global_asm!(
    include_str!("entry.S"),
    frame_size = const core::mem::size_of::<TrapFrame>(),
    register_size = const core::mem::size_of::<usize>(),
    gpr = const core::mem::offset_of!(TrapFrame, gpr),
    sepc = const core::mem::offset_of!(TrapFrame, sepc),
    sstatus = const core::mem::offset_of!(TrapFrame, sstatus),
    scause = const core::mem::offset_of!(TrapFrame, scause),
    stval = const core::mem::offset_of!(TrapFrame, stval),
    handler = sym handle_trap,
);

pub fn init() {
    extern "C" {
        fn __trap_entry();
    }
    // SAFETY: the boot hart is in S-mode with a valid kernel stack. The entry
    // is 4-byte aligned, selecting Direct mode (stvec.MODE = 0). Keep S-mode
    // interrupts disabled; synchronous exceptions do not depend on SIE.
    unsafe {
        asm!(
            "csrci sstatus, 2",
            "csrw stvec, {entry}",
            entry = in(reg) __trap_entry as usize,
            options(nostack)
        );
    }
}

extern "C" fn handle_trap(frame: &TrapFrame) -> ! {
    let interrupt = frame.scause >> (usize::BITS - 1) != 0;
    let code = frame.scause & (usize::MAX >> 1);
    let reason = if interrupt {
        "unexpected interrupt"
    } else {
        Exception::try_from(code).map_or("unknown exception", Exception::description)
    };
    let previous_mode = if frame.sstatus & (1 << 8) != 0 {
        "S"
    } else {
        "U"
    };

    // Bypass the logger: a trap can happen before logger initialization.
    crate::println!("trap: {} (interrupt={}, code={})", reason, interrupt, code);
    crate::println!(
        "  sepc={:#018x} scause={:#018x} stval={:#018x}",
        frame.sepc,
        frame.scause,
        frame.stval
    );
    crate::println!(
        "  sstatus={:#018x} previous_mode={} sp={:#018x}",
        frame.sstatus,
        previous_mode,
        frame.gpr[2]
    );
    crate::println!("trap: halted (no recovery)");
    super::halt()
}
