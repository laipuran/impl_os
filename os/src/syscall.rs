//! Linux 系统调用 ABI 解码、分发与返回值写回。
//!
//! 系统调用处理分为三步：
//! 1. [`decode`] 从用户 [`TrapContext`] 中提取调用号与 6 个原始参数；
//! 2. [`dispatch`] 执行系统调用逻辑，决定是正常返回还是终止当前任务；
//! 3. 若正常返回，由 [`complete`] 将返回值写入 `a0` 并将异常 PC 推进 4 字节。

use crate::arch::{syscall as syscall_abi, trap::TrapContext};

/// 从用户态系统调用现场提取的调用号与 6 个原始参数。
///
/// 参数保留完整的机器字位宽，本层不将其解释为有符号数，也不直接解引用用户指针。
#[derive(Debug, PartialEq, Eq)]
pub struct SyscallRequest {
    /// Linux 系统调用号。
    pub number: usize,
    /// 按 Linux ABI 寄存器顺序排列的 6 个原始参数。
    pub args: [usize; 6],
}

/// 从用户态 Trap 现场解码系统调用请求。
///
/// 若当前 Trap 不是来自用户态的系统调用（例如断点异常、中断或内核态调用），
/// 则返回 `None` 且不修改 `context`。
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

/// 完成一次正常返回的用户系统调用：将返回值写入 `a0` 并令异常 PC 前移 4 字节。
///
/// `result` 遵循 Linux 内核返回约定：非负数表示成功返回值，负数表示 `-errno`
///（不做用户态 libc 的 `-1` / `errno` 拆分）。仅更新返回值寄存器和 PC，其余通用寄存器
/// 与异常 CSR 保持不变。RISC-V 的 `ecall` 与 LoongArch 的 `syscall 0` 指令长度均为 4 字节。
///
/// 若 `context` 不是用户态系统调用现场，或 `pc + 4` 发生无符号整数溢出，则返回 `false`
/// 且不修改 `context`。调用方此时应按任务异常终止处理，不得原样恢复执行。
/// 注意：每次系统调用 Trap 只能调用一次 `complete`；`exit` 等不返回的调用不应调用本函数。
#[must_use]
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

/// 系统调用分发后交由任务执行循环处理的动作。
pub enum Outcome {
    /// 将返回值写回用户态寄存器并继续执行下一条指令。
    Return(isize),
    /// 终止当前任务并记录退出码（不写回返回值，也不推进 PC）。
    Exit(u8),
}

/// 根据 Linux 系统调用号分发请求。
///
/// 目前仅实现 `exit`（93）和 `exit_group`（94），截取首个参数的低 8 位作为退出码；
/// 其余调用号统一返回 `-ENOSYS`（-38）。
pub fn dispatch(call: SyscallRequest) -> Outcome {
    match call.number {
        93 | 94 => Outcome::Exit(call.args[0] as u8),
        _ => Outcome::Return(-38),
    }
}

#[cfg(feature = "ktest")]
#[path = "ktest/syscall/mod.rs"]
pub(crate) mod tests;
