//! 解码 RV64 `scause` 中相互独立的中断码与异常码。

use thiserror::Error;

/// 根据 `scause` 最高位的中断标志对监管态陷阱分类。
///
/// 未知原因码保留中断标志以外的全部位，便于诊断。
/// 此分类不表示陷阱可以恢复，也不决定返回地址的调整策略。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TrapCause {
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    #[error("exception: {0}")]
    Exception(ExceptionCause),
}

impl TrapCause {
    /// 解码原始 `scause`，仅移除架构定义的中断标志位。
    pub fn from_scause(scause: usize) -> Self {
        let interrupt_bit = 1usize << (usize::BITS - 1);
        let code = scause & !interrupt_bit;
        if scause & interrupt_bit != 0 {
            Self::Interrupt(InterruptCause::from(code))
        } else {
            Self::Exception(ExceptionCause::from(code))
        }
    }
}

/// 监管态中断码；无法识别的原因码会完整保留。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum InterruptCause {
    #[error("supervisor software interrupt")]
    SupervisorSoftware,
    #[error("supervisor timer interrupt")]
    SupervisorTimer,
    #[error("supervisor external interrupt")]
    SupervisorExternal,
    #[error("unknown interrupt code {0}")]
    Unknown(usize),
}

impl From<usize> for InterruptCause {
    fn from(code: usize) -> Self {
        match code {
            1 => Self::SupervisorSoftware,
            5 => Self::SupervisorTimer,
            9 => Self::SupervisorExternal,
            _ => Self::Unknown(code),
        }
    }
}

/// 同步异常码，与中断码使用不同的编号空间。
///
/// 未在此列出的保留码和扩展专用码统一保留为 `Unknown`。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ExceptionCause {
    #[error("instruction address misaligned")]
    InstructionAddressMisaligned,
    #[error("instruction access fault")]
    InstructionAccessFault,
    #[error("illegal instruction")]
    IllegalInstruction,
    #[error("breakpoint")]
    Breakpoint,
    #[error("load address misaligned")]
    LoadAddressMisaligned,
    #[error("load access fault")]
    LoadAccessFault,
    #[error("store/AMO address misaligned")]
    StoreAmoAddressMisaligned,
    #[error("store/AMO access fault")]
    StoreAmoAccessFault,
    #[error("environment call from U-mode")]
    UserEnvironmentCall,
    #[error("environment call from S-mode")]
    SupervisorEnvironmentCall,
    #[error("instruction page fault")]
    InstructionPageFault,
    #[error("load page fault")]
    LoadPageFault,
    #[error("store/AMO page fault")]
    StoreAmoPageFault,
    #[error("unknown exception code {0}")]
    Unknown(usize),
}

impl From<usize> for ExceptionCause {
    fn from(code: usize) -> Self {
        match code {
            0 => Self::InstructionAddressMisaligned,
            1 => Self::InstructionAccessFault,
            2 => Self::IllegalInstruction,
            3 => Self::Breakpoint,
            4 => Self::LoadAddressMisaligned,
            5 => Self::LoadAccessFault,
            6 => Self::StoreAmoAddressMisaligned,
            7 => Self::StoreAmoAccessFault,
            8 => Self::UserEnvironmentCall,
            9 => Self::SupervisorEnvironmentCall,
            12 => Self::InstructionPageFault,
            13 => Self::LoadPageFault,
            15 => Self::StoreAmoPageFault,
            _ => Self::Unknown(code),
        }
    }
}
