use core::arch::{asm, global_asm};

global_asm!(include_str!("entry.S"));

pub const NAME: &str = "loongarch64";

pub fn putchar(byte: u8) {
    // QEMU virt UART through DMW0; official contest QEMU still needs validation.
    const UART: *mut u8 = 0x9000_0000_1fe0_01e0 as *mut u8;
    // SAFETY: entry.S installs the PLV0 cached direct mapping used here.
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub fn shutdown() -> ! {
    // TODO: validate the official machine's poweroff mechanism; do not guess
    // an MMIO address or claim that this path exits QEMU successfully.
    crate::println!("loongarch64: poweroff not implemented; parking CPU");
    halt()
}

pub fn halt() -> ! {
    loop {
        // SAFETY: boot runs at PLV0 with interrupts disabled.
        unsafe { asm!("idle 0", options(nomem, nostack)) };
    }
}
