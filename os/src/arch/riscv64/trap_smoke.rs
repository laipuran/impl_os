//! Feature-only real trap probes; no emulation or production user-memory policy.

use core::arch::{asm, global_asm};

use super::{run_user, TrapContext, DISABLED_STATUS, USER_STATUS};

global_asm!(include_str!("trap_smoke.S"));

extern "C" {
    fn riscv_smoke_kernel() -> usize;
    fn riscv_smoke_kernel_break();
    fn riscv_smoke_nested();
    fn riscv_smoke_nested_break();
    fn riscv_smoke_user_first();
    fn riscv_smoke_user_second();
    fn riscv_smoke_user_privileged();
    fn riscv_smoke_user_float();
}

#[repr(C, align(16))]
struct UserStack([u8; 4096]);

static mut USER_STACK: UserStack = UserStack([0; 4096]);

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
    assert_eq!(context.regs[0], 0);
    assert_eq!(
        context.regs[2],
        context as *const TrapContext as usize + core::mem::size_of::<TrapContext>(),
        "kernel interrupted sp",
    );
    assert_eq!(context.regs[2] & 15, 0);
    for (index, value) in context.regs.iter().enumerate().skip(1) {
        if index != 2 {
            assert_eq!(*value, 0x100 + index, "kernel x{index} save");
        }
    }
}

/// Recognize only the two exact supervisor breakpoint PCs owned by this smoke.
/// All other traps remain subject to the shared fatal kernel-trap policy.
pub fn handle_smoke_kernel(context: &mut TrapContext) -> bool {
    if context.cause != 3 {
        return false;
    }
    let outer = riscv_smoke_kernel_break as *const () as usize;
    let nested = riscv_smoke_nested_break as *const () as usize;
    if context.pc != outer && context.pc != nested {
        return false;
    }
    assert_kernel_environment();
    assert_ne!(context.status & (1 << 8), 0, "kernel SPP");
    assert_eq!(context.status & DISABLED_STATUS, 0);
    assert!(context.bad_addr == 0 || context.bad_addr == context.pc);
    if context.pc == outer {
        assert_kernel_registers(context);
        // SAFETY: this assembly leaf raises one known breakpoint on the current
        // aligned kernel stack; its exact PC is handled by the branch below.
        unsafe { riscv_smoke_nested() };
        assert_kernel_environment();
        assert_kernel_registers(context); // The inner frame must not alias this one.
        for (index, value) in context.regs.iter_mut().enumerate().skip(1) {
            if index != 2 {
                *value = 0x200 + index;
            }
        }
    } else {
        assert_eq!(context.regs[2] & 15, 0, "nested kernel sp alignment");
    }
    context.pc += 4; // Both breakpoint sites force non-compressed instructions.
    true
}

fn set_user_registers(context: &mut TrapContext, base: usize, stack: usize) {
    for (index, value) in context.regs.iter_mut().enumerate().skip(1) {
        *value = base + index;
    }
    context.regs[0] = 0;
    context.regs[2] = stack;
}

fn assert_user_context(context: &TrapContext, base: usize, stack: usize, pc: usize, cause: usize) {
    assert_eq!(context.pc, pc, "user exception PC");
    assert_eq!(context.cause, cause, "user exception cause");
    assert_eq!(context.status & ((1 << 8) | DISABLED_STATUS), 0);
    assert_eq!(context.status & (3 << 32), USER_STATUS, "RV64 UXL");
    assert_eq!(context.regs[0], 0);
    for (index, value) in context.regs.iter().enumerate().skip(1) {
        let expected = if index == 2 { stack } else { base + index };
        assert_eq!(*value, expected, "user x{index} restore/save");
    }
    assert_kernel_environment();
}

/// Exercise nested S-mode ebreak and four real U-mode save/restore round trips.
///
/// The user probes require SBI PMP to permit RAM in bare mode. They neither map
/// memory nor provide isolation, and execute only these fixed trusted snippets.
pub fn smoke_test() {
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
    context.status = usize::MAX; // Entry must discard even hostile privilege bits.
                                 // SAFETY: fixed user code and an aligned private stack are in PMP-accessible
                                 // RAM; probes cannot write kernel memory and each immediately traps.
    unsafe { run_user(&mut context) };
    assert_user_context(&context, 0x300, stack, first, 8);
    assert_eq!(context.bad_addr, 0);

    context.pc = second; // Skip the deliberate illegal instruction after ecall.
    set_user_registers(&mut context, 0x400, stack - 16);
    // SAFETY: same valid private memory and fixed trapping code as above.
    unsafe { run_user(&mut context) };
    assert_user_context(&context, 0x400, stack - 16, second, 8);
    assert_eq!(context.bad_addr, 0);

    context.pc = privileged;
    set_user_registers(&mut context, 0x500, stack - 32);
    // SAFETY: the CSR instruction must fault in U mode and does not access data.
    unsafe { run_user(&mut context) };
    assert_user_context(&context, 0x500, stack - 32, privileged, 2);
    assert!(context.bad_addr == 0 || context.bad_addr == 0x1400_1073);

    context.pc = float;
    set_user_registers(&mut context, 0x600, stack - 48);
    // SAFETY: the fixed FP instruction must fault with FS Off; it touches no RAM.
    unsafe { run_user(&mut context) };
    assert_user_context(&context, 0x600, stack - 48, float, 2);
    assert!(context.bad_addr == 0 || context.bad_addr == 0x53);
    crate::println!("riscv64: trap smoke passed");
}
