//! Linux syscall 的整数寄存器 ABI。
//!
//! [`decode`] 只读取现场；[`complete`] 在调用正常返回时写回结果并跳过
//! syscall 指令。退出、替换程序或重启调用的路径不应调用 `complete`。

use crate::arch::{syscall as syscall_abi, trap::TrapContext};

/// 从用户 syscall 陷阱提取的原始调用号与六个机器字参数。
///
/// 参数保留全部位，不解引用用户指针，也不按具体调用解释有符号数。
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // 用户执行循环接入前，仅由 ktest 使用。
pub struct SyscallRequest {
    /// Linux 调用号；未知调用号同样保留，交给分发层处理。
    pub number: usize,
    /// 按 Linux ABI 顺序排列的六个原始参数。
    pub args: [usize; 6],
}

/// 解码用户 syscall；内核调用、中断和其他异常返回 `None`，不修改现场。
#[allow(dead_code)] // 用户执行循环接入前，仅由 ktest 使用。
pub fn decode(context: &TrapContext) -> Option<SyscallRequest> {
    if !syscall_abi::is_syscall(context) {
        return None;
    }
    let registers = context.registers();
    Some(SyscallRequest {
        number: registers[syscall_abi::NUMBER_REGISTER],
        args: core::array::from_fn(|index| registers[syscall_abi::ARGUMENT_START + index]),
    })
}

/// 完成一次正常返回的用户 syscall，写回原始结果并推进 PC。
///
/// `result` 使用 Linux 原始返回约定：成功值或负 errno，不进行 libc 的
/// `-1`/`errno` 转换。只修改 `a0` 和 PC，保留其他整数寄存器与陷阱元数据。
/// 两架构的 syscall 指令均长四字节，包括启用压缩指令的 RISC-V。
/// 下次取指的有效性仍由用户映射和 trap 策略决定。
///
/// 非用户 syscall 或 PC 加四溢出时返回 `false`，且不修改现场；调用者应将
/// 完成失败交给任务异常处理策略，不能继续恢复该现场。
/// 调用者须保证每次陷阱只完成一次；保存的异常原因不会因写回而清除。
/// 退出、exec 和重启调用应采用各自的现场处理。
#[must_use]
#[allow(dead_code)] // 用户执行循环接入前，仅由 ktest 使用。
pub fn complete(context: &mut TrapContext, result: isize) -> bool {
    if !syscall_abi::is_syscall(context) {
        return false;
    }
    let Some(next_pc) = context.pc().checked_add(4) else {
        return false;
    };
    context.registers_mut()[syscall_abi::ARGUMENT_START] = result as usize;
    context.set_pc(next_pc);
    true
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
