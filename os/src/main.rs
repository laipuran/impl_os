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
mod task;
mod trap;
mod utils;

#[no_mangle]
extern "C" fn kernel_main(boot_arg: usize) -> ! {
    // SAFETY: 仅 boot CPU 进入 Rust；链接脚本将其栈排除在此 BSS 区间之外。
    // 清零前不访问任何 Rust 全局变量。
    unsafe { clear_bss() };
    utils::logging::init().expect("logger initialization failed");
    trap::init();
    // SAFETY: 两架构入口汇编保留并整理平台启动参数，建立启动内存访问方式；
    // 仅关闭中断的启动 CPU 运行，启动信息在发现期间不变，剩余 RAM 尚未分配。
    let _memory_map = unsafe { mm::init(boot_arg) }.expect("memory initialization failed");
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
