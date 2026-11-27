//! LoongArch 64 架构支持（QEMU `virt` 平台，PLV0）。

use core::arch::{asm, global_asm};

global_asm!(include_str!("loongarch64/entry.S"));

pub(crate) mod boot;
pub(crate) mod syscall;
pub(crate) mod trap;

pub(crate) const NAME: &str = "loongarch64";

/// ELF 文件头中 LoongArch 对应的机器号（`EM_LOONGARCH`）。
pub(crate) const ELF_MACHINE: u16 = 258;

/// LoongArch 采用定长 32 位指令，ELF 入口按 4 字节对齐。
pub(crate) const ELF_ENTRY_ALIGN: usize = 4;

pub(crate) fn putchar(byte: u8) {
    // 通过 `entry.S` 配置的 DMW0 窗口访问 QEMU `virt` 的 NS16550A 串口（物理地址 `0x1fe0_01e0`）。
    const UART: *mut u8 = 0x9000_0000_1fe0_01e0 as *mut u8;
    // SAFETY: `entry.S` 已配置 `0x9000_0000_0000_0000` 起的 PLV0 直接映射窗口，可直接读写该 MMIO 寄存器。
    unsafe {
        while UART.add(5).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        UART.write_volatile(byte);
    }
}

pub(crate) fn shutdown() -> ! {
    // TODO: 后续核对评测平台 LoongArch `virt` 机器的关机控制器地址并补齐关机实现。
    crate::println!("loongarch64: poweroff not implemented; parking CPU");
    halt()
}

pub(crate) fn halt() -> ! {
    loop {
        // SAFETY: 当前运行在 PLV0 且全局中断已关闭，执行 `idle 0` 让当前核休眠等待。
        unsafe { asm!("idle 0", options(nomem, nostack)) };
    }
}
