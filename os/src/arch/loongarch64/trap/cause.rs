#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum TrapCause {
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    #[error("exception: {0}")]
    Exception(ExceptionCause),
}

impl TrapCause {
    /// bit63 是入口为 TLB refill 添加的标记；此时 ESTAT.Ecode 已过期。
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

/// ESTAT.IS 是待处理中断位图，允许多个位同时置位，不能直接当成编号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum InterruptCause {
    #[error("software interrupt {0}")]
    Software(usize),
    #[error("hardware interrupt {0}")]
    Hardware(usize),
    #[error("performance counter interrupt")]
    PerformanceCounter,
    #[error("timer interrupt")]
    Timer,
    #[error("interprocessor interrupt")]
    Interprocessor,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum ExceptionCause {
    #[error("page invalid on load")]
    PageInvalidLoad,
    #[error("page invalid on store")]
    PageInvalidStore,
    #[error("page invalid on instruction fetch")]
    PageInvalidFetch,
    #[error("page modification fault")]
    PageModification,
    #[error("page not readable")]
    PageNotReadable,
    #[error("page not executable")]
    PageNotExecutable,
    #[error("page privilege fault")]
    PagePrivilege,
    #[error("instruction address error")]
    InstructionAddressError,
    #[error("data address error")]
    DataAddressError,
    #[error("address alignment error")]
    AddressMisaligned,
    #[error("bound check error")]
    BoundCheck,
    #[error("system call")]
    SystemCall,
    #[error("breakpoint")]
    Breakpoint,
    #[error("illegal instruction")]
    IllegalInstruction,
    #[error("instruction privilege error")]
    InstructionPrivilege,
    #[error("floating point disabled")]
    FloatingPointDisabled,
    #[error("LSX disabled")]
    LsxDisabled,
    #[error("LASX disabled")]
    LasxDisabled,
    #[error("floating point exception")]
    FloatingPoint,
    #[error("TLB refill")]
    TlbRefill,
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

#[cfg(test)]
mod tests {
    use super::{ExceptionCause, InterruptCause, TrapCause};

    #[test]
    fn separates_interrupts_and_exceptions() {
        assert_eq!(
            TrapCause::from_estat(1 << 11),
            TrapCause::Interrupt(InterruptCause::Timer)
        );
        assert_eq!(
            TrapCause::from_estat((12 << 16) | (1 << 11)),
            TrapCause::Exception(ExceptionCause::Breakpoint)
        );
    }

    #[test]
    fn preserves_simultaneous_interrupts() {
        assert_eq!(
            TrapCause::from_estat((1 << 11) | 1),
            TrapCause::Interrupt(InterruptCause::Pending((1 << 11) | 1))
        );
    }

    #[test]
    fn preserves_unknown_exception_subcodes() {
        assert_eq!(
            TrapCause::from_estat((63 << 16) | (511 << 22)),
            TrapCause::Exception(ExceptionCause::Unknown {
                code: 63,
                subcode: 511
            })
        );
        assert_eq!(
            TrapCause::from_estat((8 << 16) | (1 << 22)),
            TrapCause::Exception(ExceptionCause::DataAddressError)
        );
    }

    #[test]
    fn refill_overrides_stale_estat() {
        assert_eq!(
            TrapCause::from_estat((1usize << 63) | (12 << 16)),
            TrapCause::Exception(ExceptionCause::TlbRefill)
        );
    }
}
