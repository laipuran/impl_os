use core::arch::{asm, global_asm};

global_asm!(include_str!("entry.S"));

pub const NAME: &str = "riscv64";

pub fn putchar(byte: u8) {
    const UART: *mut u8 = 0x1000_0000 as *mut u8;
    // SAFETY: QEMU virt maps a 16550 UART here; paging is still disabled.
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub fn shutdown() -> ! {
    // SAFETY: SBI v0.2 system reset, shutdown type, no error reason.
    // The firmware may return SBI_ERR_NOT_SUPPORTED; park if it does.
    unsafe {
        asm!(
            "ecall",
            inlateout("a0") 0usize => _,
            inlateout("a1") 0usize => _,
            in("a6") 0usize,
            in("a7") 0x5352_5354usize,
            options(nostack)
        );
    }
    halt()
}

pub fn halt() -> ! {
    loop {
        // SAFETY: boot runs in supervisor mode; interrupts remain disabled.
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}
