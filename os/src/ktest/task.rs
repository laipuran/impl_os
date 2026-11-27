//! 最小用户任务（[`Task`]）生命周期与系统调用分发自检。

use core::arch::global_asm;

use super::{ExitStatus, Task, TaskState};
use crate::arch::trap::TrapContext;

#[cfg(target_arch = "riscv64")]
global_asm!(include_str!("task/riscv64.S"));
#[cfg(target_arch = "loongarch64")]
global_asm!(include_str!("task/loongarch64.S"));

#[cfg(target_arch = "riscv64")]
const ARGUMENT_REGISTER: usize = 10;
#[cfg(target_arch = "riscv64")]
const NUMBER_REGISTER: usize = 17;
#[cfg(target_arch = "loongarch64")]
const ARGUMENT_REGISTER: usize = 4;
#[cfg(target_arch = "loongarch64")]
const NUMBER_REGISTER: usize = 11;

extern "C" {
    fn task_test_entry();
    fn task_test_exit();
    fn task_test_group_entry();
    fn task_test_exit_group();
    fn task_test_fault();
}

/// 返回任务测试探针的内核地址（由调用方通过 `UserEnv::address` 转换为用户态入口地址）。
pub fn entry() -> usize {
    task_test_entry as *const () as usize
}

fn snapshot(context: &TrapContext) -> ([usize; 32], usize, usize, usize, usize) {
    (
        *context.registers(),
        context.pc(),
        context.raw_status(),
        context.raw_cause(),
        context.bad_addr(),
    )
}

/// 验证已结束的任务再次调用 [`Task::run`] 会直接返回原结果且不会修改已保存的现场。
///
/// # Safety
///
/// 调用方须保证在测试期间持续满足 [`Task::run`] 的环境与映射前提。
unsafe fn assert_finished(task: &mut Task, status: ExitStatus) {
    assert_eq!(task.state(), TaskState::Finished(status));
    let saved = snapshot(task.context());
    // SAFETY: 测试探针、用户映射与测试栈均保持有效。
    assert_eq!(unsafe { task.run() }, status);
    assert_eq!(task.state(), TaskState::Finished(status));
    assert_eq!(snapshot(task.context()), saved);
}

/// 运行用户态任务探针，验证未知系统调用返回 `-ENOSYS`、`exit`/`exit_group` 正常退出以及异常故障终止。
///
/// # Safety
///
/// - 调用方须满足 [`Task::run`] 的安全前提；
/// - `user_entry` 必须是 [`entry`] 在当前 `UserEnv` 映射下的可执行地址，`stack` 为 16 字节对齐的独占用户栈顶。
pub unsafe fn kernel_test(user_entry: usize, stack: usize) {
    let user_address =
        |label: unsafe extern "C" fn()| user_entry + (label as *const () as usize - entry());
    let mut task = Task::new(TrapContext::user(user_entry, stack));
    assert_eq!(task.state(), TaskState::Ready);
    // SAFETY: 探针先发起两次未知系统调用（核对返回值 `-ENOSYS`），随后调用 `exit` 退出，不修改内核内存。
    assert_eq!(unsafe { task.run() }, ExitStatus::Exited(0x5a));
    assert_eq!(task.context().pc(), user_address(task_test_exit));
    assert_eq!(task.context().registers()[ARGUMENT_REGISTER], 0x1_2345_675a);
    assert_eq!(task.context().registers()[NUMBER_REGISTER], 93);
    // SAFETY: 同一受控测试环境仍然有效。
    unsafe { assert_finished(&mut task, ExitStatus::Exited(0x5a)) };

    let fault_pc = user_address(task_test_fault);
    let mut context = TrapContext::user(fault_pc, stack);
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        *value = 0x700 + register;
    }
    #[cfg(target_arch = "riscv64")]
    let stack_register = 2;
    #[cfg(target_arch = "loongarch64")]
    let stack_register = 3;
    context.registers_mut()[stack_register] = stack;
    // 故意在调用号寄存器中填入 `exit`（93），验证断点异常不会被误判为系统调用。
    context.registers_mut()[NUMBER_REGISTER] = 93;
    let registers = *context.registers();
    let mut fault = Task::new(context);
    // SAFETY: `task_test_fault` 仅包含一条断点指令，触发 Trap 后立即返回内核。
    assert_eq!(unsafe { fault.run() }, ExitStatus::Faulted);
    assert_eq!(fault.context().pc(), fault_pc);
    assert_eq!(fault.context().registers(), &registers);
    assert!(fault.context().is_breakpoint());
    #[cfg(target_arch = "riscv64")]
    {
        assert_eq!(fault.context().raw_cause(), 3);
        assert_eq!(fault.context().raw_status() & ((1 << 8) | (1 << 1)), 0);
        assert!(fault.context().bad_addr() == 0 || fault.context().bad_addr() == fault_pc);
    }
    #[cfg(target_arch = "loongarch64")]
    {
        assert_eq!(fault.context().raw_cause() & !0x1fff, 12 << 16);
        assert_eq!(fault.context().raw_status() & 0xf, 3);
    }
    // SAFETY: 同一受控测试环境仍然有效。
    unsafe { assert_finished(&mut fault, ExitStatus::Faulted) };
    let saved_fault = snapshot(fault.context());

    // 在前一个任务因故障终止后新建第二个正常任务，确认内核可继续运行后续任务并正确处理 `exit_group`。
    let mut group = Task::new(TrapContext::user(
        user_address(task_test_group_entry),
        stack,
    ));
    assert_eq!(group.state(), TaskState::Ready);
    // SAFETY: 独立探针仅设置参数并调用 `exit_group`（94），前序任务已结束，不会并发占用测试栈。
    assert_eq!(unsafe { group.run() }, ExitStatus::Exited(0xfe));
    assert_eq!(group.context().pc(), user_address(task_test_exit_group));
    assert_eq!(
        group.context().registers()[ARGUMENT_REGISTER],
        usize::MAX - 1
    );
    assert_eq!(group.context().registers()[NUMBER_REGISTER], 94);
    // SAFETY: 同一受控测试环境仍然有效。
    unsafe { assert_finished(&mut group, ExitStatus::Exited(0xfe)) };
    assert_eq!(snapshot(fault.context()), saved_fault);
    crate::println!("{}: task kernel tests passed", crate::arch::NAME);
}
