//! LoongArch 64 启动期内存发现：从 QEMU direct boot 传入的 EFI 系统表定位设备树（FDT）与 initrd。

use core::slice;

use crate::arch::boot_data::MappedFdt;
use crate::mm::{boot_memory::BootMemory, regions::Region};

mod tables;

/// `entry.S` 配置的 PLV0 可缓存直接映射窗口（`DMW0`）虚拟基址。
const DIRECT_MAP_BASE: usize = 0x9000_0000_0000_0000;
// QEMU `virt` direct boot 将命令行与合成的 EFI 系统表/配置表放置在低端 `[0, 1 MiB)`，
// 并在其后为 FDT 预留 1 MiB 容量（参考 QEMU `hw/loongarch/boot.c`）。
const BOOT_INFO_SIZE: usize = 0x10_0000;
const FDT_SIZE_LIMIT: usize = 0x10_0000;
const PHYSICAL_LIMIT: usize = 1 << 48;

/// 通过 `DMW0` 读取 QEMU direct boot 的启动配置表与 FDT，并记录启动数据占用的保留物理区间。
///
/// # Errors
///
/// 当 EFI 系统表地址、签名、GUID 配置表、FDT 或 `initrd` 区间不合法时返回错误。
///
/// # Safety
///
/// `boot_arg` 必须来自 QEMU `virt` direct boot 传入 `$a2`（由 `entry.S` 移入 `$a0`）的系统表物理地址；
/// 调用前 `entry.S` 必须已建立 `DMW0` 窗口，且低端 1 MiB 启动块与配置表指向的完整 FDT 在解析期间有效、可读且不被修改。
pub unsafe fn memory(boot_arg: usize) -> Result<BootMemory, &'static str> {
    // SAFETY: QEMU direct boot 将启动表放置在低端 `[0, 1 MiB)` RAM 中；
    // `entry.S` 已配置好覆盖该物理区间的 `DMW0` 窗口，且当前由启动主核独占读取。
    let boot_info = unsafe { slice::from_raw_parts(DIRECT_MAP_BASE as *const u8, BOOT_INFO_SIZE) };
    let tables = tables::parse(boot_info, boot_arg)?;
    let address = mapped_address(tables.fdt, FDT_SIZE_LIMIT)?;
    // SAFETY: 调用方保证配置表指向的 FDT 有效可读，`mapped_address` 已验证地址落在 `DMW0` 映射范围内，
    // 且 `from_raw` 会校验 FDT 总长度不超过 `FDT_SIZE_LIMIT`（1 MiB）。
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

/// 将 `DMW0` 高地址链接符号 `skernel..ekernel` 转换为内核镜像占用的物理地址区间。
///
/// # Errors
///
/// 当链接符号未落在 `DMW0` 窗口内或 `ekernel <= skernel` 时返回错误。
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
#[path = "../../ktest/boot/loongarch64.rs"]
pub(crate) mod tests;
