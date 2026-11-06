//! 仅由 feature 启用的真实 PLV0/PLV3 执行探针；不链接到正常启动路径。

use super::cause::{ExceptionCause, InterruptCause, TrapCause};
use super::{run_user, TrapContext};
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicUsize, Ordering};

global_asm!(include_str!("probes.S"), run_user = sym abi_run_user);

/// 为汇编 ABI 探针提供包裹 Rust API 的显式 C 调用边界。
unsafe extern "C" fn abi_run_user(context: *mut TrapContext) {
    // SAFETY: 仅由 feature 启用的 wrapper 转发其独占 context；
    // kernel_test 提供已映射的测试代码，并保持内核 frame 有效。
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

#[repr(C, align(16))]
struct UserStack([u8; 4096]);
static mut USER_STACK: UserStack = UserStack([0; 4096]);
static OUTER_SP: AtomicUsize = AtomicUsize::new(0);
static NESTED_COUNT: AtomicUsize = AtomicUsize::new(0);
static KERNEL_TP: AtomicUsize = AtomicUsize::new(0);
static REFILL_COUNT: AtomicUsize = AtomicUsize::new(0);
const REFILL: usize = 1 << 63;
const UNMAPPED: usize = 0x4000_0000;

fn kernel_tp() -> usize {
    let tp;
    // SAFETY: 读取当前整数 thread pointer 不产生副作用。
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

/// 仅识别该 feature 专用的精确 breakpoint 及内核 refill 探针。
/// 其他异常交由共用的内核致命 trap handler 处理。
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
    // SAFETY: 上方会识别此精确 PLV0 breakpoint；它使用当前有效内核栈，
    // 并在保存的 ERA 前移后返回。
    unsafe { __loongarch_kernel_test_nested() };
    assert_eq!(NESTED_COUNT.load(Ordering::Relaxed), 1);
    // SAFETY: 此地址不存在 PLV0 低地址 DMW 或 TLB 映射。
    // 上方会识别精确 refill 探针并跳过，不读取该地址。
    unsafe { __loongarch_kernel_test_refill(UNMAPPED) };
    assert_eq!(REFILL_COUNT.load(Ordering::Relaxed), 1);
    OUTER_SP.store(0, Ordering::Relaxed);
    // 嵌套 trap 会覆盖当前异常 CSR，但不覆盖外层保存的 frame。
    assert_eq!(context.pc(), outer_pc);
    assert_eq!(context.raw_status() & 7, 0);
    assert_eq!(
        context.cause(),
        TrapCause::Exception(ExceptionCause::Breakpoint)
    );
    assert_eq!(context.registers()[2], 0x602);
    // 汇编探针在恢复调用者前检查每个修改后的 GPR。
    // 仅保持 sp 不变，以保证被中断的内核 frame 有效。
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x700 + register;
        }
    }
    context.set_pc(context.pc() + 4);
    true
}

fn user_address(address: usize) -> usize {
    address - 0x9000_0000_0000_0000
}

fn assert_user(context: &TrapContext, pc: usize, cause: ExceptionCause, regs: &[usize; 32]) {
    assert_eq!(context.pc(), pc);
    assert_eq!(context.raw_status() & 0xf, 3);
    assert_eq!(context.raw_cause() & REFILL, 0);
    assert_eq!(context.cause(), TrapCause::Exception(cause));
    assert_eq!(context.registers(), regs);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));
}

/// 测试异常解码、嵌套内核 break/refill、用户态现场切换与 Linux syscall ABI。
///
/// 临时低地址 DMW1 别名允许 PLV3 访问物理 RAM。它映射所有 RAM，
/// 不提供隔离，仅用于此受控内核测试代码；
/// 函数报告成功前会恢复原始 DMW1。
pub fn kernel_test() {
    decode_test();
    let mut kernel_call = TrapContext::user(0x1000, 0x2000);
    kernel_call.estat = 11 << 16;
    kernel_call.prmd = 0; // PLV0 syscall 与用户 syscall 使用相同 Ecode。
    crate::syscall::tests::assert_rejected(&mut kernel_call);
    kernel_call.estat |= REFILL;
    kernel_call.prmd = 3; // refill 标记必须优先于残留的 syscall Ecode。
    crate::syscall::tests::assert_rejected(&mut kernel_call);
    KERNEL_TP.store(kernel_tp(), Ordering::Relaxed);
    NESTED_COUNT.store(0, Ordering::Relaxed);
    REFILL_COUNT.store(0, Ordering::Relaxed);
    // SAFETY: 此早期启动内核测试在页表建立前仅通过 DMW0 运行；
    // 丢弃复位时的 TLB 内容无害，
    // 并使两个预期的 refill 探针行为确定。
    unsafe { asm!("invtlb 0x0, $zero, $zero", options(nostack)) };
    // SAFETY: 汇编 wrapper 保留内核 ABI，并在恢复调用者 ABI 状态前
    // 检查每个恢复的整数寄存器。
    assert_eq!(unsafe { __loongarch_kernel_test_kernel() }, 1);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));

    let previous_window: usize;
    // SAFETY: 单 CPU 的 feature 专用启动路径独占 DMW1。MAT=CC，启用 PLV3，
    // VSEG=0；高地址、仅 PLV0 可用的 DMW0 保持不变。这不提供内存隔离。
    unsafe {
        asm!(
            "csrrd {saved}, 0x181",
            "li.w {tmp}, 0x18",
            "dbar 0",
            "csrwr {tmp}, 0x181",
            "invtlb 0x0, $zero, $zero",
            "ibar 0",
            saved = out(reg) previous_window,
            tmp = out(reg) _,
            options(nostack),
        );
    }

    // SAFETY: 构造单 CPU 内核测试栈的 raw pointer 不创建引用或别名；
    // 仅受控用户 context 使用此 RAM。
    let stack = user_address(unsafe { core::ptr::addr_of_mut!(USER_STACK.0) } as usize + 4096);
    let first = user_address(__loongarch_kernel_test_user_first as *const () as usize);
    let second = user_address(__loongarch_kernel_test_user_second as *const () as usize);
    let privileged = user_address(__loongarch_kernel_test_user_privileged as *const () as usize);
    let floating = user_address(__loongarch_kernel_test_user_float as *const () as usize);
    let mut context = TrapContext::user(first, stack);
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x1000 + register;
        }
    }
    let first_regs = *context.registers();
    // 注入私有状态，验证公共调用者无法保留恶意返回特权位或中断位；
    // API 不提供 status setter。
    context.prmd = usize::MAX;
    // SAFETY: ABI wrapper 通过 C shim 调用 Rust run_user；
    // 其受控代码仅触发 trap，无法访问暂停的内核 frame。
    assert_eq!(unsafe { __loongarch_kernel_test_call_abi(&mut context) }, 1);
    assert_user(&context, first, ExceptionCause::SystemCall, &first_regs);

    context.set_pc(second);
    for (register, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x2000 + register;
        }
    }
    // 同时修改栈，验证 r3 被恢复而非沿用。
    context.registers_mut()[3] -= 16;
    let second_regs = *context.registers();
    // SAFETY: 同一 ABI 探针使用独占、已映射 context 中修改后的 PC、
    // GPR 和栈，再次执行真实 syscall。
    assert_eq!(unsafe { __loongarch_kernel_test_call_abi(&mut context) }, 1);
    assert_user(&context, second, ExceptionCause::SystemCall, &second_regs);

    context.set_pc(privileged);
    // SAFETY: 已映射探针尝试在 PLV3 读取 CSR，必须触发 trap，
    // 不得改变任何整数寄存器或获得特权访问。
    unsafe { run_user(&mut context) };
    assert_user(
        &context,
        privileged,
        ExceptionCause::InstructionPrivilege,
        &second_regs,
    );

    context.set_pc(floating);
    // SAFETY: EUEN.FPE 为零，因此已映射浮点指令必须在访问扩展状态前
    // 触发 trap；测试不依赖 FP 内容。
    unsafe { run_user(&mut context) };
    assert_user(
        &context,
        floating,
        ExceptionCause::FloatingPointDisabled,
        &second_regs,
    );

    // 移除整个用户别名并使缓存的地址转换失效，令下一次取指发生 TLB miss。
    // 此层不遍历页表。
    // SAFETY: 仅修改 feature 专用用户窗口；内核 DMW0 保持映射，
    // 受控用户 context 无法执行任何指令。
    unsafe {
        asm!(
            "csrwr $zero, 0x181",
            "invtlb 0x0, $zero, $zero",
            "ibar 0",
            options(nostack),
        )
    };
    context.set_pc(first);
    // SAFETY: 故意不映射的取指由 TLBRENTRY 捕获，
    // context 和暂停的内核栈仍通过 DMW0 保持映射。
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

    // SAFETY: 恢复相同的 feature 专用低地址用户别名，验证特殊 refill
    // 返回路径之后正常异常处理仍然有效。
    unsafe {
        asm!(
            "li.w {tmp}, 0x18",
            "csrwr {tmp}, 0x181",
            "invtlb 0x0, $zero, $zero",
            "ibar 0",
            tmp = out(reg) _,
            options(nostack),
        );
    }
    // SAFETY: 原始受控 syscall 指令再次被映射，
    // 仍使用同一独占 context 及有效用户栈。
    unsafe { run_user(&mut context) };
    assert_user(&context, first, ExceptionCause::SystemCall, &second_regs);
    // SAFETY: DMW1 仍提供固定探针与独占栈的 PLV3 别名；中断关闭，
    // 探针只操作寄存器并返回 trap，不访问暂停的内核状态。
    unsafe {
        crate::syscall::tests::kernel_test(user_address(crate::syscall::tests::entry()), stack)
    };

    // SAFETY: 所有用户执行均已返回高地址内核映射；
    // 恢复 DMW1，在正常启动前移除 feature 专用用户映射。
    unsafe {
        asm!(
            "dbar 0",
            "csrwr {saved}, 0x181",
            "invtlb 0x0, $zero, $zero",
            "ibar 0",
            saved = inout(reg) previous_window => _,
            options(nostack),
        );
    }
    crate::println!("loongarch64: trap kernel tests passed");
}
