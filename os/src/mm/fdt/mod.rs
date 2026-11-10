//! 从字节切片解析启动内存所需的 FDT 信息，不读取物理地址。
//!
//! 不进行设备枚举，也不分配动态保留区；原始地址读取和启动存储保留由架构层负责。

use alloc::vec::Vec;

use super::regions::Region;

mod header;
mod memory;
mod tree;
mod wire;

pub use header::total_size;
use header::Blob;
pub(crate) use header::HEADER_SIZE;

/// 平台启动信息中的原始 RAM 和保留区间，尚未排序或合并。
#[derive(Debug)]
pub struct BootMemory {
    /// 所有启用的 RAM 区间。
    pub ram: Vec<Region>,
    /// 启动期间不得分配的原始区间，可能重叠。
    ///
    /// [`parse`] 收集固件静态保留区及设备树描述的 initrd；架构层追加
    /// FDT 本体、启动块及其他载荷占用。最终由
    /// [`super::regions::MemoryMap::new`] 归一化。
    pub reserved: Vec<Region>,
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
