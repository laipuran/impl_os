use core::arch::{asm, global_asm};

global_asm!(include_str!("trap.S"));

/// Integer-only diagnostic snapshot; no user stack switching or resume yet.
/// Slot offsets must match trap.S. x[2] holds the original kernel sp.
#[repr(C)]
struct TrapFrame {
    x: [usize; 32],
    sepc: usize,
    sstatus: usize,
    scause: usize,
    stval: usize,
}

const _: () = assert!(core::mem::size_of::<TrapFrame>() == 288);
const _: () = assert!(core::mem::offset_of!(TrapFrame, sepc) == 32 * 8);
const _: () = assert!(core::mem::offset_of!(TrapFrame, sstatus) == 33 * 8);
const _: () = assert!(core::mem::offset_of!(TrapFrame, scause) == 34 * 8);
const _: () = assert!(core::mem::offset_of!(TrapFrame, stval) == 35 * 8);

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

#[no_mangle]
extern "C" fn rust_trap_handler(frame: &TrapFrame) -> ! {
    let interrupt = frame.scause >> (usize::BITS - 1) != 0;
    let code = frame.scause & (usize::MAX >> 1);
    let reason = match (interrupt, code) {
        (false, 0) => "instruction address misaligned",
        (false, 1) => "instruction access fault",
        (false, 2) => "illegal instruction",
        (false, 3) => "breakpoint",
        (false, 4) => "load address misaligned",
        (false, 5) => "load access fault",
        (false, 6) => "store/AMO address misaligned",
        (false, 7) => "store/AMO access fault",
        (false, 8) => "environment call from U-mode",
        (false, 9) => "environment call from S-mode",
        (false, 12) => "instruction page fault",
        (false, 13) => "load page fault",
        (false, 15) => "store/AMO page fault",
        (true, _) => "unexpected interrupt",
        _ => "unknown exception",
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
        frame.x[2]
    );
    crate::println!("trap: halted (no recovery)");
    super::halt()
}
