//! 在被打断的内核栈上执行单 hart 的 S 态 Trap 诊断。
//!
//! 入口要求内核 sp 有效且按 16 字节对齐，并为现场
//! 和 Rust 处理函数留足空间。进入 Rust 前保存整数寄存器和 Trap CSR；
//! gp 恢复为内核基址，tp 恢复为零（内核不使用 TLS）。中断
//! 保持关闭。不切换用户栈，不支持嵌套异常恢复或返回。

use core::arch::{asm, global_asm};

mod cause;

use cause::Exception;

/// 仅包含整数寄存器的诊断快照；尚不支持用户栈切换或恢复执行。
/// `gpr[n]` 对应架构寄存器 xn；`gpr[2]` 保存被打断时的 sp。
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
    // 入口使用带符号 12 位立即数的 ADDI 分配现场空间并计算原始 sp。
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
    // 安全性：启动 hart 处于 S 态且内核栈有效。入口
    // 按 4 字节对齐，选择 Direct 模式（stvec.MODE = 0）。保持 S 态
    // 中断关闭；同步异常不依赖 SIE。
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

    // 绕过日志器：Trap 可能发生在日志器初始化之前。
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
