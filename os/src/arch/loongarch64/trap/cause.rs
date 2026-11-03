//! ESTAT decoding, including the software tag used by the refill entry.

use core::fmt;

/// An interrupt bitmap or synchronous exception captured by the trap entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapCause {
    /// Ecode is zero; ESTAT.IS describes the pending interrupt sources.
    Interrupt(InterruptCause),
    /// A synchronous exception, including a tagged TLB refill.
    Exception(ExceptionCause),
}

impl TrapCause {
    /// Decode saved ESTAT, giving the bit-63 refill tag priority over stale Ecode.
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

impl fmt::Display for TrapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interrupt(cause) => write!(f, "interrupt: {cause}"),
            Self::Exception(cause) => write!(f, "exception: {cause}"),
        }
    }
}

/// ESTAT.IS is a bitmap, not an interrupt number; simultaneous bits are retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptCause {
    /// Software interrupt 0 or 1.
    Software(usize),
    /// Hardware interrupt 0 through 7 (IS bits 2 through 9).
    Hardware(usize),
    /// Performance-monitor counter interrupt (IS bit 10).
    PerformanceCounter,
    /// Timer interrupt (IS bit 11).
    Timer,
    /// Interprocessor interrupt (IS bit 12).
    Interprocessor,
    /// Zero, multiple, or unrecognized pending bits, preserved without priority.
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

impl fmt::Display for InterruptCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Software(index) => write!(f, "software interrupt {index}"),
            Self::Hardware(index) => write!(f, "hardware interrupt {index}"),
            Self::PerformanceCounter => f.write_str("performance counter interrupt"),
            Self::Timer => f.write_str("timer interrupt"),
            Self::Interprocessor => f.write_str("interprocessor interrupt"),
            Self::Pending(mask) => write!(f, "pending interrupt mask {mask:#05x}"),
        }
    }
}

/// ESTAT.Ecode/EsubCode, preserving unknown combinations rather than guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExceptionCause {
    /// Invalid page on a load.
    PageInvalidLoad,
    /// Invalid page on a store.
    PageInvalidStore,
    /// Invalid page on instruction fetch.
    PageInvalidFetch,
    /// Page modification fault.
    PageModification,
    /// Page is not readable.
    PageNotReadable,
    /// Page is not executable.
    PageNotExecutable,
    /// Page privilege fault.
    PagePrivilege,
    /// Instruction address error (Ecode 8, EsubCode 0).
    InstructionAddressError,
    /// Data address error (Ecode 8, EsubCode 1).
    DataAddressError,
    /// Address alignment error.
    AddressMisaligned,
    /// Bound-check error.
    BoundCheck,
    /// Explicit system call.
    SystemCall,
    /// Explicit breakpoint.
    Breakpoint,
    /// Illegal instruction.
    IllegalInstruction,
    /// Instruction privilege error.
    InstructionPrivilege,
    /// Floating-point execution is disabled.
    FloatingPointDisabled,
    /// LSX execution is disabled.
    LsxDisabled,
    /// LASX execution is disabled.
    LasxDisabled,
    /// Floating-point exception.
    FloatingPoint,
    /// Dedicated refill entry; ESTAT.Ecode is stale and must not be decoded.
    TlbRefill,
    /// Unrecognized Ecode/EsubCode pair, with both fields retained.
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

impl fmt::Display for ExceptionCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            Self::PageInvalidLoad => "page invalid on load",
            Self::PageInvalidStore => "page invalid on store",
            Self::PageInvalidFetch => "page invalid on instruction fetch",
            Self::PageModification => "page modification fault",
            Self::PageNotReadable => "page not readable",
            Self::PageNotExecutable => "page not executable",
            Self::PagePrivilege => "page privilege fault",
            Self::InstructionAddressError => "instruction address error",
            Self::DataAddressError => "data address error",
            Self::AddressMisaligned => "address alignment error",
            Self::BoundCheck => "bound check error",
            Self::SystemCall => "system call",
            Self::Breakpoint => "breakpoint",
            Self::IllegalInstruction => "illegal instruction",
            Self::InstructionPrivilege => "instruction privilege error",
            Self::FloatingPointDisabled => "floating point disabled",
            Self::LsxDisabled => "LSX disabled",
            Self::LasxDisabled => "LASX disabled",
            Self::FloatingPoint => "floating point exception",
            Self::TlbRefill => "TLB refill",
            Self::Unknown { code, subcode } => {
                return write!(f, "unknown exception code {code}, subcode {subcode}");
            }
        };
        f.write_str(description)
    }
}
