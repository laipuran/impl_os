//! 解码 ESTAT，包括重填入口使用的软件标记。

use thiserror::Error;

/// 陷阱入口保存的中断位图或同步异常。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TrapCause {
    /// Ecode 为零；ESTAT.IS 表示当前待处理的中断源。
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    /// 同步异常，包括带软件标记的 TLB 重填。
    #[error("exception: {0}")]
    Exception(ExceptionCause),
}

impl TrapCause {
    /// 解码保存的 ESTAT，第 63 位的重填标记优先于残留的 Ecode。
    pub(super) fn from_estat(estat: usize) -> Self {
        if estat & (1usize << 63) != 0 {
            return Self::Exception(ExceptionCause::TlbRefill);
        }
        let code = (estat >> 16) & 0x3f;
        let subcode = (estat >> 22) & 0x1ff;
        if code == 0 {
            Self::Interrupt(InterruptCause::from(estat & 0x1fff))
        } else {
            Self::Exception(ExceptionCause::from_code(code, subcode))
        }
    }
}

/// ESTAT.IS 是位图而非中断编号；同时置位的所有中断都会保留。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum InterruptCause {
    /// 软件中断 0 或 1。
    #[error("software interrupt {0}")]
    Software(usize),
    /// 硬件中断 0 至 7，对应 IS 的第 2 至 9 位。
    #[error("hardware interrupt {0}")]
    Hardware(usize),
    /// 性能监控计数器中断，对应 IS 的第 10 位。
    #[error("performance counter interrupt")]
    PerformanceCounter,
    /// 定时器中断，对应 IS 的第 11 位。
    #[error("timer interrupt")]
    Timer,
    /// 处理器间中断，对应 IS 的第 12 位。
    #[error("interprocessor interrupt")]
    Interprocessor,
    /// 待处理位为零、有多个置位或无法识别时，完整保留且不设优先级。
    #[error("pending interrupt mask {0:#05x}")]
    Pending(usize),
}

impl From<usize> for InterruptCause {
    fn from(pending: usize) -> Self {
        if pending.count_ones() != 1 {
            return Self::Pending(pending);
        }
        match pending.trailing_zeros() {
            index @ 0..=1 => Self::Software(index as usize),
            index @ 2..=9 => Self::Hardware(index as usize - 2),
            10 => Self::PerformanceCounter,
            11 => Self::Timer,
            12 => Self::Interprocessor,
            _ => Self::Pending(pending),
        }
    }
}

/// ESTAT.Ecode/EsubCode；保留未知组合，不推测其含义。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ExceptionCause {
    /// 读取时页无效。
    #[error("page invalid on load")]
    PageInvalidLoad,
    /// 写入时页无效。
    #[error("page invalid on store")]
    PageInvalidStore,
    /// 取指时页无效。
    #[error("page invalid on instruction fetch")]
    PageInvalidFetch,
    /// 页修改异常。
    #[error("page modification fault")]
    PageModification,
    /// 页不可读。
    #[error("page not readable")]
    PageNotReadable,
    /// 页不可执行。
    #[error("page not executable")]
    PageNotExecutable,
    /// 页特权等级异常。
    #[error("page privilege fault")]
    PagePrivilege,
    /// 指令地址错误，Ecode 为 8，EsubCode 为 0。
    #[error("instruction address error")]
    InstructionAddressError,
    /// 数据地址错误，Ecode 为 8，EsubCode 为 1。
    #[error("data address error")]
    DataAddressError,
    /// 地址对齐错误。
    #[error("address alignment error")]
    AddressMisaligned,
    /// 边界检查错误。
    #[error("bound check error")]
    BoundCheck,
    /// 显式系统调用。
    #[error("system call")]
    SystemCall,
    /// 显式断点。
    #[error("breakpoint")]
    Breakpoint,
    /// 非法指令。
    #[error("illegal instruction")]
    IllegalInstruction,
    /// 指令特权等级错误。
    #[error("instruction privilege error")]
    InstructionPrivilege,
    /// 浮点执行未启用。
    #[error("floating point disabled")]
    FloatingPointDisabled,
    /// LSX 执行未启用。
    #[error("LSX disabled")]
    LsxDisabled,
    /// LASX 执行未启用。
    #[error("LASX disabled")]
    LasxDisabled,
    /// 浮点异常。
    #[error("floating point exception")]
    FloatingPoint,
    /// 专用重填入口；此时 ESTAT.Ecode 是残留值，不应解码。
    #[error("TLB refill")]
    TlbRefill,
    /// 无法识别的 Ecode/EsubCode 组合，两个字段均完整保留。
    #[error("unknown exception code {code}, subcode {subcode}")]
    Unknown { code: usize, subcode: usize },
}

impl ExceptionCause {
    fn from_code(code: usize, subcode: usize) -> Self {
        match (code, subcode) {
            (1, 0) => Self::PageInvalidLoad,
            (2, 0) => Self::PageInvalidStore,
            (3, 0) => Self::PageInvalidFetch,
            (4, 0) => Self::PageModification,
            (5, 0) => Self::PageNotReadable,
            (6, 0) => Self::PageNotExecutable,
            (7, 0) => Self::PagePrivilege,
            (8, 0) => Self::InstructionAddressError,
            (8, 1) => Self::DataAddressError,
            (9, 0) => Self::AddressMisaligned,
            (10, 0) => Self::BoundCheck,
            (11, 0) => Self::SystemCall,
            (12, 0) => Self::Breakpoint,
            (13, 0) => Self::IllegalInstruction,
            (14, 0) => Self::InstructionPrivilege,
            (15, 0) => Self::FloatingPointDisabled,
            (16, 0) => Self::LsxDisabled,
            (17, 0) => Self::LasxDisabled,
            (18, 0) => Self::FloatingPoint,
            _ => Self::Unknown { code, subcode },
        }
    }
}
