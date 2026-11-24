//! 扁平设备树（FDT / DTB）物理内存与保留区解析器。
//!
//! 本模块仅在安全的字节切片（`&[u8]`）上解析启动内存布局（RAM、保留表、`/reserved-memory` 与 `initrd`），
//! 不涉及裸指针解引用或外设枚举。原始 FDT 地址的可访问性与架构映射由各架构早期启动代码保证，
//! 已映射存储的边界初筛、切片借用及 FDT 本体物理区间的保留由 [`crate::arch::boot_data`] 负责。

use super::{boot_memory::BootMemory, regions::Region};

mod header;
mod memory;
mod tree;
mod wire;

pub use header::total_size;
use header::Blob;
pub(crate) use header::HEADER_SIZE;

/// 校验并解析完整的设备树字节切片，提取物理 RAM 与启动保留区。
///
/// # Errors
///
/// 当 FDT 头部损坏、数据块截断或重叠、节点结构不合法，或包含不支持的动态保留区/寻址格式时返回错误。
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
#[path = "../ktest/mm/fdt.rs"]
pub mod tests;
