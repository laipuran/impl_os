//! RISC-V 64 Linux 系统调用寄存器约定。

use super::trap::{
    cause::{ExceptionCause, TrapCause},
    TrapContext,
};

/// 系统调用号寄存器：`a7`（`x17`）。
pub(crate) const NUMBER_REGISTER: usize = 17;
/// 系统调用参数起始寄存器：`a0..a5`（`x10..x15`），其中 `a0` 兼作返回值寄存器。
pub(crate) const ARGUMENT_START: usize = 10;

/// 判断当前 Trap 是否为用户态发起的 `ecall`（排除 S-mode `ecall` 及其他异常）。
pub(crate) fn is_syscall(context: &TrapContext) -> bool {
    context.cause() == TrapCause::Exception(ExceptionCause::UserEnvironmentCall)
}
