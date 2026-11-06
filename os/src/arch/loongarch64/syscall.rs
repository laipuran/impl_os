//! LoongArch64 Linux syscall 寄存器约定；syscall 的立即数不承载调用号。

use super::trap::{
    cause::{ExceptionCause, TrapCause},
    TrapContext,
};

/// a7（r11）保存调用号。
pub(crate) const NUMBER_REGISTER: usize = 11;
/// a0..a5（r4..r9）保存六个参数；a0 同时保存返回值。
pub(crate) const ARGUMENT_START: usize = 4;

/// SystemCall 不编码来源特权级，必须同时检查保存的 PPLV 为 PLV3。
pub(crate) fn is_syscall(context: &TrapContext) -> bool {
    context.cause() == TrapCause::Exception(ExceptionCause::SystemCall)
        && context.raw_status() & 3 == 3
}
