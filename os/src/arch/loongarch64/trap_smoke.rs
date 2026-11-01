//! Feature-only real PLV0/PLV3 execution probes; never linked into normal boot.

use super::{run_user, TrapContext};
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicUsize, Ordering};

global_asm!(include_str!("trap_smoke.S"));

extern "C" {
    fn __loongarch_smoke_kernel() -> usize;
    fn __loongarch_smoke_kernel_break();
    fn __loongarch_smoke_nested();
    fn __loongarch_smoke_nested_break();
    fn __loongarch_smoke_refill(address: usize);
    fn __loongarch_smoke_refill_load();
    fn __loongarch_smoke_user_first();
    fn __loongarch_smoke_user_second();
    fn __loongarch_smoke_user_privileged();
    fn __loongarch_smoke_user_float();
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
    // SAFETY: reading the current integer thread pointer has no side effects.
    unsafe { asm!("move {tp}, $tp", tp = out(reg) tp, options(nomem, nostack)) };
    tp
}

fn ecode(context: &TrapContext) -> usize {
    (context.cause >> 16) & 0x3f
}

/// Recognize only the exact feature-only breakpoints and kernel refill probe.
/// All other exceptions are left for the shared fatal kernel-trap handler.
pub fn handle_smoke_kernel(context: &mut TrapContext) -> bool {
    if context.cause & REFILL != 0 {
        if context.pc != __loongarch_smoke_refill_load as *const () as usize {
            return false;
        }
        assert_eq!(context.status & 7, 0);
        assert_eq!(context.bad_addr, UNMAPPED);
        assert_ne!(OUTER_SP.load(Ordering::Relaxed), 0);
        assert!(context.regs[3] < OUTER_SP.load(Ordering::Relaxed));
        assert_eq!(context.regs[2], KERNEL_TP.load(Ordering::Relaxed));
        REFILL_COUNT.fetch_add(1, Ordering::Relaxed);
        context.pc += 4;
        return true;
    }
    if ecode(context) != 12 {
        return false;
    }
    let outer_pc = __loongarch_smoke_kernel_break as *const () as usize;
    let nested_pc = __loongarch_smoke_nested_break as *const () as usize;
    if context.pc == nested_pc {
        assert_eq!(context.status & 7, 0);
        assert_ne!(OUTER_SP.load(Ordering::Relaxed), 0);
        assert!(context.regs[3] < OUTER_SP.load(Ordering::Relaxed));
        assert_eq!(context.regs[3] & 15, 0);
        assert_eq!(context.regs[2], KERNEL_TP.load(Ordering::Relaxed));
        NESTED_COUNT.fetch_add(1, Ordering::Relaxed);
        context.pc += 4;
        return true;
    }
    if context.pc != outer_pc {
        return false;
    }
    assert_eq!(context.status & 7, 0);
    assert_eq!(context.regs[0], 0);
    assert_eq!(context.regs[3] & 15, 0);
    assert_eq!(context.regs[12], context.regs[3]);
    for (register, value) in context.regs.iter().enumerate().skip(1) {
        if register != 3 && register != 12 {
            assert_eq!(*value, 0x600 + register);
        }
    }
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));
    OUTER_SP.store(context.regs[3], Ordering::Relaxed);
    // SAFETY: this exact PLV0 breakpoint is recognized above, uses the current
    // valid kernel stack, and returns after its saved ERA is advanced.
    unsafe { __loongarch_smoke_nested() };
    assert_eq!(NESTED_COUNT.load(Ordering::Relaxed), 1);
    // SAFETY: no PLV0 low DMW or TLB mapping exists for this address. The
    // exact refill probe is recognized above and skipped without reading it.
    unsafe { __loongarch_smoke_refill(UNMAPPED) };
    assert_eq!(REFILL_COUNT.load(Ordering::Relaxed), 1);
    OUTER_SP.store(0, Ordering::Relaxed);
    // The nested trap overwrote live exception CSRs but not the outer frame.
    assert_eq!(context.pc, outer_pc);
    assert_eq!(context.status & 7, 0);
    assert_eq!(ecode(context), 12);
    assert_eq!(context.regs[2], 0x602);
    context.regs[4] = 0x777;
    context.pc += 4;
    true
}

fn user_address(address: usize) -> usize {
    address - 0x9000_0000_0000_0000
}

fn assert_user(context: &TrapContext, pc: usize, cause: usize, regs: &[usize; 32]) {
    assert_eq!(context.pc, pc);
    assert_eq!(context.status & 0xf, 3);
    assert_eq!(context.cause & REFILL, 0);
    assert_eq!(ecode(context), cause);
    assert_eq!(&context.regs, regs);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));
}

/// Exercise nested kernel break/refill traps and six user exception round trips.
///
/// A temporary low DMW1 alias permits PLV3 to access physical RAM. It maps all
/// RAM without isolation and exists ONLY for this controlled smoke payload;
/// the original DMW1 is restored before this function reports success.
pub fn smoke_test() {
    KERNEL_TP.store(kernel_tp(), Ordering::Relaxed);
    NESTED_COUNT.store(0, Ordering::Relaxed);
    REFILL_COUNT.store(0, Ordering::Relaxed);
    // SAFETY: this early boot smoke runs exclusively through DMW0 before
    // any page tables exist; discarding reset-time TLB contents is harmless
    // and makes both intentional refill probes deterministic.
    unsafe { asm!("invtlb 0x0, $zero, $zero", options(nostack)) };
    // SAFETY: the assembly wrapper preserves the kernel ABI, and the exact
    // breakpoint handler validates its frame and supplies the return value.
    assert_eq!(unsafe { __loongarch_smoke_kernel() }, 0x777);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));

    let previous_window: usize;
    // SAFETY: single-CPU feature-only boot owns DMW1. MAT=CC, PLV3 enabled,
    // VSEG=0; high PLV0-only DMW0 is unchanged. This is NOT memory isolation.
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

    // SAFETY: forming a raw pointer to the single-CPU smoke stack creates
    // no reference or alias; only the controlled user context uses this RAM.
    let stack = user_address(unsafe { core::ptr::addr_of_mut!(USER_STACK.0) } as usize + 4096);
    let first = user_address(__loongarch_smoke_user_first as *const () as usize);
    let second = user_address(__loongarch_smoke_user_second as *const () as usize);
    let privileged = user_address(__loongarch_smoke_user_privileged as *const () as usize);
    let floating = user_address(__loongarch_smoke_user_float as *const () as usize);
    let mut context = TrapContext::user(first, stack);
    for (register, value) in context.regs.iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x1000 + register;
        }
    }
    let first_regs = context.regs;
    // Deliberately hostile return privilege/interrupt bits must be sanitized.
    context.status = usize::MAX;
    // SAFETY: the controlled payload only traps, uses the temporary low DMW
    // alias, and never accesses the exclusive context or suspended kernel stack.
    unsafe { run_user(&mut context) };
    assert_user(&context, first, 11, &first_regs);

    context.pc = second;
    for (register, value) in context.regs.iter_mut().enumerate().skip(1) {
        if register != 3 {
            *value = 0x2000 + register;
        }
    }
    // Change the stack as well, proving r3 is restored rather than retained.
    context.regs[3] -= 16;
    let second_regs = context.regs;
    // SAFETY: this second controlled syscall is mapped by the same DMW and
    // uses the same exclusively owned state; all restored GPRs are checked.
    unsafe { run_user(&mut context) };
    assert_user(&context, second, 11, &second_regs);

    context.pc = privileged;
    // SAFETY: the mapped probe attempts a CSR read at PLV3, which must trap
    // without changing any integer registers or granting privileged access.
    unsafe { run_user(&mut context) };
    assert_user(&context, privileged, 14, &second_regs);

    context.pc = floating;
    // SAFETY: EUEN.FPE is zero, so this mapped floating instruction must trap
    // before touching extension state; the test does not depend on FP contents.
    unsafe { run_user(&mut context) };
    assert_user(&context, floating, 15, &second_regs);

    // Remove the entire user alias and invalidate cached translations, making
    // the next fetch miss the TLB. This layer does not walk page tables.
    // SAFETY: only the feature-only user window changes; kernel DMW0 stays
    // mapped and the controlled user context cannot execute any instruction.
    unsafe {
        asm!(
            "csrwr $zero, 0x181",
            "invtlb 0x0, $zero, $zero",
            "ibar 0",
            options(nostack),
        )
    };
    context.pc = first;
    // SAFETY: the intentionally unmapped fetch is captured by TLBRENTRY, and
    // the context and suspended kernel stack remain mapped through DMW0.
    unsafe { run_user(&mut context) };
    assert_ne!(
        context.cause & REFILL,
        0,
        "user refill: cause={:#x}, pc={:#x}, bad_addr={:#x}",
        context.cause,
        context.pc,
        context.bad_addr,
    );
    assert_eq!(context.pc, first);
    assert_eq!(context.bad_addr, first);
    assert_eq!(context.status & 0xf, 3);
    assert_eq!(context.regs, second_regs);
    assert_eq!(kernel_tp(), KERNEL_TP.load(Ordering::Relaxed));

    // SAFETY: reinstate the same feature-only low user alias to prove normal
    // exception delivery still works after the special refill return path.
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
    // SAFETY: the original controlled syscall instruction is mapped again,
    // with the same exclusively owned context and valid user stack.
    unsafe { run_user(&mut context) };
    assert_user(&context, first, 11, &second_regs);

    // SAFETY: all user executions have returned to the high kernel mapping;
    // restoring DMW1 removes the feature-only user mapping before normal boot.
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
    crate::println!("loongarch64: trap smoke passed");
}
