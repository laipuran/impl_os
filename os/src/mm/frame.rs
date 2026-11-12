//! 基于位图的 4 KiB 物理页框分配器。
//!
//! 分配器元数据存放在内核静态堆中，初始化与分配过程不直接读写被管理的物理页，
//! 也不负责建立页表映射或将页面清零。分配出的 [`Frame`] 句柄通过生命周期借用分配器，
//! 离开作用域时自动归还页框。当前实现面向单核关中断环境。
//! 正常启动的分配器由 `mm` 全局持有，测试也可创建独立实例。

use alloc::vec::Vec;
use core::cell::RefCell;

use super::regions::{MemoryMap, Region};

mod bitmap;

use bitmap::Bitmap;

/// 物理页框大小（4 KiB）。
pub const PAGE_SIZE: usize = 4096;
const WORD_BITS: usize = usize::BITS as usize;

/// 管理多段不连续可用物理区间的单页分配器。
///
/// 内部使用 1 bit/页的位图记录空闲状态（RAM 空洞不占空间），并配合非空位图字索引栈实现 $O(1)$ 分配与回收。
/// 由于所有 [`Frame`] 句柄均借用自本结构体，编译器可在编译期保证分配器不会先于任何已分配的页框销毁。
/// 当前使用 `RefCell` 维护内部可变性，不支持跨核共享或中断重入。
pub struct FrameAllocator {
    // 在页表模块正式接入前，仅 ktest 直接读取区间并调用分配接口。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    regions: Vec<Region>,
    total: usize,
    state: RefCell<Bitmap>,
}

impl FrameAllocator {
    /// 根据 [`MemoryMap`] 中的可用物理区间构建页框分配器（需在内核堆初始化后调用）。
    ///
    /// 允许传入可用区间为空的 `MemoryMap`，此时 `total()` 和 `available()` 均为 0，`alloc()` 恒返回 `None`。
    /// 构造过程仅初始化位图元数据，不会解引用 `map` 中描述的物理地址。
    ///
    /// # Errors
    ///
    /// 当可用区间未按 [`PAGE_SIZE`] 对齐、总页数计算溢出，或内核堆空间不足以容纳位图与索引数组时返回错误。
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

    /// 返回受管理的总物理页框数（含已分配页框）。
    pub fn total(&self) -> usize {
        self.total
    }

    /// 返回当前空闲的物理页框数。
    pub fn available(&self) -> usize {
        self.state.borrow().available()
    }

    /// 分配一个 4 KiB 物理页框（内容未清零）；若空闲页耗尽则返回 `None`。
    ///
    /// 分配过程为 $O(1)$ 位图操作且不会触发堆分配。注意物理地址 `0` 在特定合成测试区间下也是合法返回值。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    pub fn alloc(&self) -> Option<Frame<'_>> {
        let index = self.state.borrow_mut().alloc()?;
        Some(Frame {
            allocator: self,
            index,
        })
    }
}

/// 代表单个已分配物理页框独占所有权的 RAII 句柄，销毁时自动将页框归还给 [`FrameAllocator`]。
///
/// `Frame` 仅负责物理页框的分配记账，不直接提供内存引用。调用方在将该物理页映射到页表或通过指针访问前，
/// 需确保该物理区间对应真实且独占的 RAM，并在 `Frame` 销毁前解除页表映射、停止一切访问。
#[must_use = "丢弃 Frame 句柄会立即回收该物理页框"]
pub struct Frame<'a> {
    allocator: &'a FrameAllocator,
    index: usize,
}

impl Frame<'_> {
    /// 返回该页框按 [`PAGE_SIZE`] 对齐的起始**物理字节地址**（不可直接作为虚拟指针解引用）。
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
#[path = "../ktest/mm/frame/mod.rs"]
pub(crate) mod tests;
