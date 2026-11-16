//! 内核主入口与早期初始化流程。

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

/// 架构汇编完成栈设置后跳转至此的共享 Rust 入口。
#[no_mangle]
extern "C" fn kernel_main(boot_arg: usize) -> ! {
    // SAFETY: 入口汇编仅放行主核进入此函数，且链接脚本已将当前正在使用的启动栈
    // 放在 `[sbss, ebss)` 区间之外；在此之前尚未访问任何 Rust 静态变量。
    unsafe { clear_bss() };
    utils::logging::init().expect("logger initialization failed");
    trap::init();
    // SAFETY: 入口汇编已按各架构约定传入 `boot_arg` 并建立早期内存访问环境；
    // 当前运行在单核关中断状态下，启动信息与未分配的 RAM 均保持有效且不被并发修改。
    let _frames = unsafe { mm::init(boot_arg) }.expect("memory initialization failed");
    log::info!("booted on {}", arch::NAME);
    #[cfg(feature = "ktest")]
    ktest::run(&_frames);
    arch::shutdown()
}

/// 清零链接脚本定义的 `[sbss, ebss)` 未初始化数据段。
unsafe fn clear_bss() {
    extern "C" {
        static mut sbss: u8;
        static mut ebss: u8;
    }
    let mut current = core::ptr::addr_of_mut!(sbss);
    let end = core::ptr::addr_of_mut!(ebss);
    while current < end {
        // 使用 volatile 写入，防止编译器将循环优化为当前尚不可用的外部 `memset` 调用。
        current.write_volatile(0);
        current = current.add(1);
    }
}
