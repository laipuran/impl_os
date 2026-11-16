//! LoongArch 64 Trap、TLB 重填与特权级切换自检（仅在启用 `ktest` feature 时编译）。

use super::cause::{ExceptionCause, InterruptCause, TrapCause};
use super::{run_user, TrapContext};
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicUsize, Ordering};

global_asm!(include_str!("probes.S"), run_user = sym abi_run_user);

/// 为汇编 ABI 包装函数提供调用 [`run_user`] 的 `extern "C"` 垫片。
unsafe extern "C" fn abi_run_user(context: *mut TrapContext) {
    // SAFETY: 仅由本模块的 `probes.S` 调用，传入独占的 `context` 且用户探针与栈已映射。
    unsafe { run_user(&mut *context) };
}

extern "C" {
    fn __loongarch_kernel_test_kernel() -> usize;
    fn __loongarch_kernel_test_call_abi(context: *mut TrapContext) -> usize;
    fn __loongarch_kernel_test_kernel_break();
    fn __loongarch_kernel_test_nested();
    fn __loongarch_kernel_test_nested_break();
    fn __loongarch_kernel_test_refill(address: usize);
    fn __loongarch_kernel_test_refill_load();
    fn __loongarch_kernel_test_user_first();
    fn __loongarch_kernel_test_user_second();
    fn __loongarch_kernel_test_user_privileged();
    fn __loongarch_kernel_test_user_float();
}

static OUTER_SP: AtomicUsize = AtomicUsize::new(0);
static NESTED_COUNT: AtomicUsize = AtomicUsize::new(0);
static KERNEL_TP: AtomicUsize = AtomicUsize::new(0);
static REFILL_COUNT: AtomicUsize = AtomicUsize::new(0);
const REFILL: usize = 1 << 63;
const UNMAPPED: usize = 0x4000_0000;

fn kernel_tp() -> usize {
    let tp;
    // SAFETY: 只读获取当前 `$tp` 寄存器，无副作用。
    unsafe { asm!("move {tp}, $tp", tp = out(reg) tp, options(nomem, nostack)) };
    tp
}

fn decode_test() {
    let decode = |estat| TrapCause::from_estat(estat);
    assert_eq!(decode(1 << 11), TrapCause::Interrupt(InterruptCause::Timer));
    let pending = (1 << 11) | 1;
    assert_eq!(
        decode(pending),
        TrapCause::Interrupt(InterruptCause::Pending(pending))
    );
    assert_eq!(
        decode((12 << 16) | pending),
        TrapCause::Exception(ExceptionCause::Breakpoint)
    );
    assert_eq!(
        decode((8 << 16) | (1 << 22)),
        TrapCause::Exception(ExceptionCause::DataAddressError)
    );
    for (code, subcode) in [(8, 2), (12, 1), (63, 511)] {
        assert_eq!(
            decode((code << 16) | (subcode << 22)),
            TrapCause::Exception(ExceptionCause::Unknown { code, subcode })
        );
    }
    assert_eq!(
        decode(REFILL | (12 << 16) | pending),
        TrapCause::Exception(ExceptionCause::TlbRefill)
    );
}

/// 拦截并处理本测试模块专属的内核态断点与内核态 TLB 重填探针；其余异常返回 `false`。
pub fn handle_kernel_test(context: &mut TrapContext) -> bool {
    if context.cause() == TrapCause::Exception(ExceptionCause::TlbRefill) {
        if context.pc() != __loongarch_kernel_test_refill_load as *const () as usize {
            return false;
        }
        assert_eq!(context.raw_status() & 7, 0);
        assert_eq!(context.bad_addr(), UNMAPPED);
        assert_ne!(OUTER_SP.load(Ordering::Relaxed), 0);
        assert!(context.sp() < OUTER_SP.load(Ordering::Relaxed));
        assert_eq!(context.registers()[2], KERNEL_TP.load(Ordering::Relaxed));
        REFILL_COUNT.fetch_add(1, Ordering::Relaxed);
        context.set_pc(context.pc() + 4);
        return true;
    }
    if context.cause() != TrapCause::Exception(ExceptionCause::Breakpoint) {
        return false;
    }
    let outer_pc = __loongarch_kernel_test_kernel_break as *const () as usize;
    let nested_pc = __loongarch_kernel_test_nested_break as *const () as usize;
    if context.pc() == nested_pc {
        assert_eq!(context.raw_status() & 7, 0);
        assert_ne!(OUTER_SP.load(Ordering::Relaxed), 0);
        assert!(context.sp() < OUTER_SP.load(Ordering::Relaxed));
        assert_eq!(context.sp() & 15, 0);
        assert_eq!(context.registers()[2], KERNEL_TP.load(Ordering::Relaxed));
        NESTED_COUNT.fetch_add(1, Ordering::Relaxed);
        context.set_pc(context.pc() + 4);
        return true;
    }
    if context.pc() != outer_pc {
        return false;
    }
    assert_eq!(context.raw_status() & 7, 0);
    assert_eq!(context.registers()[0], 0);
    assert_eq!(context.sp() & 15, 0);
    assert_eq!(context.registers()[12], context.sp());
    for (register, value) in context.registers().iter().enumerate().skip(1) {
        if register != 3 && register != 12 {
            assert_eq!(*value, 0x600 + register);
        }
    }
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));
    OUTER_SP.store(context.sp(), Ordering::Relaxed);
    // SAFETY: 触发内层同步断点，由上方的 `nested_pc` 分支拦截并将 `ERA` 前移 4 字节后返回。
    unsafe { __loongarch_kernel_test_nested() };
    assert_eq!(NESTED_COUNT.load(Ordering::Relaxed), 1);
    // SAFETY: 访问未映射的低地址 `UNMAPPED` 触发内核态 TLB refill，由上方的 `TlbRefill` 分支拦截并跳过该指令。
    unsafe { __loongarch_kernel_test_refill(UNMAPPED) };
    assert_eq!(REFILL_COUNT.load(Ordering::Relaxed), 1);
    OUTER_SP.store(0, Ordering::Relaxed);
    // 验证内层断点与 TLB refill 均未破坏外层保存的栈帧状态。
    assert_eq!(context.pc(), outer_pc);
    assert_eq!(context.raw_status() & 7, 0);
    assert_eq!(
        context.cause(),
        TrapCause::Exception(ExceptionCause::Breakpoint)
    );
    assert_eq!(context.registers()[2], 0x602);
    // 修改除 `$sp`（`$r3`）以外的全部非零通用寄存器，交由 `probes.S` 逐一核对恢复值。
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x700 + register;
        }
    }
    context.set_pc(context.pc() + 4);
    true
}

fn assert_user(context: &TrapContext, pc: usize, cause: ExceptionCause, regs: &[usize; 32]) {
    assert_eq!(context.pc(), pc);
    assert_eq!(context.raw_status() & 0xf, 3);
    assert_eq!(context.raw_cause() & REFILL, 0);
    assert_eq!(context.cause(), TrapCause::Exception(cause));
    assert_eq!(context.registers(), regs);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));
}

/// 验证 `ESTAT` 解码、内核态嵌套断点/TLB 重填以及 PLV3 用户态切换。
pub fn kernel_test(env: &mut crate::ktest::user_env::UserEnv) {
    decode_test();
    let mut kernel_call = TrapContext::user(0x1000, 0x2000);
    kernel_call.estat = 11 << 16;
    kernel_call.prmd = 0; // PLV0 触发的 `syscall` 不能当成用户态系统调用处理。
    crate::syscall::tests::assert_rejected(&mut kernel_call);
    kernel_call.estat |= REFILL;
    kernel_call.prmd = 3; // 软件重填标记必须优先于残留的 `SystemCall` Ecode。
    crate::syscall::tests::assert_rejected(&mut kernel_call);
    KERNEL_TP.store(kernel_tp(), Ordering::Relaxed);
    NESTED_COUNT.store(0, Ordering::Relaxed);
    REFILL_COUNT.store(0, Ordering::Relaxed);
    // SAFETY: 当前内核完全通过 `DMW0` 运行；清空复位残留的 TLB 表项无副作用，且能保证后续 refill 探针行为确定。
    unsafe { asm!("invtlb 0x0, $zero, $zero", options(nostack)) };
    // SAFETY: 汇编探针先备份调用栈 ABI 寄存器，再触发内核态断点并核对所有恢复后的通用寄存器。
    assert_eq!(unsafe { __loongarch_kernel_test_kernel() }, 1);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));

    let stack = env.stack();
    let first = env.address(__loongarch_kernel_test_user_first as *const () as usize);
    let second = env.address(__loongarch_kernel_test_user_second as *const () as usize);
    let privileged = env.address(__loongarch_kernel_test_user_privileged as *const () as usize);
    let floating = env.address(__loongarch_kernel_test_user_float as *const () as usize);
    let mut context = TrapContext::user(first, stack);
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x1000 + register;
        }
    }
    let first_regs = *context.registers();
    // 故意填入全 1 的 `prmd`，验证 `run_user` 会强制重置特权位与中断位。
    context.prmd = usize::MAX;
    // SAFETY: 汇编 ABI 包装函数通过 `abi_run_user` 执行受控用户探针，并在返回前检查被调用者保存寄存器。
    assert_eq!(unsafe { __loongarch_kernel_test_call_abi(&mut context) }, 1);
    assert_user(&context, first, ExceptionCause::SystemCall, &first_regs);

    context.set_pc(second);
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x2000 + register;
        }
    }
    // 同时修改用户栈指针 `$r3`，确认重入时恢复的是新栈指针。
    context.registers_mut()[3] -= 16;
    let second_regs = *context.registers();
    // SAFETY: 使用更新后的 PC、通用寄存器和栈指针再次进入受控探针。
    assert_eq!(unsafe { __loongarch_kernel_test_call_abi(&mut context) }, 1);
    assert_user(&context, second, ExceptionCause::SystemCall, &second_regs);

    context.set_pc(privileged);
    // SAFETY: 在 PLV3 下执行 `csrrd` 特权指令，必须触发 `InstructionPrivilege` 异常并返回内核。
    unsafe { run_user(&mut context) };
    assert_user(
        &context,
        privileged,
        ExceptionCause::InstructionPrivilege,
        &second_regs,
    );

    context.set_pc(floating);
    // SAFETY: `EUEN.FPE == 0` 时在 PLV3 下执行浮点指令，必须触发 `FloatingPointDisabled` 异常。
    unsafe { run_user(&mut context) };
    assert_user(
        &context,
        floating,
        ExceptionCause::FloatingPointDisabled,
        &second_regs,
    );

    // 临时撤销 `DMW1` 用户态窗口，使下一次进入 PLV3 取指触发 TLB refill。
    env.revoke();
    context.set_pc(first);
    // SAFETY: 未映射的用户取指会由 `TLBRENTRY` 捕获并安全返回内核，内核代码与栈仍由 `DMW0` 映射。
    unsafe { run_user(&mut context) };
    assert_eq!(
        context.cause(),
        TrapCause::Exception(ExceptionCause::TlbRefill)
    );
    assert_ne!(context.raw_cause() & REFILL, 0);
    assert_eq!(context.pc(), first);
    assert_eq!(context.bad_addr(), first);
    assert_eq!(context.raw_status() & 0xf, 3);
    assert_eq!(context.registers(), &second_regs);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));

    // 重新开启 `DMW1` 窗口，确认从 TLB refill 恢复后普通 `syscall` 依然工作正常。
    env.enable();
    // SAFETY: 用户态映射已恢复，继续使用同一独占上下文执行受控 `syscall` 探针。
    unsafe { run_user(&mut context) };
    assert_user(&context, first, ExceptionCause::SystemCall, &second_regs);
    crate::println!("loongarch64: trap kernel tests passed");
}
