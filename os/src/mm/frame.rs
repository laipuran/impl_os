//! 以位图管理 4 KiB 物理页框；句柄独占分配器内的页框记录，销毁时自动回收。
//!
//! 元数据来自启动堆，不访问被管理的物理内存，不建立映射或清零页面。
//! 分配器由调用者持有，不使用全局可变状态；目前仅供关闭中断的 boot CPU 使用。

use alloc::vec::Vec;
use core::cell::RefCell;

use super::regions::{MemoryMap, Region};

mod bitmap;

use bitmap::Bitmap;

/// 页框大小，单位为字节。
pub const PAGE_SIZE: usize = 4096;
const WORD_BITS: usize = usize::BITS as usize;

/// 管理不连续可用区间的单页分配器。
///
/// 每页使用一位位图，并为每个位图字预留一个非空字索引槽，不为 RAM 空洞分配元数据。
/// 所有句柄都借用分配器，因而在句柄销毁前无法销毁分配器。
/// 此类型不支持跨 CPU 共享或中断重入。
/// 仅保证同一分配器内不重复分配，不验证 RAM 有效性或跨分配器的地址重叠。
pub struct FrameAllocator {
    // 正式页框使用者接入前，仅 ktest 读取区间并调用分配接口；接入后移除对应豁免。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    regions: Vec<Region>,
    total: usize,
    state: RefCell<Bitmap>,
}

impl FrameAllocator {
    /// 为内存地图中的可用完整页建立分配记录；必须在内核堆初始化后调用。
    ///
    /// 可用区间列表可以为空，此时总页数和空闲页数均为零，分配返回 `None`。
    /// 不访问所描述的物理内存，因此也可用于合成区间的记账。
    ///
    /// # Errors
    ///
    /// 可用区间未按 4 KiB 对齐，页数溢出，或启动堆无法容纳
    /// 位图及非空字索引时返回错误。
    pub fn new(map: MemoryMap) -> Result<Self, &'static str> {
        let mut total = 0usize;
        for region in map.usable() {
            if region.start % PAGE_SIZE != 0 || region.end % PAGE_SIZE != 0 {
                return Err("invalid usable frame region");
            }
            total = total
                .checked_add((region.end - region.start) / PAGE_SIZE)
                .ok_or("frame count overflow")?;
        }
        let bitmap = Bitmap::new(total)?;
        Ok(Self {
            regions: map.into_usable(),
            total,
            state: RefCell::new(bitmap),
        })
    }

    /// 返回管理的总页数，包括当前已分配的页。
    pub fn total(&self) -> usize {
        self.total
    }

    /// 返回当前空闲页数。
    pub fn available(&self) -> usize {
        self.state.borrow().available()
    }

    /// 分配一个未清零的页框；耗尽时返回 `None`，不申请额外堆内存。
    ///
    /// 物理地址零也是有效分配结果。返回的句柄销毁时自动回收页框。
    /// 不承诺地址顺序；位图字的选择和更新为常数时间，不扫描整个位图。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    pub fn alloc(&self) -> Option<Frame<'_>> {
        let index = self.state.borrow_mut().alloc()?;
        Some(Frame {
            allocator: self,
            index,
        })
    }
}

/// 独占所属分配器内的一个页框记录；不可复制，销毁时自动归还。
///
/// 本类型不提供安全的内存引用。调用者建立映射或通过裸指针访问页面时，
/// 必须另行确认地址对应有效且独占的 RAM，不与内核、启动数据或其他使用者冲突，
/// 并保证所有访问和映射在句柄销毁前结束；物理地址本身不延长有效期。
#[must_use = "销毁句柄会立即回收页框"]
pub struct Frame<'a> {
    allocator: &'a FrameAllocator,
    index: usize,
}

impl Frame<'_> {
    /// 返回按 [`PAGE_SIZE`] 对齐的物理字节地址，不是可直接解引用的虚拟地址。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    pub fn start_address(&self) -> usize {
        let mut index = self.index;
        for region in &self.allocator.regions {
            let count = (region.end - region.start) / PAGE_SIZE;
            if index < count {
                return region.start + index * PAGE_SIZE;
            }
            index -= count;
        }
        unreachable!("frame index outside managed regions");
    }
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        self.allocator.state.borrow_mut().dealloc(self.index);
    }
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
