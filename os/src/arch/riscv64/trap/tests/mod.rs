//! 仅由 feature 启用的真实 trap 探针；不模拟执行，也不提供生产环境用户内存策略。

use core::arch::{asm, global_asm};

use super::cause::{ExceptionCause, InterruptCause, TrapCause};
use super::{run_user, TrapContext, DISABLED_STATUS, USER_STATUS};

global_asm!(include_str!("probes.S"), run_user_shim = sym run_user_shim);

extern "C" {
    fn riscv_kernel_test_kernel() -> usize;
    fn riscv_kernel_test_kernel_break();
    fn riscv_kernel_test_nested();
    fn riscv_kernel_test_nested_break();
    fn riscv_kernel_test_call_user(context: &mut TrapContext) -> usize;
    fn riscv_kernel_test_user_first();
    fn riscv_kernel_test_user_second();
    fn riscv_kernel_test_user_privileged();
    fn riscv_kernel_test_user_float();
}

#[repr(C, align(16))]
struct UserStack([u8; 4096]);

static mut USER_STACK: UserStack = UserStack([0; 4096]);

unsafe extern "C" fn run_user_shim(context: &mut TrapContext) {
    // SAFETY: 汇编 wrapper 仅在 trap 初始化后使用内核测试独占的 context、
    // 固定用户代码及有效的私有栈调用。
    unsafe { run_user(context) };
}

fn assert_cause_decoding() {
    let interrupt_bit = 1usize << (usize::BITS - 1);
    assert_eq!(
        TrapCause::from_scause(5),
        TrapCause::Exception(ExceptionCause::LoadAccessFault),
    );
    assert_eq!(
        TrapCause::from_scause(interrupt_bit | 5),
        TrapCause::Interrupt(InterruptCause::SupervisorTimer),
    );
    for code in [10, interrupt_bit - 1] {
        assert_eq!(
            TrapCause::from_scause(code),
            TrapCause::Exception(ExceptionCause::Unknown(code)),
        );
        assert_eq!(
            TrapCause::from_scause(interrupt_bit | code),
            TrapCause::Interrupt(InterruptCause::Unknown(code)),
        );
    }
}

fn assert_kernel_environment() {
    let scratch: usize;
    let status: usize;
    let thread_pointer: usize;
    // SAFETY: 这些探针仅在单个 boot hart 的 S-mode 测试中执行。
    unsafe {
        asm!(
            "csrr {scratch}, sscratch",
            "csrr {status}, sstatus",
            "mv {thread_pointer}, tp",
            scratch = out(reg) scratch,
            status = out(reg) status,
            thread_pointer = out(reg) thread_pointer,
            options(nomem, nostack),
        );
    }
    assert_eq!(scratch, 0, "kernel sscratch invariant");
    // sret 按架构规则设置 SPIE=1，但当前 SIE 必须保持为零。
    assert_eq!(
        status & (DISABLED_STATUS & !(1 << 5)),
        0,
        "kernel interrupts/extensions",
    );
    assert_eq!(thread_pointer, 0, "kernel tp must not come from user state");
}

fn assert_kernel_registers(context: &TrapContext) {
    assert_eq!(context.registers()[0], 0);
    assert_eq!(
        context.sp(),
        context as *const TrapContext as usize + core::mem::size_of::<TrapContext>(),
        "kernel interrupted sp",
    );
    assert_eq!(context.sp() & 15, 0);
    for (index, value) in context.registers().iter().enumerate().skip(1) {
        if index != 2 {
            assert_eq!(*value, 0x100 + index, "kernel x{index} save");
        }
    }
}

/// 仅识别此内核测试拥有的两个精确 supervisor breakpoint PC。
/// 其他 trap 仍由共用的内核致命 trap 策略处理。
pub fn handle_kernel_test(context: &mut TrapContext) -> bool {
    if context.cause() != TrapCause::Exception(ExceptionCause::Breakpoint) {
        return false;
    }
    let outer = riscv_kernel_test_kernel_break as *const () as usize;
    let nested = riscv_kernel_test_nested_break as *const () as usize;
    if context.pc() != outer && context.pc() != nested {
        return false;
    }
    assert_kernel_environment();
    assert_ne!(context.raw_status() & (1 << 8), 0, "kernel SPP");
    assert_eq!(context.raw_status() & DISABLED_STATUS, 0);
    assert!(context.bad_addr() == 0 || context.bad_addr() == context.pc());
    if context.pc() == outer {
        assert_kernel_registers(context);
        // SAFETY: 此汇编叶函数在当前对齐的内核栈上触发一个已知 breakpoint；
        // 下方分支会处理其精确 PC。
        unsafe { riscv_kernel_test_nested() };
        assert_kernel_environment();
        assert_kernel_registers(context); // 内层 frame 不得与此外层 frame 重叠。
        for (index, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
            if index != 2 {
                *value = 0x200 + index;
            }
        }
    } else {
        assert_eq!(context.sp() & 15, 0, "nested kernel sp alignment");
    }
    // 除初始硬件状态保存外，也测试内核返回时的状态清理：清除 SPP
    // 不得返回 U mode，且这些中断及扩展位必须被清除。
    context.sstatus = (context.sstatus & !(1 << 8)) | DISABLED_STATUS;
    context.set_pc(context.pc() + 4); // 两处均强制使用非压缩指令。
    true
}

fn set_user_registers(context: &mut TrapContext, base: usize, stack: usize) {
    for (index, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        *value = base + index;
    }
    context.registers_mut()[0] = 0;
    context.registers_mut()[2] = stack;
}

fn assert_user_context(context: &TrapContext, base: usize, stack: usize, pc: usize, cause: usize) {
    assert_eq!(context.pc(), pc, "user exception PC");
    assert_eq!(context.raw_cause(), cause, "user exception cause");
    assert_eq!(context.raw_status() & ((1 << 8) | DISABLED_STATUS), 0);
    assert_eq!(context.raw_status() & (3 << 32), USER_STATUS, "RV64 UXL");
    assert_eq!(context.registers()[0], 0);
    for (index, value) in context.registers().iter().enumerate().skip(1) {
        let expected = if index == 2 { stack } else { base + index };
        assert_eq!(*value, expected, "user x{index} restore/save");
    }
    assert_kernel_environment();
}

/// 测试异常解码、嵌套 S-mode ebreak、用户态现场切换、syscall ABI 与任务运行。
///
/// 用户探针要求 SBI PMP 在 bare mode 下允许访问 RAM。它们不映射内存，
/// 也不提供隔离，仅执行这些固定且可信的代码片段。
pub fn kernel_test() {
    assert_cause_decoding();
    let mut kernel_call = TrapContext::user(0x1000, 0x2000);
    kernel_call.scause = 9; // S-mode ecall 不是用户 syscall。
    kernel_call.sstatus = 1 << 8;
    crate::syscall::tests::assert_rejected(&mut kernel_call);
    assert_kernel_environment();
    // SAFETY: 汇编保存内核 ABI，加载哨兵寄存器值，
    // 并触发 handle_kernel_test 识别的精确内核测试 breakpoint。
    assert_eq!(
        unsafe { riscv_kernel_test_kernel() },
        1,
        "kernel GPR restore"
    );
    assert_kernel_environment();

    let stack = core::ptr::addr_of_mut!(USER_STACK) as usize + core::mem::size_of::<UserStack>();
    let first = riscv_kernel_test_user_first as *const () as usize;
    let second = riscv_kernel_test_user_second as *const () as usize;
    let privileged = riscv_kernel_test_user_privileged as *const () as usize;
    let float = riscv_kernel_test_user_float as *const () as usize;
    let mut context = TrapContext::user(first, stack);
    set_user_registers(&mut context, 0x300, stack);
    context.sstatus = usize::MAX; // 入口必须丢弃恶意特权位。
                                  // SAFETY: 固定用户代码与对齐私有栈位于 PMP 可访问的 RAM 中；
                                  // 汇编 wrapper 保留有效的内核 gp/tp，并在恢复调用者状态前
                                  // 检查 Rust run_user 调用的 ABI。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "initial user-call ABI",
    );
    assert_user_context(&context, 0x300, stack, first, 8);
    assert_eq!(context.bad_addr(), 0);

    context.set_pc(second); // 跳过 ecall 后故意设置的非法指令。
    set_user_registers(&mut context, 0x400, stack - 16);
    // SAFETY: 使用与上文相同的有效私有内存和固定 trap 代码；
    // 再次进入时必须采用修改后的用户 context，且仍保留调用 ABI。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "resumed user-call ABI",
    );
    assert_user_context(&context, 0x400, stack - 16, second, 8);
    assert_eq!(context.bad_addr(), 0);

    context.set_pc(privileged);
    set_user_registers(&mut context, 0x500, stack - 32);
    // SAFETY: 此 CSR 指令必须在 U mode 触发异常，且不访问数据。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "privileged user-call ABI",
    );
    assert_user_context(&context, 0x500, stack - 32, privileged, 2);
    assert!(context.bad_addr() == 0 || context.bad_addr() == 0x1400_1073);

    context.set_pc(float);
    set_user_registers(&mut context, 0x600, stack - 48);
    // SAFETY: 固定 FP 指令必须在 FS Off 时触发异常，且不访问 RAM。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "FP user-call ABI",
    );
    assert_user_context(&context, 0x600, stack - 48, float, 2);
    assert!(context.bad_addr() == 0 || context.bad_addr() == 0x53);
    // SAFETY: 与已有用户探针相同，入口与独占栈位于 PMP 允许访问的 RAM；
    // 中断关闭，固定 syscall 探针不访问内核数据，不提供隔离。
    unsafe { crate::syscall::tests::kernel_test(crate::syscall::tests::entry(), stack) };
    // SAFETY: 固定任务探针继续使用相同的可信用户执行环境及独占栈；
    // 各任务同步返回后才复用该栈，中断始终关闭。
    unsafe { crate::task::tests::kernel_test(crate::task::tests::entry(), stack) };
    crate::println!("riscv64: trap kernel tests passed");
}
