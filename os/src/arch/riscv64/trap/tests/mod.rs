//! Feature-only real trap probes; no emulation or production user-memory policy.

use core::arch::{asm, global_asm};

use super::cause::{ExceptionCause, InterruptCause, TrapCause};
use super::{run_user, TrapContext, DISABLED_STATUS, USER_STATUS};

global_asm!(include_str!("probes.S"), run_user_shim = sym run_user_shim);

extern "C" {
    fn riscv_smoke_kernel() -> usize;
    fn riscv_smoke_kernel_break();
    fn riscv_smoke_nested();
    fn riscv_smoke_nested_break();
    fn riscv_smoke_call_user(context: &mut TrapContext) -> usize;
    fn riscv_smoke_user_first();
    fn riscv_smoke_user_second();
    fn riscv_smoke_user_privileged();
    fn riscv_smoke_user_float();
}

#[repr(C, align(16))]
struct UserStack([u8; 4096]);

static mut USER_STACK: UserStack = UserStack([0; 4096]);

unsafe extern "C" fn run_user_shim(context: &mut TrapContext) {
    // SAFETY: the assembly wrapper is called only with the smoke's exclusively
    // owned context, fixed user code, and valid private stack after trap init.
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
    // SAFETY: these probes execute only in the single boot hart's S-mode test.
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
    // sret sets SPIE=1 architecturally, but live SIE must remain zero.
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

/// Recognize only the two exact supervisor breakpoint PCs owned by this smoke.
/// All other traps remain subject to the shared fatal kernel-trap policy.
pub fn handle_smoke_kernel(context: &mut TrapContext) -> bool {
    if context.cause() != TrapCause::Exception(ExceptionCause::Breakpoint) {
        return false;
    }
    let outer = riscv_smoke_kernel_break as *const () as usize;
    let nested = riscv_smoke_nested_break as *const () as usize;
    if context.pc() != outer && context.pc() != nested {
        return false;
    }
    assert_kernel_environment();
    assert_ne!(context.raw_status() & (1 << 8), 0, "kernel SPP");
    assert_eq!(context.raw_status() & DISABLED_STATUS, 0);
    assert!(context.bad_addr() == 0 || context.bad_addr() == context.pc());
    if context.pc() == outer {
        assert_kernel_registers(context);
        // SAFETY: this assembly leaf raises one known breakpoint on the current
        // aligned kernel stack; its exact PC is handled by the branch below.
        unsafe { riscv_smoke_nested() };
        assert_kernel_environment();
        assert_kernel_registers(context); // The inner frame must not alias this one.
        for (index, value) in context.registers_mut().iter_mut().enumerate().skip(1) {
            if index != 2 {
                *value = 0x200 + index;
            }
        }
    } else {
        assert_eq!(context.sp() & 15, 0, "nested kernel sp alignment");
    }
    // Test kernel-return sanitation as well as initial hardware capture: clearing
    // SPP must not return to U mode, and these interrupt/extension bits must drop.
    context.sstatus = (context.sstatus & !(1 << 8)) | DISABLED_STATUS;
    context.set_pc(context.pc() + 4); // Both sites force non-compressed instructions.
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

/// Exercise decoding, nested S-mode ebreak, and four real U-mode ABI round trips.
///
/// The user probes require SBI PMP to permit RAM in bare mode. They neither map
/// memory nor provide isolation, and execute only these fixed trusted snippets.
pub fn smoke_test() {
    assert_cause_decoding();
    assert_kernel_environment();
    // SAFETY: the assembly saves the kernel ABI, loads sentinel registers, and
    // raises the exact smoke breakpoint recognized by handle_smoke_kernel.
    assert_eq!(unsafe { riscv_smoke_kernel() }, 1, "kernel GPR restore");
    assert_kernel_environment();

    let stack = core::ptr::addr_of_mut!(USER_STACK) as usize + core::mem::size_of::<UserStack>();
    let first = riscv_smoke_user_first as *const () as usize;
    let second = riscv_smoke_user_second as *const () as usize;
    let privileged = riscv_smoke_user_privileged as *const () as usize;
    let float = riscv_smoke_user_float as *const () as usize;
    let mut context = TrapContext::user(first, stack);
    set_user_registers(&mut context, 0x300, stack);
    context.sstatus = usize::MAX; // Entry must discard even hostile privilege bits.
                                  // SAFETY: fixed user code and an aligned private stack are in PMP-accessible
                                  // RAM; the assembly wrapper preserves valid kernel gp/tp and checks the Rust
                                  // run_user call's ABI before restoring its own caller's state.
    assert_eq!(
        unsafe { riscv_smoke_call_user(&mut context) },
        1,
        "initial user-call ABI",
    );
    assert_user_context(&context, 0x300, stack, first, 8);
    assert_eq!(context.bad_addr(), 0);

    context.set_pc(second); // Skip the deliberate illegal instruction after ecall.
    set_user_registers(&mut context, 0x400, stack - 16);
    // SAFETY: same valid private memory and fixed trapping code as above; this
    // reentry must honor the edited user context and still preserve the call ABI.
    assert_eq!(
        unsafe { riscv_smoke_call_user(&mut context) },
        1,
        "resumed user-call ABI",
    );
    assert_user_context(&context, 0x400, stack - 16, second, 8);
    assert_eq!(context.bad_addr(), 0);

    context.set_pc(privileged);
    set_user_registers(&mut context, 0x500, stack - 32);
    // SAFETY: the CSR instruction must fault in U mode and does not access data.
    assert_eq!(
        unsafe { riscv_smoke_call_user(&mut context) },
        1,
        "privileged user-call ABI",
    );
    assert_user_context(&context, 0x500, stack - 32, privileged, 2);
    assert!(context.bad_addr() == 0 || context.bad_addr() == 0x1400_1073);

    context.set_pc(float);
    set_user_registers(&mut context, 0x600, stack - 48);
    // SAFETY: the fixed FP instruction must fault with FS Off; it touches no RAM.
    assert_eq!(
        unsafe { riscv_smoke_call_user(&mut context) },
        1,
        "FP user-call ABI",
    );
    assert_user_context(&context, 0x600, stack - 48, float, 2);
    assert!(context.bad_addr() == 0 || context.bad_addr() == 0x53);
    crate::println!("riscv64: trap smoke passed");
}
