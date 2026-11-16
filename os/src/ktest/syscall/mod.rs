//! Linux 系统调用 ABI 自检：验证调用号与 6 个参数提取、返回值写回、PC 前移与边界溢出拦截。

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

/// 返回系统调用测试探针的内核地址（由调用方通过 `UserEnv::address` 转换为用户态入口地址）。
pub fn entry() -> usize {
    syscall_test_entry as *const () as usize
}

/// 验证非用户态系统调用的 `context` 会被 [`decode`] 与 [`complete`] 拒绝且现场保持原样。
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
    // 在已捕获的系统调用上下文上直接测试 `usize::MAX` 附近的 PC 边界（不实际跳转到该边界地址执行）。
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
    // 还原探针的真实现场，以便后续继续执行到末尾的 `syscall_test_done` 断点。
    context.set_pc(pc);
    *context.registers_mut() = registers;
}

/// 连续执行 3 次用户态系统调用，验证参数解码、返回值写回、PC 前移及异常拒绝行为。
///
/// # Safety
///
/// - 调用方必须满足 [`run_user`] 的安全前提；
/// - `user_entry` 必须是 [`entry`] 在当前 `UserEnv` 映射下的可执行地址，且 `stack` 为 16 字节对齐的独占用户栈顶。
pub unsafe fn kernel_test(user_entry: usize, stack: usize) {
    let mut context = TrapContext::user(user_entry, stack);
    let mut args = [usize::MAX, 0x102, 0x203, 0x304, 0x405, 1usize << 63];
    // 覆盖超过 32 位的正返回值与负 errno，确保 64 位寄存器完整写回。
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
        // SAFETY: 用户探针与测试栈均已映射；探针仅设置寄存器并执行系统调用指令，不触碰内核内存。
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
        // 汇编探针从系统调用返回后会将 `a0` 移入 `a5`（第 6 个参数），以此验证用户态实际观测到了返回值。
        args[5] = result as usize;
    }
    assert_completion_boundary(&mut context);
    // SAFETY: 完成第 3 次系统调用后恢复执行，探针将直接执行末尾的断点指令返回。
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
