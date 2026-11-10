//! QEMU virt direct ELF boot：从 a2 指向的 system table 查找 FDT 和 initrd。

use core::slice;

use crate::arch::boot_data::MappedFdt;
use crate::mm::{fdt, regions::Region};

mod tables;

/// 入口汇编建立的 PLV0 缓存直接映射窗口。
const DIRECT_MAP_BASE: usize = 0x9000_0000_0000_0000;
// 当前 QEMU virt direct boot 将命令行和合成 system/configuration tables
// 放在前 1 MiB，FDT 放在其后；这不是通用 UEFI 启动协议。
const BOOT_INFO_SIZE: usize = 0x10_0000;
const FDT_SIZE_LIMIT: usize = 0x10_0000;
const PHYSICAL_LIMIT: usize = 1 << 48;

/// 经 DMW 读取 QEMU direct boot 的启动信息，并保留它仍占用的 RAM。
///
/// # Errors
///
/// 启动表地址、签名、GUID 表、FDT 或 initrd 区间无效时返回错误。
/// 不支持经 UEFI 固件加载的启动方式。
///
/// # Safety
///
/// `boot_arg` 必须来自 QEMU virt direct boot 的 a2；入口汇编必须已建立
/// DMW0，低端启动数据和配置表指向的完整 FDT 必须可读且不变。
pub unsafe fn memory(boot_arg: usize) -> Result<fdt::BootMemory, &'static str> {
    // SAFETY: QEMU direct boot 将完整的前 1 MiB 启动块放入低端 RAM；
    // DMW0 已由入口汇编建立，boot CPU 独占读取。
    let boot_info = unsafe { slice::from_raw_parts(DIRECT_MAP_BASE as *const u8, BOOT_INFO_SIZE) };
    let tables = tables::parse(boot_info, boot_arg)?;
    let address = mapped_address(tables.fdt, FDT_SIZE_LIMIT)?;
    // SAFETY: 调用者保证配置表中的 FDT 可读，地址已检查在 DMW 可表示范围；
    // from_raw 会验证总长度不超过 QEMU 保留的 1 MiB FDT 容量。
    let blob = unsafe { MappedFdt::from_raw(tables.fdt, address, FDT_SIZE_LIMIT) }?;
    let mut memory = blob.parse_memory()?;
    memory.reserved.push(Region::new(0, BOOT_INFO_SIZE)?);
    if let Some(initrd) = tables.initrd {
        mapped_address(initrd.start, initrd.end - initrd.start)?;
        memory.reserved.push(initrd);
    }
    log::info!(
        "boot tables: physical={:#x}, FDT={:#x}",
        boot_arg,
        tables.fdt
    );
    Ok(memory)
}

/// 将高地址链接符号转换成完整内核的物理占用区间。
///
/// # Errors
///
/// 链接边界为空、顺序无效或不属于当前 DMW0 可表示的物理范围时返回错误。
pub fn kernel_region() -> Result<Region, &'static str> {
    extern "C" {
        static skernel: u8;
        static ekernel: u8;
    }
    let start = (core::ptr::addr_of!(skernel) as usize)
        .checked_sub(DIRECT_MAP_BASE)
        .ok_or("kernel outside DMW0")?;
    let end = (core::ptr::addr_of!(ekernel) as usize)
        .checked_sub(DIRECT_MAP_BASE)
        .ok_or("kernel outside DMW0")?;
    let size = end.checked_sub(start).ok_or("invalid kernel bounds")?;
    mapped_address(start, size)?;
    Region::new(start, size)
}

fn mapped_address(physical: usize, size: usize) -> Result<usize, &'static str> {
    if physical
        .checked_add(size)
        .is_none_or(|end| end > PHYSICAL_LIMIT)
    {
        return Err("physical range outside DMW0");
    }
    Ok(DIRECT_MAP_BASE | physical)
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
