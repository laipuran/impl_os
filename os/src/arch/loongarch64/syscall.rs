//! LoongArch 64 Linux 系统调用寄存器约定。

use super::trap::{
    cause::{ExceptionCause, TrapCause},
    TrapContext,
};

/// 系统调用号寄存器：`$a7`（`$r11`）。
pub(crate) const NUMBER_REGISTER: usize = 11;
/// 系统调用参数起始寄存器：`$a0..$a5`（`$r4..$r9`），其中 `$a0` 兼作返回值寄存器。
pub(crate) const ARGUMENT_START: usize = 4;

/// 判断当前 Trap 是否为来自用户态（`PPLV == 3`）的 `syscall` 异常。
///
/// LoongArch 的 `ESTAT.Ecode = 0xb`（`SystemCall`）不区分触发时的特权级，因此必须同时检查保存的 `PPLV` 为 PLV3。
pub(crate) fn is_syscall(context: &TrapContext) -> bool {
    context.cause() == TrapCause::Exception(ExceptionCause::SystemCall)
        && context.raw_status() & 3 == 3
}
