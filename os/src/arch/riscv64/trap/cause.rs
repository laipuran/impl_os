#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum TrapCause {
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    #[error("exception: {0}")]
    Exception(ExceptionCause),
}

impl TrapCause {
    /// 先区分中断位，再按各自独立的编号空间解析原因。
    pub(super) fn from_scause(scause: usize) -> Self {
        let interrupt_bit = 1usize << (usize::BITS - 1);
        let code = scause & !interrupt_bit;
        if scause & interrupt_bit != 0 {
            Self::Interrupt(InterruptCause::from(code))
        } else {
            Self::Exception(ExceptionCause::from(code))
        }
    }
}

/// S 态常用中断编号；未识别的编号保留供诊断使用。
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[repr(usize)]
pub(super) enum InterruptCause {
    #[error("supervisor software interrupt")]
    SupervisorSoftware = 1,
    #[error("supervisor timer interrupt")]
    SupervisorTimer = 5,
    #[error("supervisor external interrupt")]
    SupervisorExternal = 9,
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

/// RISC-V scause 寄存器中的同步异常编号。
///
/// 中断使用不同的编号空间，必须在转换前
/// 加以区分。未知异常编号由 Unknown 保留。
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[repr(usize)]
pub(super) enum ExceptionCause {
    #[error("instruction address misaligned")]
    InstructionAddressMisaligned = 0,
    #[error("instruction access fault")]
    InstructionAccessFault = 1,
    #[error("illegal instruction")]
    IllegalInstruction = 2,
    #[error("breakpoint")]
    Breakpoint = 3,
    #[error("load address misaligned")]
    LoadAddressMisaligned = 4,
    #[error("load access fault")]
    LoadAccessFault = 5,
    #[error("store/AMO address misaligned")]
    StoreAmoAddressMisaligned = 6,
    #[error("store/AMO access fault")]
    StoreAmoAccessFault = 7,
    #[error("environment call from U-mode")]
    UserEnvironmentCall = 8,
    #[error("environment call from S-mode")]
    SupervisorEnvironmentCall = 9,
    #[error("instruction page fault")]
    InstructionPageFault = 12,
    #[error("load page fault")]
    LoadPageFault = 13,
    #[error("store/AMO page fault")]
    StoreAmoPageFault = 15,
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

#[cfg(test)]
mod tests {
    use super::{ExceptionCause, InterruptCause, TrapCause};

    const INTERRUPT_BIT: usize = 1usize << (usize::BITS - 1);

    #[test]
    fn separates_interrupt_and_exception_codes() {
        assert_eq!(
            TrapCause::from_scause(5),
            TrapCause::Exception(ExceptionCause::LoadAccessFault)
        );
        assert_eq!(
            TrapCause::from_scause(INTERRUPT_BIT | 5),
            TrapCause::Interrupt(InterruptCause::SupervisorTimer)
        );
    }

    #[test]
    fn decodes_supervisor_interrupts() {
        for (code, cause) in [
            (1, InterruptCause::SupervisorSoftware),
            (5, InterruptCause::SupervisorTimer),
            (9, InterruptCause::SupervisorExternal),
        ] {
            assert_eq!(
                TrapCause::from_scause(INTERRUPT_BIT | code),
                TrapCause::Interrupt(cause)
            );
        }
    }

    #[test]
    fn preserves_unknown_codes() {
        for code in [0, 16, INTERRUPT_BIT - 1] {
            assert_eq!(
                TrapCause::from_scause(INTERRUPT_BIT | code),
                TrapCause::Interrupt(InterruptCause::Unknown(code))
            );
        }
        for code in [10, 16, INTERRUPT_BIT - 1] {
            assert_eq!(
                TrapCause::from_scause(code),
                TrapCause::Exception(ExceptionCause::Unknown(code))
            );
        }
    }

    #[test]
    fn formats_cause_descriptions() {
        assert_eq!(
            TrapCause::from_scause(3).to_string(),
            "exception: breakpoint"
        );
        assert_eq!(
            TrapCause::from_scause(INTERRUPT_BIT | 5).to_string(),
            "interrupt: supervisor timer interrupt"
        );
        assert_eq!(
            TrapCause::from_scause(16).to_string(),
            "exception: unknown exception code 16"
        );
    }
}
