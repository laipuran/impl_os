//! 单处理器、仅整数状态的异常处理；此处保留 SAVE0..SAVE3。
//!
//! 中断以及浮点、向量执行始终关闭。实际运行的代码必须
//! 自行提供用户映射；启动时的直接映射窗口仅允许 PLV0 访问。

use core::arch::{asm, global_asm};

pub mod cause;
use cause::TrapCause;

/// 进入 Rust 前捕获的整数寄存器和异常 CSR。
///
/// 通用寄存器按架构编号 r0..r31 索引；r3 为栈指针，
/// r2 为线程指针。公开接口仅允许修改通用寄存器和异常
/// 程序计数器。不支持浮点、LSX 和 LASX。
///
/// 普通陷阱捕获 ERA、PRMD、ESTAT 和 BADV。重填陷阱则
/// 捕获 TLBRERA/TLBRBADV，将 TLBRPRMD.PWE 规范化至 PRMD 位 3，
/// 并在 ESTAT 位 63 添加标记，因为此时 ESTAT.Ecode 已过期。因此保存的
/// `era`、`prmd`、`estat` 是规范化的返回状态，不一定是原始 CSR 值。
/// 仅报告重填异常，不通过遍历页表解决异常。
#[repr(C)]
pub struct TrapContext {
    gpr: [usize; 32],
    era: usize,
    prmd: usize,
    estat: usize,
    badv: usize,
}

impl TrapContext {
    /// 构造关闭中断和扩展状态的 PLV3 上下文。
    ///
    /// `entry` 必须为四字节对齐的用户可执行地址；`stack`
    /// 必须为 16 字节对齐的可写用户栈顶。此处不映射、
    /// 不校验这些地址，也不提供内存隔离。
    #[allow(dead_code)] // 公开执行接口；启动流程仅在内核测试中使用。
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

    /// 按架构寄存器编号返回通用寄存器。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn registers(&self) -> &[usize; 32] {
        &self.gpr
    }

    /// 修改通用寄存器；恢复执行时 r0 仍恒为零。
    #[allow(dead_code)] // 公开执行接口。
    pub fn registers_mut(&mut self) -> &mut [usize; 32] {
        &mut self.gpr
    }

    /// 返回 ERA，重填时返回规范化的 TLBRERA 程序计数器。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn pc(&self) -> usize {
        self.era
    }

    /// 设置下一异常返回程序计数器，不改变特权状态。
    #[allow(dead_code)] // 公开执行接口。
    pub fn set_pc(&mut self, pc: usize) {
        self.era = pc;
    }

    /// 返回保存的架构栈指针（r3）。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn sp(&self) -> usize {
        self.gpr[3]
    }

    /// 解码 ESTAT，合成的重填标记优先于 Ecode。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn cause(&self) -> TrapCause {
        TrapCause::from_estat(self.estat)
    }

    /// 返回 ESTAT，重填时位 63 置一；并非未经修改的 CSR 快照。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn raw_cause(&self) -> usize {
        self.estat
    }

    /// 返回 PRMD，或将 PWE 移至 PRMD 位 3 后的规范化 TLBRPRMD。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn raw_status(&self) -> usize {
        self.prmd
    }

    /// 返回 BADV，重填时返回 TLBRBADV。
    #[allow(dead_code)] // 公开上下文查看接口。
    pub fn bad_addr(&self) -> usize {
        self.badv
    }
}

// 上下文偏移用于所有汇编通用寄存器访问；栈帧及 addi.d/ld.d
// 位移必须适配有符号 12 位立即数，并保持 ABI 对齐。
const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, gpr) % 8 == 0);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
    assert!(core::mem::size_of::<TrapContext>() <= 2047);
    assert!(core::mem::offset_of!(TrapContext, badv) <= 2047);
};

global_asm!(
    include_str!("entry.S"),
    include_str!("return.S"),
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

/// 安装对齐的直接异常及重填向量，并保留 SAVE0..SAVE3。
///
/// 在 PLV0、内核栈有效时调用一次，且须早于执行用户代码。
/// 清除 ECFG.VS 及所有中断使能；EUEN 关闭浮点、LSX、LASX。
/// SAVE3 保留 Rust 陷阱处理函数所需的内核 ABI 线程指针。
pub fn init() {
    // SAFETY: 单处理器内核早期初始化独占这些特权 CSR。
    // 汇编向量按 4096 字节对齐，并保持永久映射。
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

/// 进入 PLV3，在任意用户异常发生时返回并保存全部整数状态。
///
/// 即便上下文包含恶意状态，也会将 PRMD 改写为 PPLV=3、PIE=0、PWE=0；
/// 特权返回状态不会泄漏到用户执行环境。
/// 内核栈、线程指针和 ABI 被调用者保存寄存器在切换后保持不变。
///
/// # Safety
/// 在已初始化的启动处理器上以 PLV0 调用，并关闭中断。
/// 整个切换过程中，上下文必须保持由调用者独占，且在 PLV0
/// 可访问。程序计数器与栈必须满足指令和 ABI 对齐；未映射
/// 访问会作为陷阱报告，而不会被自动解决。调用者必须
/// 确保用户执行不能覆盖内核状态（包括此上下文
/// 和暂停执行的内核栈）。此处不创建映射，
/// 也不提供内存隔离。由于 EUEN 为零，浮点和向量指令会刻意
/// 触发陷阱；不保存任何扩展寄存器状态。
#[allow(dead_code)] // 公开执行接口；启动流程仅在内核测试中使用。
pub unsafe fn run_user(context: &mut TrapContext) {
    context.prmd = 3;
    // SAFETY: 调用者保证状态已映射，并独占单处理器执行；
    // 汇编在改变特权级前保存内核 ABI 栈帧。
    unsafe { __loongarch_run_user(context) };
}

#[no_mangle]
extern "C" fn __loongarch_handle_kernel(context: &mut TrapContext) {
    crate::trap::handle_kernel(context);
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
