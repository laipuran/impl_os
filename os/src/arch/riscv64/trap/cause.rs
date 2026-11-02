/// Synchronous exception codes from the RISC-V scause register.
///
/// Interrupts use a different code space and must be distinguished before
/// conversion. Unknown exception codes remain available in the raw scause.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub(super) enum Exception {
    InstructionAddressMisaligned = 0,
    InstructionAccessFault = 1,
    IllegalInstruction = 2,
    Breakpoint = 3,
    LoadAddressMisaligned = 4,
    LoadAccessFault = 5,
    StoreAmoAddressMisaligned = 6,
    StoreAmoAccessFault = 7,
    UserEnvironmentCall = 8,
    SupervisorEnvironmentCall = 9,
    InstructionPageFault = 12,
    LoadPageFault = 13,
    StoreAmoPageFault = 15,
}

impl TryFrom<usize> for Exception {
    type Error = usize;

    fn try_from(code: usize) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::InstructionAddressMisaligned),
            1 => Ok(Self::InstructionAccessFault),
            2 => Ok(Self::IllegalInstruction),
            3 => Ok(Self::Breakpoint),
            4 => Ok(Self::LoadAddressMisaligned),
            5 => Ok(Self::LoadAccessFault),
            6 => Ok(Self::StoreAmoAddressMisaligned),
            7 => Ok(Self::StoreAmoAccessFault),
            8 => Ok(Self::UserEnvironmentCall),
            9 => Ok(Self::SupervisorEnvironmentCall),
            12 => Ok(Self::InstructionPageFault),
            13 => Ok(Self::LoadPageFault),
            15 => Ok(Self::StoreAmoPageFault),
            _ => Err(code),
        }
    }
}

impl Exception {
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::InstructionAddressMisaligned => "instruction address misaligned",
            Self::InstructionAccessFault => "instruction access fault",
            Self::IllegalInstruction => "illegal instruction",
            Self::Breakpoint => "breakpoint",
            Self::LoadAddressMisaligned => "load address misaligned",
            Self::LoadAccessFault => "load access fault",
            Self::StoreAmoAddressMisaligned => "store/AMO address misaligned",
            Self::StoreAmoAccessFault => "store/AMO access fault",
            Self::UserEnvironmentCall => "environment call from U-mode",
            Self::SupervisorEnvironmentCall => "environment call from S-mode",
            Self::InstructionPageFault => "instruction page fault",
            Self::LoadPageFault => "load page fault",
            Self::StoreAmoPageFault => "store/AMO page fault",
        }
    }
}
