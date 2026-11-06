//! 在 trap 自检提供的可信用户执行环境中验证 syscall ABI。

use core::arch::global_asm;

use super::{complete, decode, SyscallRequest};
use crate::arch::trap::{
    cause::{ExceptionCause, TrapCause},
    run_user, TrapContext,
};

#[cfg(target_arch = "riscv64")]
global_asm!(include_str!("riscv64.S"));
#[cfg(target_arch = "loongarch64")]
global_asm!(include_str!("loongarch64.S"));

extern "C" {
    fn syscall_test_entry();
    fn syscall_test_first();
    fn syscall_test_second();
    fn syscall_test_third();
    fn syscall_test_done();
}

/// 返回探针的内核地址；调用者按当前用户映射转换为用户入口。
pub fn entry() -> usize {
    syscall_test_entry as *const () as usize
}

/// 不能作为用户 syscall 处理的现场必须被拒绝，且 PC、整数状态及异常信息不变。
pub fn assert_rejected(context: &mut TrapContext) {
    let registers = *context.registers();
    let metadata = (
        context.pc(),
        context.raw_status(),
        context.raw_cause(),
        context.bad_addr(),
    );
    assert_eq!(decode(context), None);
    assert!(!complete(context, -38));
    assert_eq!(context.registers(), &registers);
    assert_eq!(
        (
            context.pc(),
            context.raw_status(),
            context.raw_cause(),
            context.bad_addr(),
        ),
        metadata,
    );
}

fn assert_completion_boundary(context: &mut TrapContext) {
    let pc = context.pc();
    let registers = *context.registers();
    let metadata = (
        context.raw_status(),
        context.raw_cause(),
        context.bad_addr(),
    );
    // 只修改已捕获的 syscall 现场，不实际执行这些机器字边界地址。
    context.set_pc(usize::MAX - 7);
    assert!(complete(context, -38));
    assert_eq!(context.pc(), usize::MAX - 3);
    let boundary_registers = *context.registers();
    assert!(
        !complete(context, 7),
        "syscall PC overflow must be rejected"
    );
    assert_eq!(context.pc(), usize::MAX - 3);
    assert_eq!(context.registers(), &boundary_registers);
    assert_eq!(
        (
            context.raw_status(),
            context.raw_cause(),
            context.bad_addr()
        ),
        metadata,
    );
    // 恢复真实探针现场，后续继续执行终止 breakpoint。
    context.set_pc(pc);
    *context.registers_mut() = registers;
}

/// 执行三次连续用户 syscall，验证完整参数、结果可见性和异常拒绝。
///
/// # Safety
/// 调用者须满足 [`run_user`] 的前提；`user_entry` 必须是 [`entry`] 在用户
/// 执行环境中的可执行地址，完整探针须可取指，且保持各标签相对入口的偏移。
/// 提供 16 字节对齐的可写用户栈，并保持中断关闭。
/// 仅可在单 CPU 自检中使用，不提供用户隔离。
pub unsafe fn kernel_test(user_entry: usize, stack: usize) {
    let mut context = TrapContext::user(user_entry, stack);
    let mut args = [usize::MAX, 0x102, 0x203, 0x304, 0x405, 1usize << 63];
    // 成功值超过 32 位，错误值为负数；均须完整返回用户寄存器。
    let success = 0x1_2345_6789isize;
    #[cfg(target_arch = "riscv64")]
    let result_register = 10;
    #[cfg(target_arch = "loongarch64")]
    let result_register = 4;
    for (trap, number, result) in [
        (
            syscall_test_first as *const () as usize,
            0x1_0000_0123,
            success,
        ),
        (syscall_test_second as *const () as usize, 0x456, -38),
        (syscall_test_third as *const () as usize, 0x789, 0),
    ] {
        // SAFETY: 调用者提供映射与私有栈；固定探针只操作寄存器并触发 trap，
        // 不访问或修改暂停的内核现场。
        unsafe { run_user(&mut context) };
        let pc = user_entry + (trap - entry());
        assert_eq!(context.pc(), pc);
        let registers = *context.registers();
        let metadata = (
            context.raw_status(),
            context.raw_cause(),
            context.bad_addr(),
        );
        assert_eq!(decode(&context), Some(SyscallRequest { number, args }));
        assert_eq!(context.registers(), &registers);
        assert_eq!(context.pc(), pc);
        assert!(complete(&mut context, result));
        let mut expected_registers = registers;
        expected_registers[result_register] = result as usize;
        assert_eq!(context.registers(), &expected_registers);
        assert_eq!(context.pc(), pc + 4);
        assert_eq!(
            (
                context.raw_status(),
                context.raw_cause(),
                context.bad_addr()
            ),
            metadata,
        );
        // 前两次用户恢复后，探针将返回值复制到下一次调用的第六参数，
        // 证明用户观察到了结果、其余参数未被破坏，且 PC 没有停留在原 syscall。
        args[5] = result as usize;
    }
    assert_completion_boundary(&mut context);
    // SAFETY: 最后一次完成后继续执行固定探针，必须到达终止 breakpoint。
    unsafe { run_user(&mut context) };
    assert_eq!(
        context.pc(),
        user_entry + (syscall_test_done as *const () as usize - entry()),
    );
    assert_eq!(
        context.cause(),
        TrapCause::Exception(ExceptionCause::Breakpoint)
    );
    assert_rejected(&mut context);
    crate::println!("{}: syscall ABI tests passed", crate::arch::NAME);
}
