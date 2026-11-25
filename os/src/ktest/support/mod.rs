//! 内核自检套件共用的测试样本构造工具。

pub(crate) mod elf;
pub(crate) mod fdt;
#[cfg(target_arch = "riscv64")]
pub(crate) mod frames;
