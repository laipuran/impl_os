//! RISC-V 64 启动期设备树解析与内核物理区间计算。

use crate::arch::boot_data::MappedFdt;
use crate::mm::{boot_memory::BootMemory, regions::Region};

/// 在物理直通模式（`satp = Bare`）下读取固件传入的设备树并提取内存布局。
///
/// # Errors
///
/// 当 FDT 地址、头部、结构块或内存节点描述不合法时返回错误。
///
/// # Safety
///
/// `boot_arg` 必须来自固件传入 `a1`（由 `entry.S` 移入 `a0`）的 FDT 物理地址，
/// 且该地址指向的完整 FDT 在解析期间必须有效、可读且不被修改。
pub unsafe fn memory(boot_arg: usize) -> Result<BootMemory, &'static str> {
    // SAFETY: 调用方保证固件提供的 FDT 有效可读；当前处于 `satp = Bare` 模式，物理地址可直接作为虚拟地址访问。
    let blob = unsafe { MappedFdt::from_raw(boot_arg, boot_arg, isize::MAX as usize) }?;
    let memory = blob.parse_memory()?;
    log::info!("FDT: physical={:#x}", boot_arg);
    Ok(memory)
}

/// 返回链接脚本中 `skernel..ekernel` 覆盖的完整内核镜像物理区间（含启动栈与静态堆）。
///
/// # Errors
///
/// 当链接符号地址为空或 `ekernel <= skernel` 时返回错误。
pub fn kernel_region() -> Result<Region, &'static str> {
    extern "C" {
        static skernel: u8;
        static ekernel: u8;
    }
    let start = core::ptr::addr_of!(skernel) as usize;
    let end = core::ptr::addr_of!(ekernel) as usize;
    Region::new(
        start,
        end.checked_sub(start).ok_or("invalid kernel bounds")?,
    )
}
