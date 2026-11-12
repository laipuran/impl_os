//! RISC-V 64 架构支持（QEMU `virt` 平台，S-mode）。

use core::arch::{asm, global_asm};

global_asm!(include_str!("entry.S"));

pub(crate) mod boot;
pub(crate) mod paging;
pub(crate) mod syscall;
pub(crate) mod trap;

pub(crate) const NAME: &str = "riscv64";

pub(crate) fn putchar(byte: u8) {
    const UART: *mut u8 = 0x1000_0000 as *mut u8;
    // SAFETY: QEMU `virt` 在物理地址 `0x1000_0000` 处映射了 NS16550A 兼容 UART，
    // 正常启动以裸地址访问该设备；启用 Sv39 时，活动页表保留该地址的监管态 MMIO 映射。
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub(crate) fn shutdown() -> ! {
    // SAFETY: 按 SBI v0.2+ 寄存器调用约定，调用 SBI v0.3 引入的 System Reset（SRST）扩展（EID = 0x53525354，FID = 0）请求关机；
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
