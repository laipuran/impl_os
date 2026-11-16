//! LoongArch 64 `ESTAT` 寄存器与软件 TLB 重填标记解码。

use thiserror::Error;

/// 从保存的 `estat` 解码出的中断或同步异常原因。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TrapCause {
    /// `Ecode == 0`：异步中断（由 `ESTAT.IS` 位图标识挂起的中断源）。
    #[error("interrupt: {0}")]
    Interrupt(InterruptCause),
    /// 同步异常（含带软件标记的 TLB 重填异常）。
    #[error("exception: {0}")]
    Exception(ExceptionCause),
}

impl TrapCause {
    /// 解码保存的 `estat` 值；第 63 位的软件 TLB 重填标记优先于残留的 `Ecode`。
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

/// `ESTAT.IS`（位 `0..=12`）中断状态解码。
///
/// 由于 `ESTAT.IS` 是位图而非单一编号，若同时有多个中断置位（或全为 0），则原样保留在 [`Self::Pending`] 中。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum InterruptCause {
    /// 软件中断 `SWI0` / `SWI1`（`IS[0..=1]`）。
    #[error("software interrupt {0}")]
    Software(usize),
    /// 硬件外部中断 `HWI0..HWI7`（`IS[2..=9]`）。
    #[error("hardware interrupt {0}")]
    Hardware(usize),
    /// 性能监测计数溢出中断 `PMI`（`IS[10]`）。
    #[error("performance counter interrupt")]
    PerformanceCounter,
    /// 定时器中断 `TI`（`IS[11]`）。
    #[error("timer interrupt")]
    Timer,
    /// 核间中断 `IPI`（`IS[12]`）。
    #[error("interprocessor interrupt")]
    Interprocessor,
    /// 没有置位或有多个中断位同时置位时，保留完整的 `IS` 掩码。
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

/// `ESTAT.Ecode` 与 `EsubCode` 同步异常原因码。
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ExceptionCause {
    /// `PIL`（`Ecode = 0x1`）：load 操作页无效异常。
    #[error("page invalid on load")]
    PageInvalidLoad,
    /// `PIS`（`Ecode = 0x2`）：store 操作页无效异常。
    #[error("page invalid on store")]
    PageInvalidStore,
    /// `PIF`（`Ecode = 0x3`）：取指操作页无效异常。
    #[error("page invalid on instruction fetch")]
    PageInvalidFetch,
    /// `PME`（`Ecode = 0x4`）：页修改异常。
    #[error("page modification fault")]
    PageModification,
    /// `PNR`（`Ecode = 0x5`）：页不可读异常。
    #[error("page not readable")]
    PageNotReadable,
    /// `PNX`（`Ecode = 0x6`）：页不可执行异常。
    #[error("page not executable")]
    PageNotExecutable,
    /// `PPI`（`Ecode = 0x7`）：页特权等级不合规异常。
    #[error("page privilege fault")]
    PagePrivilege,
    /// `ADEF`（`Ecode = 0x8, EsubCode = 0`）：取指地址错异常。
    #[error("instruction address error")]
    InstructionAddressError,
    /// `ADEM`（`Ecode = 0x8, EsubCode = 1`）：访存指令地址错异常。
    #[error("data address error")]
    DataAddressError,
    /// `ALE`（`Ecode = 0x9`）：地址非对齐异常。
    #[error("address alignment error")]
    AddressMisaligned,
    /// `BCE`（`Ecode = 0xa`）：边界约束检查错异常。
    #[error("bound check error")]
    BoundCheck,
    /// `SYS`（`Ecode = 0xb`）：系统调用异常。
    #[error("system call")]
    SystemCall,
    /// `BRK`（`Ecode = 0xc`）：断点异常。
    #[error("breakpoint")]
    Breakpoint,
    /// `INE`（`Ecode = 0xd`）：指令不存在（非法指令）异常。
    #[error("illegal instruction")]
    IllegalInstruction,
    /// `IPE`（`Ecode = 0xe`）：指令特权等级错异常。
    #[error("instruction privilege error")]
    InstructionPrivilege,
    /// `FPD`（`Ecode = 0xf`）：浮点指令未使能异常。
    #[error("floating point disabled")]
    FloatingPointDisabled,
    /// `SXD`（`Ecode = 0x10`）：128 位向量（LSX）指令未使能异常。
    #[error("LSX disabled")]
    LsxDisabled,
    /// `ASXD`（`Ecode = 0x11`）：256 位向量（LASX）指令未使能异常。
    #[error("LASX disabled")]
    LasxDisabled,
    /// `FPE`（`Ecode = 0x12, EsubCode = 0`）：基础浮点运算异常。
    #[error("floating point exception")]
    FloatingPoint,
    /// 由 `TLBRENTRY` 入口捕获并打上软件最高位标记的 TLB 重填异常。
    #[error("TLB refill")]
    TlbRefill,
    /// 未识别的 `Ecode` / `EsubCode` 组合。
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
