//! 从字节切片解析启动内存所需的 FDT 信息，不读取物理地址。
//!
//! 不进行设备枚举，也不分配动态保留区；原始地址读取和启动存储保留由架构层负责。

use super::{boot_memory::BootMemory, regions::Region};

mod header;
mod memory;
mod tree;
mod wire;

pub use header::total_size;
use header::Blob;
pub(crate) use header::HEADER_SIZE;

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
#[path = "../../ktest/mm/fdt/mod.rs"]
pub mod tests;
