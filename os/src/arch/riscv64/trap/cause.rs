//! RISC-V 64 `scause` 寄存器解码：区分异步中断与同步异常。

use thiserror::Error;

/// 根据 `scause` 最高位（Interrupt bit）分类的 S-mode Trap 原因。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TrapCause {
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    #[error("exception: {0}")]
    Exception(ExceptionCause),
}

impl TrapCause {
    /// 解码原始 `scause` 值，根据最高位拆分为中断或同步异常。
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

/// S-mode 中断原因码（未识别的编码保留在 `Unknown` 中）。
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

/// S-mode 同步异常原因码（未识别的编码保留在 `Unknown` 中）。
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
