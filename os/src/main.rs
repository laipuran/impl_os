#![no_std]
#![no_main]
#![cfg_attr(target_arch = "loongarch64", feature(asm_experimental_arch))]

mod arch;
mod console;
mod lang_items;
mod utils;

#[no_mangle]
fn kernel_main() -> ! {
    // 安全性：只有启动 CPU 进入 Rust；链接器将其栈排除在
    // 此 BSS 区间之外。清零之前不访问任何 Rust 全局变量。
    unsafe { clear_bss() };

    utils::logging::init().unwrap();
    #[cfg(target_arch = "riscv64")]
    arch::trap::init();

    log::info!("booted on {}", arch::NAME);

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
        current.write(0);
        current = current.add(1);
    }
}
