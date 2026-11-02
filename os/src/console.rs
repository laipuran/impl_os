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

// 仅供启动阶段使用的控制台：尚不支持并发写入和中断上下文。
pub fn print(args: fmt::Arguments<'_>) {
    Console.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        $crate::console::print(format_args!("{}\n", format_args!($($arg)*)))
    };
}
