//! 在架构 trap 自检的可信用户映射中执行任务生命周期探针。

use core::arch::global_asm;

use super::{ExitStatus, Task, TaskState};
use crate::arch::trap::{
    cause::{ExceptionCause, TrapCause},
    TrapContext,
};

#[cfg(target_arch = "riscv64")]
global_asm!(include_str!("riscv64.S"));
#[cfg(target_arch = "loongarch64")]
global_asm!(include_str!("loongarch64.S"));

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

/// 返回探针的内核地址；调用者按当前用户映射转换为用户入口。
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

/// 验证终止任务再次运行不改变返回结果或已保存的完整现场。
///
/// # Safety
/// 调用者须在本函数内持续满足 [`Task::run`] 的用户映射与独占执行约束。
unsafe fn assert_finished(task: &mut Task, status: ExitStatus) {
    assert_eq!(task.state(), TaskState::Finished(status));
    let saved = snapshot(task.context());
    // SAFETY: 调用者保持原有可信探针、用户映射及私有栈有效；
    // 即使实现错误地再次进入用户态，也不访问暂停的内核状态。
    assert_eq!(unsafe { task.run() }, status);
    assert_eq!(task.state(), TaskState::Finished(status));
    assert_eq!(snapshot(task.context()), saved);
}

/// 执行真实用户任务，验证未知 syscall、两种退出及故障后的继续执行。
///
/// # Safety
/// 调用者须满足 [`Task::run`] 的前提；`user_entry` 必须是 [`entry`] 在用户
/// 映射中的可执行地址，完整探针须可取指并保持各标签相对入口的偏移。
/// `stack` 为架构自检独占的 4096 字节用户栈顶，须按 16 字节对齐。
/// 整个测试期间保持映射有效、中断关闭，且仅在单 CPU 上执行；不提供隔离。
pub unsafe fn kernel_test(user_entry: usize, stack: usize) {
    let user_address =
        |label: unsafe extern "C" fn()| user_entry + (label as *const () as usize - entry());
    let mut task = Task::new(TrapContext::user(user_entry, stack));
    assert_eq!(task.state(), TaskState::Ready);
    // SAFETY: 调用者提供完整可信探针与有效私有栈；用户代码仅操作寄存器，
    // 连续检查两次 -ENOSYS 后以 exit 终止，不修改内核内存。
    assert_eq!(unsafe { task.run() }, ExitStatus::Exited(0x5a));
    assert_eq!(task.context().pc(), user_address(task_test_exit));
    assert_eq!(task.context().registers()[ARGUMENT_REGISTER], 0x1_2345_675a);
    assert_eq!(task.context().registers()[NUMBER_REGISTER], 93);
    // SAFETY: 整个测试仍保持相同的可信代码、私有栈和用户映射有效。
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
    // 即使寄存器看起来像 exit，也不能把 breakpoint 当成 syscall。
    context.registers_mut()[NUMBER_REGISTER] = 93;
    let registers = *context.registers();
    let mut fault = Task::new(context);
    // SAFETY: 此独立入口只执行 breakpoint；使用新建现场和同一有效私有栈，
    // 不依赖前一任务或 syscall 自检留下的上下文。
    assert_eq!(unsafe { fault.run() }, ExitStatus::Faulted);
    assert_eq!(fault.context().pc(), fault_pc);
    assert_eq!(fault.context().registers(), &registers);
    assert_eq!(
        fault.context().cause(),
        TrapCause::Exception(ExceptionCause::Breakpoint),
    );
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
    // SAFETY: 故障探针、用户栈与映射保持有效，故障任务仍由此处独占。
    unsafe { assert_finished(&mut fault, ExitStatus::Faulted) };
    let saved_fault = snapshot(fault.context());

    // 故障返回后运行全新正常任务，证明内核可继续进入用户态并处理 exit_group。
    let mut group = Task::new(TrapContext::user(
        user_address(task_test_group_entry),
        stack,
    ));
    assert_eq!(group.state(), TaskState::Ready);
    // SAFETY: 此独立探针仅设置 exit_group 参数并触发 syscall；
    // 前一任务已同步返回，不会并发使用该架构私有用户栈。
    assert_eq!(unsafe { group.run() }, ExitStatus::Exited(0xfe));
    assert_eq!(group.context().pc(), user_address(task_test_exit_group));
    assert_eq!(
        group.context().registers()[ARGUMENT_REGISTER],
        usize::MAX - 1
    );
    assert_eq!(group.context().registers()[NUMBER_REGISTER], 94);
    // SAFETY: 此正常任务的代码、私有栈与映射仍然有效，且无并发用户执行。
    unsafe { assert_finished(&mut group, ExitStatus::Exited(0xfe)) };
    assert_eq!(snapshot(fault.context()), saved_fault);
    crate::println!("{}: task kernel tests passed", crate::arch::NAME);
}
