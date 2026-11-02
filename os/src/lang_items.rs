use core::panic::PanicInfo;

use crate::{arch, println};

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    println!("kernel panic: {}", info);
    // 不要将 panic 报告为成功关机。
    arch::halt()
}
