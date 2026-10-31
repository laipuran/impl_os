use core::panic::PanicInfo;

use crate::{arch, println};

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    println!("kernel panic: {}", info);
    // Do not report a successful shutdown for a panic.
    arch::halt()
}
