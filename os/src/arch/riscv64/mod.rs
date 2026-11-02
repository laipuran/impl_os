use core::arch::{asm, global_asm};

pub mod trap;

global_asm!(include_str!("entry.S"));

pub const NAME: &str = "riscv64";

pub fn putchar(byte: u8) {
    const UART: *mut u8 = 0x1000_0000 as *mut u8;
    // 安全性：QEMU virt 在此处映射了 16550 UART；分页仍处于关闭状态。
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub fn shutdown() -> ! {
    // 安全性：调用 SBI v0.2 系统重置，类型为关机，原因为无错误。
    // 固件可能返回 SBI_ERR_NOT_SUPPORTED；若如此则停放 CPU。
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
        // 安全性：启动代码在监管态运行；中断保持关闭。
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}
