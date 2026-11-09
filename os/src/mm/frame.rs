//! 以位图管理 4 KiB 物理页框；句柄独占页框，销毁时自动回收。
//!
//! 元数据来自启动堆，不访问被管理的物理内存，不建立映射或清零页面。
//! 分配器由调用者持有，不使用全局可变状态；目前仅供关闭中断的 boot CPU 使用。

use alloc::vec::Vec;
use core::cell::RefCell;

use super::regions::{MemoryMap, Region};

/// 页框大小，单位为字节。
pub const PAGE_SIZE: usize = 4096;
const WORD_BITS: usize = usize::BITS as usize;

struct State {
    // 置位表示空闲；最后一个字中不对应物理页的位始终为零。
    free: Vec<usize>,
    available: usize,
    hint: usize,
}

/// 管理不连续可用区间的单页分配器。
///
/// 每页使用一位元数据，不为 RAM 空洞分配位图。所有句柄都借用分配器，
/// 因而在句柄销毁前无法销毁分配器。此类型不支持跨 CPU 共享或中断重入。
pub struct FrameAllocator {
    // 正式页框使用者接入前，仅 ktest 读取区间并调用分配接口；接入后移除对应豁免。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    regions: Vec<Region>,
    total: usize,
    state: RefCell<State>,
}

impl FrameAllocator {
    /// 接管内存地图中的可用完整页；必须在内核堆初始化后调用。
    ///
    /// 可用区间列表可以为空，此时总页数和空闲页数均为零，分配返回 `None`。
    ///
    /// # Errors
    ///
    /// 可用区间未按 4 KiB 对齐，页数溢出，或启动堆无法容纳
    /// 位图时返回错误。失败时不接管任何页框。
    ///
    /// # Safety
    ///
    /// 调用者必须独占所有可用区间，保证它们是有效 RAM，且不包含内核、
    /// 启动数据或其他仍在使用的内存。在分配器存活期间，不得通过其他分配器
    /// 或外部访问复用这些区间；通过页框句柄访问时必须遵守其有效期。
    /// 纯记账测试可以使用合成区间，但不得通过生成的地址访问物理内存。
    pub unsafe fn new(map: MemoryMap) -> Result<Self, &'static str> {
        let mut total = 0usize;
        for region in map.usable() {
            if region.start % PAGE_SIZE != 0 || region.end % PAGE_SIZE != 0 {
                return Err("invalid usable frame region");
            }
            total = total
                .checked_add((region.end - region.start) / PAGE_SIZE)
                .ok_or("frame count overflow")?;
        }
        let words = total.div_ceil(WORD_BITS);
        let mut free = Vec::new();
        free.try_reserve_exact(words)
            .map_err(|_| "insufficient heap for frame bitmap")?;
        free.resize(words, usize::MAX);
        if total % WORD_BITS != 0 {
            free[words - 1] = (1usize << (total % WORD_BITS)) - 1;
        }
        Ok(Self {
            regions: map.into_usable(),
            total,
            state: RefCell::new(State {
                free,
                available: total,
                hint: 0,
            }),
        })
    }

    /// 返回管理的总页数，包括当前已分配的页。
    pub fn total(&self) -> usize {
        self.total
    }

    /// 返回当前空闲页数。
    pub fn available(&self) -> usize {
        self.state.borrow().available
    }

    /// 分配一个未清零的页框；耗尽时返回 `None`，不申请额外堆内存。
    ///
    /// 物理地址零也是有效分配结果。返回的句柄销毁时自动回收页框。
    #[cfg_attr(not(feature = "ktest"), allow(dead_code))]
    pub fn alloc(&self) -> Option<Frame<'_>> {
        let mut state = self.state.borrow_mut();
        if state.available == 0 {
            return None;
        }
        let words = state.free.len();
        for offset in 0..words {
            let word = (state.hint + offset) % words;
            let bits = state.free[word];
            if bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                state.free[word] &= !(1usize << bit);
                state.available -= 1;
                state.hint = word;
                return Some(Frame {
                    allocator: self,
                    index: word * WORD_BITS + bit,
                });
            }
        }
        unreachable!("free frame count disagrees with bitmap");
    }
}

/// 独占一个物理页框；不可复制，销毁时自动归还所属分配器。
///
/// 本类型不提供安全的内存引用。调用者建立映射或通过裸指针访问页面时，
/// 必须保证所有访问和映射在句柄销毁前结束；物理地址本身不延长有效期。
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
        let mut state = self.allocator.state.borrow_mut();
        let word = self.index / WORD_BITS;
        let mask = 1usize << (self.index % WORD_BITS);
        debug_assert_eq!(state.free[word] & mask, 0);
        state.free[word] |= mask;
        state.available += 1;
        state.hint = word;
    }
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
