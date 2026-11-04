use core::alloc::Layout;
use core::panic::PanicInfo;

use crate::{arch, println};

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    // 格式化和 panic 输出不分配堆内存，避免耗尽后的递归分配。
    panic!("kernel heap allocation failed: {:?}", layout);
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    println!("kernel panic: {}", info);
    // Do not report a successful shutdown for a panic.
    arch::halt()
}
