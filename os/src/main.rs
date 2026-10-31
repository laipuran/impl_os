#![no_std]
#![no_main]
#![cfg_attr(target_arch = "loongarch64", feature(asm_experimental_arch))]

mod arch;
mod console;
mod lang_items;
#[no_mangle]
extern "C" fn kernel_main() -> ! {
    // SAFETY: only the boot CPU enters Rust; the linker excludes its stack
    // from this BSS interval. No Rust globals are accessed before clearing it.
    unsafe { clear_bss() };
    println!("impl_os: booted on {}", arch::NAME);
    println!("scaffold only: userspace, EXT4 and contest tests are not implemented");
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
        // Volatile stores avoid lowering this loop to an unavailable memset.
        current.write_volatile(0);
        current = current.add(1);
    }
}
