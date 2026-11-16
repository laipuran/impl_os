//! RISC-V 64 架构支持（QEMU `virt` 平台，S-mode）。

use core::arch::{asm, global_asm};

global_asm!(include_str!("entry.S"));

pub(crate) mod boot;
pub(crate) mod syscall;
pub(crate) mod trap;

pub(crate) const NAME: &str = "riscv64";

pub(crate) fn putchar(byte: u8) {
    const UART: *mut u8 = 0x1000_0000 as *mut u8;
    // SAFETY: QEMU `virt` 在物理地址 `0x1000_0000` 处映射了 NS16550A 兼容 UART，
    // 当前内核运行在物理直通模式（`satp = Bare`），可直接通过 volatile 读写该 MMIO 寄存器。
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub(crate) fn shutdown() -> ! {
    // SAFETY: 调用 SBI v0.2 System Reset 扩展（EID = 0x53525354，FID = 0）请求关机；
    // 若固件不支持该扩展而返回，则回退到下方的 `halt()` 挂起 CPU。
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

pub(crate) fn halt() -> ! {
    loop {
        // SAFETY: 当前运行在 S-mode 且全局中断已关闭，执行 `wfi` 让当前核休眠等待。
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}
