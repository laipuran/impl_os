//! Decode the independent interrupt and exception code spaces of RV64 `scause`.

use core::fmt;

/// A supervisor trap, classified by the high interrupt bit of `scause`.
///
/// Unknown codes retain every non-interrupt bit for diagnostics. Classification
/// does not imply that the trap is recoverable or choose a return-PC policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapCause {
    Interrupt(InterruptCause),
    Exception(ExceptionCause),
}

impl TrapCause {
    /// Decode raw `scause`, removing only its architectural interrupt bit.
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

impl fmt::Display for TrapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interrupt(cause) => write!(f, "interrupt: {cause}"),
            Self::Exception(cause) => write!(f, "exception: {cause}"),
        }
    }
}

/// Supervisor interrupt codes; unrecognized codes are retained without loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptCause {
    SupervisorSoftware,
    SupervisorTimer,
    SupervisorExternal,
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

impl fmt::Display for InterruptCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SupervisorSoftware => f.write_str("supervisor software interrupt"),
            Self::SupervisorTimer => f.write_str("supervisor timer interrupt"),
            Self::SupervisorExternal => f.write_str("supervisor external interrupt"),
            Self::Unknown(code) => write!(f, "unknown interrupt code {code}"),
        }
    }
}

/// Synchronous exception codes, distinct from the interrupt numbering space.
///
/// Reserved and extension-specific codes not named here remain `Unknown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExceptionCause {
    InstructionAddressMisaligned,
    InstructionAccessFault,
    IllegalInstruction,
    Breakpoint,
    LoadAddressMisaligned,
    LoadAccessFault,
    StoreAmoAddressMisaligned,
    StoreAmoAccessFault,
    UserEnvironmentCall,
    SupervisorEnvironmentCall,
    InstructionPageFault,
    LoadPageFault,
    StoreAmoPageFault,
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

impl fmt::Display for ExceptionCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstructionAddressMisaligned => f.write_str("instruction address misaligned"),
            Self::InstructionAccessFault => f.write_str("instruction access fault"),
            Self::IllegalInstruction => f.write_str("illegal instruction"),
            Self::Breakpoint => f.write_str("breakpoint"),
            Self::LoadAddressMisaligned => f.write_str("load address misaligned"),
            Self::LoadAccessFault => f.write_str("load access fault"),
            Self::StoreAmoAddressMisaligned => f.write_str("store/AMO address misaligned"),
            Self::StoreAmoAccessFault => f.write_str("store/AMO access fault"),
            Self::UserEnvironmentCall => f.write_str("environment call from U-mode"),
            Self::SupervisorEnvironmentCall => f.write_str("environment call from S-mode"),
            Self::InstructionPageFault => f.write_str("instruction page fault"),
            Self::LoadPageFault => f.write_str("load page fault"),
            Self::StoreAmoPageFault => f.write_str("store/AMO page fault"),
            Self::Unknown(code) => write!(f, "unknown exception code {code}"),
        }
    }
}
