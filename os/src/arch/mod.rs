pub(crate) mod boot_data;

#[cfg(target_arch = "riscv64")]
mod riscv64;
// 底层分页自检直接检查 Sv39 页表和 CSR。
#[cfg(all(target_arch = "riscv64", feature = "ktest"))]
pub(crate) use riscv64::paging;
#[cfg(target_arch = "riscv64")]
pub(crate) use riscv64::phys_access;
#[cfg(target_arch = "riscv64")]
pub(crate) mod user_paging;
#[cfg(target_arch = "riscv64")]
pub(crate) use riscv64::{
    boot, halt, putchar, shutdown, syscall, trap, ELF_ENTRY_ALIGN, ELF_MACHINE, NAME,
};

#[cfg(target_arch = "loongarch64")]
mod loongarch64;
#[cfg(target_arch = "loongarch64")]
pub(crate) use loongarch64::{
    boot, halt, putchar, shutdown, syscall, trap, ELF_ENTRY_ALIGN, ELF_MACHINE, NAME,
};
