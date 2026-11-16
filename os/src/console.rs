//! 早期串口控制台输出与格式化宏。

use core::fmt::{self, Write};

struct Console;

impl Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                crate::arch::putchar(b'\r');
            }
            crate::arch::putchar(byte);
        }
        Ok(())
    }
}

/// 向架构早期串口输出格式化文本。
///
/// 当前仅供单核关中断环境使用，尚未加锁保护并发或中断重入。
pub fn print(args: fmt::Arguments<'_>) {
    Console.write_fmt(args).unwrap();
}

/// 向早期串口打印一行格式化文本。
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        $crate::console::print(format_args!("{}\n", format_args!($($arg)*)))
    };
}
