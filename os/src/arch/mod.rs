pub(crate) mod boot_data;

#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "riscv64")]
#[cfg_attr(not(feature = "ktest"), allow(unused_imports))] // 用户地址空间接入前由 ktest 使用。
pub(crate) use riscv64::paging;
#[cfg(target_arch = "riscv64")]
pub(crate) use riscv64::{boot, halt, putchar, shutdown, syscall, trap, NAME};

#[cfg(target_arch = "loongarch64")]
mod loongarch64;
#[cfg(target_arch = "loongarch64")]
pub(crate) use loongarch64::{boot, halt, putchar, shutdown, syscall, trap, NAME};
