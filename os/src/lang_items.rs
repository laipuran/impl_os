//! 全局堆分配失败与内核 panic 处理函数。

use core::alloc::Layout;
use core::panic::PanicInfo;

use crate::{arch, println};

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    // 串口格式化输出不依赖堆分配，不会因堆耗尽而引发递归分配。
    panic!("kernel heap allocation failed: {:?}", layout);
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    println!("kernel panic: {}", info);
    // 内核异常终止时直接挂起 CPU，不能调用 `arch::shutdown()` 伪造正常关机。
    arch::halt()
}
