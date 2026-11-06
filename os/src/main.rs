#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![cfg_attr(target_arch = "loongarch64", feature(asm_experimental_arch))]

extern crate alloc;

mod arch;
mod console;
#[cfg(feature = "ktest")]
mod ktest;
mod lang_items;
mod mm;
mod syscall;
mod trap;
mod utils;

#[no_mangle]
fn kernel_main() -> ! {
    // SAFETY: 仅 boot CPU 进入 Rust；链接脚本将其栈排除在此 BSS 区间之外。
    // 清零前不访问任何 Rust 全局变量。
    unsafe { clear_bss() };
    utils::logging::init().expect("logger initialization failed");
    trap::init();
    mm::init();
    log::info!("booted on {}", arch::NAME);
    #[cfg(feature = "ktest")]
    ktest::run();
    arch::shutdown()
}

unsafe fn clear_bss() {
    extern "C" {
        static mut sbss: u8;
        static mut ebss: u8;
    }
    let mut current = core::ptr::addr_of_mut!(sbss);
    let end = core::ptr::addr_of_mut!(ebss);
    while current < end {
        // Volatile 写入避免将此循环转换为当前不可用的 memset。
        current.write_volatile(0);
        current = current.add(1);
    }
}
