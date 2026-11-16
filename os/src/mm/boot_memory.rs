//! 平台启动阶段提取的原始物理内存描述。

use alloc::vec::Vec;

use super::regions::Region;

/// 从设备树与平台启动表中收集到的原始 RAM 与保留区间（尚未排序或合并）。
#[derive(Debug)]
pub struct BootMemory {
    /// 已启用的物理 RAM 区间列表。
    pub ram: Vec<Region>,
    /// 启动阶段不可分配的保留物理区间列表（允许乱序或重叠）。
    ///
    /// 由设备树解析器填入固件保留区及 `initrd`，架构启动层追加 FDT 本体与平台启动配置表，
    /// 再由共享内存初始化流程（`mm::discover`）追加内核镜像区间，最后交由 [`super::regions::MemoryMap::new`] 统一归并扣除。
    pub reserved: Vec<Region>,
}
