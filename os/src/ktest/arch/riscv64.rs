//! RISC-V 64 Trap 与特权级切换自检（仅在启用 `ktest` feature 时编译）。

use core::arch::{asm, global_asm};
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        sscratch,
        sstatus::{self, FS, VS, XS},
    },
};

use super::{run_user, TrapContext, DISABLED_STATUS};

global_asm!(include_str!("riscv64/probes.S"), run_user_shim = sym run_user_shim);

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

const SSTATUS_UXL_MASK: usize = 0b11 << 32;
const SSTATUS_UXL_64: usize = 0b10 << 32;

/// 为汇编 ABI 包装函数提供调用 [`run_user`] 的 `extern "C"` 垫片。
///
/// # Safety
///
/// - 仅可由 `probes.S` 中的 `riscv_kernel_test_call_user` 在 [`super::init`] 完成后调用。
/// - 调用时必须满足 [`run_user`] 的全部安全前提（单核关中断、`context` 独占有效、受控用户探针与私有测试栈可安全访问）。
unsafe extern "C" fn run_user_shim(context: &mut TrapContext) {
    // SAFETY: 汇编探针仅在 `trap::init` 完成后，使用当前测试独占的 `context`、固定探针代码与私有测试栈调用本函数。
    unsafe { run_user(context) };
}

fn assert_cause_decoding() {
    let interrupt_bit = 1usize << (usize::BITS - 1);
    let mut context = TrapContext::user(0, 0);
    context.scause = 5;
    assert_eq!(context.cause(), Ok(Trap::Exception(Exception::LoadFault)));
    assert_eq!(
        alloc::format!("{}", context.cause_for_log()),
        "exception: LoadFault"
    );
    context.scause = interrupt_bit | 5;
    assert_eq!(
        context.cause(),
        Ok(Trap::Interrupt(Interrupt::SupervisorTimer)),
    );
    assert_eq!(
        alloc::format!("{}", context.cause_for_log()),
        "interrupt: SupervisorTimer"
    );
    for code in [10, interrupt_bit - 1] {
        context.scause = code;
        assert_eq!(context.cause(), Err(Trap::Exception(code)));
        assert_eq!(
            alloc::format!("{}", context.cause_for_log()),
            alloc::format!("exception: unknown exception code {code}")
        );
        assert!(!crate::arch::syscall::is_syscall(&context));
        context.scause = interrupt_bit | code;
        assert_eq!(context.cause(), Err(Trap::Interrupt(code)));
        assert_eq!(
            alloc::format!("{}", context.cause_for_log()),
            alloc::format!("interrupt: unknown interrupt code {code}")
        );
        assert!(!crate::arch::syscall::is_syscall(&context));
    }
}

fn assert_kernel_environment() {
    let thread_pointer: usize;
    // SAFETY: 只读取主核的 `tp`，不修改寄存器或内存。
    unsafe {
        asm!(
            "mv {thread_pointer}, tp",
            thread_pointer = out(reg) thread_pointer,
            options(nomem, nostack),
        );
    }
    assert_eq!(sscratch::read(), 0, "kernel sscratch invariant");
    let status = sstatus::read();
    // `sret` 会按硬件规范将 `SPIE` 置 1，此处不要求 `SPIE = 0`，仅检查 `SIE` 与扩展状态位。
    assert!(!status.sie(), "kernel interrupts");
    assert_eq!(status.fs(), FS::Off, "kernel floating-point state");
    assert_eq!(status.vs(), VS::Off, "kernel vector state");
    assert_eq!(status.xs(), XS::AllOff, "kernel extension state");
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

/// 拦截并处理本测试模块专属的两个 S-mode `ebreak` 探针地址；其余内核异常返回 `false` 交由全局 panic 处理。
pub fn handle_kernel_test(context: &mut TrapContext) -> bool {
    if !context.is_breakpoint() {
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
        // SAFETY: 在已 16 字节对齐的当前内核栈上触发内层同步断点，由本函数的 `nested` 分支拦截并跳过。
        unsafe { riscv_kernel_test_nested() };
        assert_kernel_environment();
        assert_kernel_registers(context); // 验证内层 Trap 栈帧未覆盖外层保存的寄存器。
        for (index, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
            if index != 2 {
                *value = 0x200 + index;
            }
        }
    } else {
        assert_eq!(context.sp() & 15, 0, "nested kernel sp alignment");
    }
    // 故意在保存的 `sstatus` 中清零 `SPP` 并置位禁用位，验证 `.Lrv_return_kernel` 能否强制修正回安全的 S-mode 状态。
    context.sstatus = (context.sstatus & !(1 << 8)) | DISABLED_STATUS;
    context.set_pc(context.pc() + 4); // `probes.S` 已禁用压缩指令，两条 `ebreak` 均为 4 字节。
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
    assert_eq!(
        context.raw_status() & SSTATUS_UXL_MASK,
        SSTATUS_UXL_64,
        "RV64 UXL",
    );
    assert_eq!(context.registers()[0], 0);
    for (index, value) in context.registers().iter().enumerate().skip(1) {
        let expected = if index == 2 { stack } else { base + index };
        assert_eq!(*value, expected, "user x{index} restore/save");
    }
    assert_kernel_environment();
}

/// 验证 `scause` 解码、S-mode 嵌套断点恢复以及 U-mode 上下文往返切换。
pub fn kernel_test(env: &mut crate::ktest::user_env::UserEnv) {
    assert_cause_decoding();
    let mut kernel_call = TrapContext::user(0x1000, 0x2000);
    kernel_call.scause = 9; // S-mode `ecall` 不能当成用户态系统调用处理。
    kernel_call.sstatus = 1 << 8;
    crate::syscall::tests::assert_rejected(&mut kernel_call);
    assert_kernel_environment();
    // SAFETY: 汇编探针先备份调用栈 ABI 寄存器，再写入测试特征值并触发由 `handle_kernel_test` 拦截的断点。
    assert_eq!(
        unsafe { riscv_kernel_test_kernel() },
        1,
        "kernel GPR restore"
    );
    assert_kernel_environment();

    let stack = env.stack();
    let first = env.address(riscv_kernel_test_user_first as *const () as usize);
    let second = env.address(riscv_kernel_test_user_second as *const () as usize);
    let privileged = env.address(riscv_kernel_test_user_privileged as *const () as usize);
    let float = env.address(riscv_kernel_test_user_float as *const () as usize);
    let mut context = TrapContext::user(first, stack);
    set_user_registers(&mut context, 0x300, stack);
    // 验证 `run_user` 入口会强制覆写恶意的 `sstatus`。
    context.sstatus = usize::MAX;
    // SAFETY: 用户探针与对齐的私有测试栈均在 RAM 中；汇编包装函数在恢复自身栈帧前会校验内核 ABI 寄存器是否完好。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "initial user-call ABI",
    );
    assert_user_context(&context, 0x300, stack, first, 8);
    assert_eq!(context.bad_addr(), 0);

    context.set_pc(second); // 跨过 `first` 末尾故意放置的非法指令，跳到第二个探针。
    set_user_registers(&mut context, 0x400, stack - 16);
    // SAFETY: 使用修改后的 PC、GPR 与栈指针再次进入同一受控探针环境。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "resumed user-call ABI",
    );
    assert_user_context(&context, 0x400, stack - 16, second, 8);
    assert_eq!(context.bad_addr(), 0);

    context.set_pc(privileged);
    set_user_registers(&mut context, 0x500, stack - 32);
    // SAFETY: 在 U-mode 下执行写 `sscratch` 特权指令，必须触发非法指令异常并返回内核。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "privileged user-call ABI",
    );
    assert_user_context(&context, 0x500, stack - 32, privileged, 2);
    assert!(context.bad_addr() == 0 || context.bad_addr() == 0x1400_1073);

    context.set_pc(float);
    set_user_registers(&mut context, 0x600, stack - 48);
    // SAFETY: `FS = Off` 时在 U-mode 下执行浮点指令，必须触发非法指令异常并返回内核。
    assert_eq!(
        unsafe { riscv_kernel_test_call_user(&mut context) },
        1,
        "FP user-call ABI",
    );
    assert_user_context(&context, 0x600, stack - 48, float, 2);
    assert!(context.bad_addr() == 0 || context.bad_addr() == 0x53);
    crate::println!("riscv64: trap kernel tests passed");
}
