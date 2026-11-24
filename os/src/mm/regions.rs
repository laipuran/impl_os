//! 物理地址区间的排序合并、保留区扣除与页边界对齐。

use alloc::vec::Vec;

/// 半开物理地址区间 `[start, end)`（单位：字节）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// 区间起始物理地址（包含）。
    pub start: usize,
    /// 区间结束物理地址（不包含）。
    pub end: usize,
}

impl Region {
    /// 根据起始地址与非零字节长度构造区间，若长度为 0 或发生地址加法溢出则返回错误。
    pub fn new(start: usize, size: usize) -> Result<Self, &'static str> {
        let end = start.checked_add(size).ok_or("physical range overflow")?;
        if start == end {
            return Err("empty physical range");
        }
        Ok(Self { start, end })
    }
}

/// 归一化后的物理内存布局：包含有序且不重叠的 RAM 区间、保留区间及按页对齐的可用区间。
#[derive(Debug)]
pub struct MemoryMap {
    ram: Vec<Region>,
    reserved: Vec<Region>,
    usable: Vec<Region>,
}

impl MemoryMap {
    /// 返回已排序并合并重叠部分的物理 RAM 区间。
    pub fn ram(&self) -> &[Region] {
        &self.ram
    }

    /// 返回已排序并合并重叠部分的保留区间（可能包含位于 RAM 之外的固件或 MMIO 保留区）。
    pub fn reserved(&self) -> &[Region] {
        &self.reserved
    }

    /// 返回从 RAM 中扣除保留区并向内按页大小对齐后的可用物理区间。
    ///
    /// 返回的各区间非空、按地址升序排列且互不重叠；若无完整可用页则返回空切片。
    pub fn usable(&self) -> &[Region] {
        &self.usable
    }

    /// 消耗当前结构并移交可用物理区间列表的所有权。
    pub fn into_usable(self) -> Vec<Region> {
        self.usable
    }

    /// 归并原始 RAM 与保留区间，从 RAM 中扣除所有保留部分，并将剩余区间向内按 `page_size` 对齐。
    ///
    /// 若扣除或向内页对齐后不剩任何完整页面，仍会返回有效的 `MemoryMap`（其 [`Self::usable`] 为空）。
    ///
    /// # Errors
    ///
    /// 当任一输入区间满足 `start >= end`、`ram` 为空，或 `page_size` 不是 2 的幂时返回错误。
    pub fn new(
        mut ram: Vec<Region>,
        mut reserved: Vec<Region>,
        page_size: usize,
    ) -> Result<Self, &'static str> {
        if !page_size.is_power_of_two() {
            return Err("invalid page size");
        }
        normalize(&mut ram)?;
        normalize(&mut reserved)?;
        if ram.is_empty() {
            return Err("no RAM described");
        }
        let mut usable = Vec::new();
        for region in &ram {
            let mut cursor = region.start;
            for exclusion in &reserved {
                if exclusion.end <= cursor {
                    continue;
                }
                if exclusion.start >= region.end {
                    break;
                }
                if exclusion.start > cursor {
                    push_aligned(&mut usable, cursor, exclusion.start, page_size);
                }
                cursor = cursor.max(exclusion.end).min(region.end);
                if cursor == region.end {
                    break;
                }
            }
            push_aligned(&mut usable, cursor, region.end, page_size);
        }
        Ok(Self {
            ram,
            reserved,
            usable,
        })
    }
}

/// 校验区间非空，按起始地址升序排列并原地合并重叠或相邻的区间。
fn normalize(regions: &mut Vec<Region>) -> Result<(), &'static str> {
    if regions.iter().any(|region| region.start >= region.end) {
        return Err("invalid physical range");
    }
    regions.sort_unstable_by_key(|region| region.start);
    let mut length = 0;
    for index in 0..regions.len() {
        let region = regions[index];
        if length > 0 && regions[length - 1].end >= region.start {
            regions[length - 1].end = regions[length - 1].end.max(region.end);
        } else {
            regions[length] = region;
            length += 1;
        }
    }
    regions.truncate(length);
    Ok(())
}

/// 将区间 `[start, end)` 向内收缩对齐到 `page_size` 边界，若对齐后仍包含至少一页则追加到 `output`。
fn push_aligned(output: &mut Vec<Region>, start: usize, end: usize, page_size: usize) {
    // 若 `start` 接近 `usize::MAX` 导致向上取整溢出，说明该尾部碎片不足一页，直接忽略即可。
    let Some(start) = start.checked_add(page_size - 1) else {
        return;
    };
    let start = start & !(page_size - 1);
    let end = end & !(page_size - 1);
    if start < end {
        output.push(Region { start, end });
    }
}

#[cfg(any(test, feature = "ktest"))]
#[path = "../ktest/mm/regions.rs"]
pub mod tests;
