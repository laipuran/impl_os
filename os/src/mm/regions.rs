//! 物理内存区间的校验、保留区扣除和页对齐，不访问区间中的内存。

use alloc::vec::Vec;

/// 以字节为单位的半开物理地址区间。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// 包含在区间内的物理起点。
    pub start: usize,
    /// 不包含在区间内的物理终点。
    pub end: usize,
}

impl Region {
    /// 从物理起点和非零长度构造区间，拒绝地址溢出。
    pub fn new(start: usize, size: usize) -> Result<Self, &'static str> {
        let end = start.checked_add(size).ok_or("physical range overflow")?;
        if start == end {
            return Err("empty physical range");
        }
        Ok(Self { start, end })
    }
}

/// 已排序、合并的 RAM、保留区和页对齐可用区间。
#[derive(Debug)]
pub struct MemoryMap {
    ram: Vec<Region>,
    reserved: Vec<Region>,
    usable: Vec<Region>,
}

impl MemoryMap {
    /// 返回已排序、合并的平台 RAM 区间。
    pub fn ram(&self) -> &[Region] {
        &self.ram
    }

    /// 返回已排序、合并的保留区间，可以包含 RAM 之外的区域。
    pub fn reserved(&self) -> &[Region] {
        &self.reserved
    }

    /// 返回扣除保留区后的完整页区间；各区间非空、按地址排序且互不重叠。
    ///
    /// 起止地址均按构造时指定的页大小对齐。没有完整可用页时返回空切片。
    pub fn usable(&self) -> &[Region] {
        &self.usable
    }

    /// 消耗内存地图，移交与 [`Self::usable`] 相同的可用区间。
    pub fn into_usable(self) -> Vec<Region> {
        self.usable
    }

    /// 扣除保留区后向内按页对齐；不保留不完整的页。
    ///
    /// 没有完整可用页时，地图仍有效，[`Self::usable`] 返回空切片。
    ///
    /// # Errors
    ///
    /// 区间无效、未描述 RAM 或页大小不是二次幂时返回错误。
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

fn push_aligned(output: &mut Vec<Region>, start: usize, end: usize, page_size: usize) {
    // 最后一个不完整页的向上对齐可能溢出，此时没有可保留的完整页。
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
#[path = "regions/tests/mod.rs"]
pub mod tests;
