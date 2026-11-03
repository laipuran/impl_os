use core::arch::{asm, global_asm};

pub mod trap;

global_asm!(include_str!("entry.S"));

pub const NAME: &str = "loongarch64";

pub fn putchar(byte: u8) {
    // 通过 DMW0 访问 QEMU virt UART；仍需在比赛官方 QEMU 中验证。
    const UART: *mut u8 = 0x9000_0000_1fe0_01e0 as *mut u8;
    // 安全性：entry.S 已建立此处使用的 PLV0 带缓存直接映射。
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub fn shutdown() -> ! {
    // TODO：验证官方机器的关机机制；不要猜测
    // MMIO 地址，也不要声称此路径能成功退出 QEMU。
    crate::println!("loongarch64: poweroff not implemented; parking CPU");
    halt()
}

pub fn halt() -> ! {
    loop {
        // 安全性：启动代码在 PLV0 运行，且中断已关闭。
        unsafe { asm!("idle 0", options(nomem, nostack)) };
    }
}
