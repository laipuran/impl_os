//! RV64 Linux syscall 寄存器约定；ecall 不带立即数操作数。

use super::trap::{
    cause::{ExceptionCause, TrapCause},
    TrapContext,
};

/// a7（x17）保存调用号。
pub(crate) const NUMBER_REGISTER: usize = 17;
/// a0..a5（x10..x15）保存六个参数；a0 同时保存返回值。
pub(crate) const ARGUMENT_START: usize = 10;

/// scause 区分用户 ecall 和监管态 ecall，不能将后者作为用户调用处理。
pub(crate) fn is_syscall(context: &TrapContext) -> bool {
    context.cause() == TrapCause::Exception(ExceptionCause::UserEnvironmentCall)
}
