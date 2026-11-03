//! 单 CPU 的 PLV0/PLV3 整数现场保存、可信内核栈切换和异常返回。
//!
//! SAVE0/1 暂存入口 t0/sp，SAVE2 保存用户执行期间的可信内核栈顶。
//! 按 PRMD.PPLV 选择栈，不依赖用户 sp；保存完成后允许可处理同步异常嵌套。
//! 用户异常处理后直接返回 PLV3，不返回 enter_user 的调用者。
//! TLB refill 由独立入口捕获和诊断，不填充页表/TLB。中断和扩展始终关闭，
//! 不包含调度或地址空间隔离。默认 DMW0 仅允许 PLV0，用户映射由调用者提供。

use core::arch::{asm, global_asm};

mod cause;

use cause::{ExceptionCause, TrapCause};

const USER_STATUS: usize = 3; // PRMD.PPLV=3，PIE/PWE=0。
const REFILL: usize = 1usize << 63;
const KERNEL_DIRECT_BASE: usize = 0x9000_0000_0000_0000;

/// 整数寄存器和异常 CSR；handler 可修改通用寄存器和 era 后恢复执行。
/// gpr[n] 对应 rn；r3 是 sp，r2 是 tp。prmd 保存先前状态，而不是 CRMD。
/// estat 保留原始 ESTAT，额外用 bit63 标记 TLB refill；此时 era/badv 来自
/// TLBRERA/TLBRBADV，prmd 将 TLBRPRMD.PWE 规范化到 PRMD 的 bit3。
#[repr(C, align(16))]
pub struct TrapFrame {
    gpr: [usize; 32],
    era: usize,
    prmd: usize,
    estat: usize,
    badv: usize,
}

impl TrapFrame {
    /// 构造初始用户整数现场；不检查地址，也不提供用户映射或内存隔离。
    #[allow(dead_code)]
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut frame = Self {
            gpr: [0; 32],
            era: entry,
            prmd: USER_STATUS,
            estat: 0,
            badv: 0,
        };
        frame.gpr[3] = stack;
        frame
    }

    fn cause(&self) -> TrapCause {
        TrapCause::from_estat(self.estat)
    }
}

const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::size_of::<TrapFrame>() % 16 == 0);
    assert!(core::mem::size_of::<TrapFrame>() < 2048);
};

global_asm!(
    include_str!("entry.S"),
    include_str!("return.S"),
    frame_size = const core::mem::size_of::<TrapFrame>(),
    register_size = const core::mem::size_of::<usize>(),
    gpr = const core::mem::offset_of!(TrapFrame, gpr),
    era = const core::mem::offset_of!(TrapFrame, era),
    prmd = const core::mem::offset_of!(TrapFrame, prmd),
    estat = const core::mem::offset_of!(TrapFrame, estat),
    badv = const core::mem::offset_of!(TrapFrame, badv),
    handler = sym handle_trap,
    user_status = const USER_STATUS,
    refill = const REFILL,
);

/// 消费当前内核调用栈首次进入 PLV3，后续用户异常直接处理并返回 PLV3。
///
/// # Safety
/// 当前单 CPU 必须已初始化 Trap，处于 PLV0 且中断关闭。内核栈须有效并
/// 为现场和嵌套 Rust handler 留足空间，用户运行期间不能回收或覆盖它。
/// 用户 PC 须四字节对齐且具有 PLV3 可执行映射，实际使用的栈/数据须有
/// 相应访问权限，调用遵守 ABI 的函数时栈须 16 字节对齐。
/// 调用者须提供映射；默认 PLV0 的 DMW0 不能直接作为用户映射。
/// 当前无内存隔离，只能运行不会破坏内核内存的可信用户程序。
/// 不保存浮点、LSX、LASX 等扩展现场。
#[allow(dead_code)]
pub unsafe fn enter_user(mut frame: TrapFrame) -> ! {
    extern "C" {
        fn __enter_user(frame: *mut TrapFrame) -> !;
    }
    frame.prmd = USER_STATUS;
    frame.estat = 0;
    // 安全性：调用者保证运行环境与映射有效，汇编接管对齐的局部现场和
    // 当前内核调用栈，之后不返回或再次访问此 Rust 局部变量。
    unsafe { __enter_user(&raw mut frame) }
}

pub fn init() {
    extern "C" {
        fn __trap_entry();
        fn __tlb_refill_entry();
    }
    // 安全性：启动 CPU 在 PLV0 独占这些 CSR。EENTRY 使用高地址 DMW，
    // TLBRENTRY 使用物理地址；两个入口都按 4096 字节对齐。
    unsafe {
        asm!(
            "li.w {tmp}, 4",
            "csrxchg $zero, {tmp}, 0x0",
            "csrwr $zero, 0x4", // ECFG：关闭中断源，VS=0。
            "csrwr $zero, 0x2", // EUEN：关闭浮点和向量等扩展。
            "csrwr $zero, 0x30",
            "csrwr $zero, 0x31",
            "csrwr $zero, 0x32",
            "csrwr {entry}, 0xc",
            "csrwr {refill}, 0x88",
            "csrwr $zero, 0x8a",
            "ibar 0",
            tmp = out(reg) _,
            entry = inout(reg) __trap_entry as *const () as usize => _,
            refill = inout(reg) (
                __tlb_refill_entry as *const () as usize - KERNEL_DIRECT_BASE
            ) => _,
            options(nostack),
        );
    }
}

extern "C" fn handle_trap(frame: &mut TrapFrame) {
    let cause = frame.cause();
    let previous_mode = frame.prmd & 3;
    log::trace!(
        "trap: from=PLV{} cause={} era={:#018x}",
        previous_mode,
        cause,
        frame.era,
    );
    log::debug!(
        "trap state: estat={:#018x} badv={:#018x} prmd={:#018x} sp={:#018x}",
        frame.estat,
        frame.badv,
        frame.prmd,
        frame.gpr[3],
    );
    match cause {
        TrapCause::Exception(ExceptionCause::Breakpoint) => {
            // LoongArch 指令定长四字节，不需要读取用户指令内存。
            frame.era = frame.era.wrapping_add(4);
        }
        _ => {
            log::error!(
                "unhandled trap: from=PLV{} cause={} era={:#018x} badv={:#018x}",
                previous_mode,
                cause,
                frame.era,
                frame.badv,
            );
            super::halt();
        }
    }
}
