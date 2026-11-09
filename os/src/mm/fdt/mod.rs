//! 只解析启动内存所需的 FDT 信息；不进行设备枚举，也不分配动态保留区。

use alloc::vec::Vec;

use super::regions::Region;

mod header;
mod memory;
mod tree;
mod wire;

pub use header::total_size;
use header::{Blob, HEADER_SIZE};

/// 平台启动信息中的原始 RAM 和保留区间，尚未排序或合并。
#[derive(Debug)]
pub struct BootMemory {
    /// 所有启用的 RAM 区间。
    pub ram: Vec<Region>,
    /// 启动期间不得分配的原始区间，可能重叠。
    ///
    /// [`parse`] 收集固件静态保留区及设备树描述的 initrd；[`read`] 追加
    /// FDT 本体，平台层继续追加启动块及其他载荷占用。最终由
    /// [`super::regions::MemoryMap::new`] 归一化。
    pub reserved: Vec<Region>,
}

/// 通过架构提供的映射读取完整 FDT，并保留其物理存储区间。
///
/// # Errors
///
/// 地址无效、头部无效、长度超过平台容量或解析失败时返回错误。
///
/// # Safety
///
/// `mapped` 必须是 `physical` 对应的可读内核地址，头部和头部描述的
/// 完整 FDT 必须可读且不变。`capacity` 是平台允许的最大 FDT 长度。
pub unsafe fn read(
    physical: usize,
    mapped: usize,
    capacity: usize,
) -> Result<BootMemory, &'static str> {
    if physical % 8 != 0 || mapped == 0 || mapped.checked_add(HEADER_SIZE).is_none() {
        return Err("invalid FDT address");
    }
    // SAFETY: 调用者保证映射后的头部可读且不变；已检查空地址和地址溢出。
    let header = unsafe { core::slice::from_raw_parts(mapped as *const u8, HEADER_SIZE) };
    let size = total_size(header)?;
    if size > capacity || mapped.checked_add(size).is_none() {
        return Err("FDT exceeds platform storage");
    }
    let region = Region::new(physical, size)?;
    // SAFETY: 调用者保证完整 FDT 可读；total_size 已限制长度，地址加长度
    // 无溢出，且读取期间没有写入者。
    let blob = unsafe { core::slice::from_raw_parts(mapped as *const u8, size) };
    let mut memory = parse(blob)?;
    memory.reserved.push(region);
    Ok(memory)
}

/// 校验并解析完整 FDT，拒绝截断、重叠的数据块及不支持的内存描述。
pub fn parse(blob: &[u8]) -> Result<BootMemory, &'static str> {
    tree::parse(Blob::parse(blob)?)
}

fn physical_region(start: u64, size: u64) -> Result<Region, &'static str> {
    Region::new(
        usize::try_from(start).map_err(|_| "physical address exceeds usize")?,
        usize::try_from(size).map_err(|_| "physical size exceeds usize")?,
    )
}

#[cfg(any(test, feature = "ktest"))]
pub mod tests;
