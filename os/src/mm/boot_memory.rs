//! 平台启动发现的、尚未归一化的物理内存描述。

use alloc::vec::Vec;

use super::regions::Region;

/// 平台启动信息中的原始 RAM 和保留区间，尚未排序或合并。
#[derive(Debug)]
pub struct BootMemory {
    /// 所有启用的 RAM 区间。
    pub ram: Vec<Region>,
    /// 启动期间不得分配的原始区间，可能重叠。
    ///
    /// FDT 解析收集固件静态保留区及设备树描述的 initrd；架构层追加
    /// FDT 本体、启动块及其他载荷占用。最终由
    /// [`super::regions::MemoryMap::new`] 归一化。
    pub reserved: Vec<Region>,
}
