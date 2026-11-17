//! RISC-V 64 `scause` 寄存器解码：区分异步中断与同步异常。

use thiserror::Error;

/// 根据 `scause` 最高位（Interrupt bit）分类的 S-mode Trap 原因。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TrapCause {
    /// `scause` 最高位为 `1`：异步中断。
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    /// `scause` 最高位为 `0`：同步异常。
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

/// S-mode 异步中断原因码（与 [`ExceptionCause`] 共享低位编码空间，由 `scause` 最高位区分）。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum InterruptCause {
    /// `code = 1`：S-mode 软件中断（SSI）。
    #[error("supervisor software interrupt")]
    SupervisorSoftware,
    /// `code = 5`：S-mode 时钟中断（STI）。
    #[error("supervisor timer interrupt")]
    SupervisorTimer,
    /// `code = 9`：S-mode 外部中断（SEI）。
    #[error("supervisor external interrupt")]
    SupervisorExternal,
    /// 未识别的中断编码（保存已清除 `scause` 最高位中断标志后的原始低位编号）。
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

/// S-mode 同步异常原因码（`scause` 最高位为 `0` 时的低位异常编号）。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ExceptionCause {
    /// `code = 0`：取指地址未对齐。
    #[error("instruction address misaligned")]
    InstructionAddressMisaligned,
    /// `code = 1`：取指访问故障（如物理内存或 PMP 权限拒绝）。
    #[error("instruction access fault")]
    InstructionAccessFault,
    /// `code = 2`：非法指令（含越权访问 CSR 或在 `FS`/`VS` 为 `Off` 时执行浮点/向量指令）。
    #[error("illegal instruction")]
    IllegalInstruction,
    /// `code = 3`：断点异常（`ebreak`）。
    #[error("breakpoint")]
    Breakpoint,
    /// `code = 4`：load 地址未对齐。
    #[error("load address misaligned")]
    LoadAddressMisaligned,
    /// `code = 5`：load 访问故障。
    #[error("load access fault")]
    LoadAccessFault,
    /// `code = 6`：store/AMO 地址未对齐。
    #[error("store/AMO address misaligned")]
    StoreAmoAddressMisaligned,
    /// `code = 7`：store/AMO 访问故障。
    #[error("store/AMO access fault")]
    StoreAmoAccessFault,
    /// `code = 8`：来自 U-mode 的环境调用（`ecall` 系统调用）。
    #[error("environment call from U-mode")]
    UserEnvironmentCall,
    /// `code = 9`：来自 S-mode 的环境调用。
    #[error("environment call from S-mode")]
    SupervisorEnvironmentCall,
    /// `code = 12`：取指缺页异常。
    #[error("instruction page fault")]
    InstructionPageFault,
    /// `code = 13`：load 缺页异常。
    #[error("load page fault")]
    LoadPageFault,
    /// `code = 15`：store/AMO 缺页异常。
    #[error("store/AMO page fault")]
    StoreAmoPageFault,
    /// 未识别的同步异常编码（保存原始 `scause` 编号）。
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
