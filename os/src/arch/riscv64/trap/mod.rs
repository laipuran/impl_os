//! 单 hart 的 S/U 态整数现场保存、可信内核栈切换和异常返回。
//!
//! 入口要求内核 sp 有效且按 16 字节对齐，并为现场
//! 和 Rust 处理函数留足空间。进入 Rust 前保存整数寄存器和 Trap CSR；
//! gp 恢复为内核基址，tp 恢复为零（内核不使用 TLS）。中断
//! 保持关闭。保存完成后的可处理同步异常允许嵌套，返回时恢复各层现场。
//! 内核执行时 sscratch=0，用户执行时指向可信内核栈顶。
//! 用户异常处理完成后直接返回 U 态，不返回 enter_user 的调用者。
//! 不支持异步中断或浮点/向量现场恢复；目前无用户地址空间隔离。

use core::arch::{asm, global_asm};

mod cause;

use cause::{ExceptionCause, TrapCause};

const USER_STATUS: usize = 2usize << 32; // UXL=64，其余用户返回控制位清零。
const DISABLED_STATUS: usize = (1 << 1) | (1 << 5) | (3 << 9) | (3 << 13) | (3 << 15);

/// 整数寄存器和 Trap CSR；handler 可修改通用寄存器和 sepc 后恢复执行。
/// `gpr[n]` 对应架构寄存器 xn；`gpr[2]` 保存被打断时的 sp。
#[repr(C, align(16))]
pub struct TrapFrame {
    gpr: [usize; 32],
    sepc: usize,
    sstatus: usize,
    scause: usize,
    stval: usize,
}

impl TrapFrame {
    /// 构造初始用户整数现场；不检查地址，也不提供内存隔离。
    #[allow(dead_code)] // 首次用户进入接口，默认启动暂不运行用户代码。
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut frame = Self {
            gpr: [0; 32],
            sepc: entry,
            sstatus: USER_STATUS,
            scause: 0,
            stval: 0,
        };
        frame.gpr[2] = stack;
        frame
    }

    fn cause(&self) -> TrapCause {
        TrapCause::from_scause(self.scause)
    }
}

const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::size_of::<TrapFrame>() % 16 == 0);
    // 入口使用带符号 12 位立即数的 ADDI 分配现场空间并计算原始 sp。
    assert!(core::mem::size_of::<TrapFrame>() < 2048);
};

global_asm!(
    include_str!("entry.S"),
    include_str!("return.S"),
    frame_size = const core::mem::size_of::<TrapFrame>(),
    register_size = const core::mem::size_of::<usize>(),
    gpr = const core::mem::offset_of!(TrapFrame, gpr),
    sepc = const core::mem::offset_of!(TrapFrame, sepc),
    sstatus = const core::mem::offset_of!(TrapFrame, sstatus),
    scause = const core::mem::offset_of!(TrapFrame, scause),
    stval = const core::mem::offset_of!(TrapFrame, stval),
    handler = sym handle_trap,
    user_status = const USER_STATUS,
    disabled_status = const DISABLED_STATUS,
);

/// 消费当前内核调用栈，首次进入 U 态；后续用户 Trap 直接处理并返回 U 态。
///
/// # Safety
/// 必须已在当前单 hart 初始化 Trap，且 S 态中断关闭。当前内核栈必须
/// 有效并为现场及嵌套 Rust handler 留足空间，其内存须在用户运行时保持有效。
/// 用户 PC 必须指向可执行且内核可读的指令；用户代码实际使用的栈/数据
/// 须可访问，调用遵守 ABI 的函数时栈须满足 ABI 对齐。
/// 当前没有内存隔离，只能运行不会破坏内核代码、栈和数据的可信用户程序；
/// Bare 模式还要求固件 PMP 允许用户访问所需 RAM。不保存浮点或向量状态。
#[allow(dead_code)]
pub unsafe fn enter_user(mut frame: TrapFrame) -> ! {
    extern "C" {
        fn __enter_user(frame: *mut TrapFrame) -> !;
    }
    frame.sstatus = USER_STATUS;
    // 安全性：调用者保证内存与运行环境有效；局部现场按 16 字节对齐，
    // 汇编接管当前调用栈，之后不再返回或访问这个 Rust 局部变量。
    unsafe { __enter_user(&raw mut frame) }
}

pub fn init() {
    extern "C" {
        fn __trap_entry();
    }
    // 安全性：启动 hart 处于 S 态且内核栈有效。入口
    // 按 4 字节对齐，选择 Direct 模式（stvec.MODE = 0）。保持 S 态
    // 中断关闭；同步异常不依赖 SIE。
    unsafe {
        asm!(
            "csrc sstatus, {disabled}",
            "csrw sie, zero",
            "csrw sscratch, zero",
            "csrw stvec, {entry}",
            disabled = in(reg) DISABLED_STATUS,
            entry = in(reg) __trap_entry as *const () as usize,
            options(nostack)
        );
    }
}

extern "C" fn handle_trap(frame: &mut TrapFrame) {
    let cause = frame.cause();
    let previous_mode = if frame.sstatus & (1 << 8) != 0 {
        "S"
    } else {
        "U"
    };
    log::trace!(
        "trap: from={} cause={} sepc={:#018x}",
        previous_mode,
        cause,
        frame.sepc,
    );
    log::debug!(
        "trap state: scause={:#018x} stval={:#018x} sstatus={:#018x} sp={:#018x}",
        frame.scause,
        frame.stval,
        frame.sstatus,
        frame.gpr[2],
    );

    match cause {
        TrapCause::Exception(ExceptionCause::Breakpoint) => {
            // 安全性：当前仅运行无地址空间隔离的可信软件断点，sepc 指向
            // 内核可读的指令内存。按半字读取，不要求四字节对齐。
            let instruction = unsafe { (frame.sepc as *const u16).read_unaligned() };
            let instruction_size = if instruction & 0b11 == 0b11 { 4 } else { 2 };
            frame.sepc = frame.sepc.wrapping_add(instruction_size);
        }
        _ => {
            log::error!(
                "unhandled trap: from={} cause={} sepc={:#018x}",
                previous_mode,
                cause,
                frame.sepc,
            );
            super::halt();
        }
    }
}
