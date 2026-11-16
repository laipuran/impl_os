//! FDT 头部、内存保留表（memory reservation block）与各数据块边界校验。
//!
//! 格式定义参考 [Devicetree Specification v0.4 第 5 章]：
//! - §5.1 规定版本兼容性（当前解析器支持兼容 v17 布局的 DTB）；
//! - §5.2 定义 40 字节大端序头部字段；
//! - §5.3 ~ §5.6 定义内存保留表、结构块（structure block）与字符串块（strings block）的对齐和布局。
//!
//! [Devicetree Specification v0.4 第 5 章]: https://github.com/devicetree-org/devicetree-specification/blob/v0.4/source/chapter5-flattened-format.rst

use alloc::vec::Vec;

use super::{
    physical_region,
    wire::{be32, be64},
    Region,
};

pub(super) const MAGIC: u32 = 0xd00d_feed;
/// DTB v17 标准头部字节长度。
pub(crate) const HEADER_SIZE: usize = 40;
const STRUCTURE_OFFSET_FIELD: usize = 8;
const STRINGS_OFFSET_FIELD: usize = 12;
const STRINGS_SIZE_FIELD: usize = 32;
const STRUCTURE_SIZE_FIELD: usize = 36;

/// 校验 FDT 头部魔数、版本兼容性，并返回头部声明的 DTB 总字节长度（`totalsize`）。
///
/// # Errors
///
/// 当切片不足 40 字节、魔数不匹配、`totalsize` 越界或版本不兼容 v17 时返回错误。
pub fn total_size(header: &[u8]) -> Result<usize, &'static str> {
    if header.len() < HEADER_SIZE || be32(header, 0)? != MAGIC {
        return Err("invalid FDT magic or truncated header");
    }
    let size = be32(header, 4)? as usize;
    if size < HEADER_SIZE || size > isize::MAX as usize {
        return Err("invalid FDT total size");
    }
    // 要求 version >= 17 且 last_comp_version <= 17，确保遵循 v17 头部布局。
    if be32(header, 20)? < 17 || be32(header, 24)? > 17 {
        return Err("unsupported FDT version");
    }
    Ok(size)
}

pub(super) struct Blob<'a> {
    pub structure: &'a [u8],
    pub strings: &'a [u8],
    pub reserved: Vec<Region>,
}

impl<'a> Blob<'a> {
    pub fn parse(blob: &'a [u8]) -> Result<Self, &'static str> {
        let size = total_size(blob)?;
        let blob = blob.get(..size).ok_or("truncated FDT")?;
        let structure = block(blob, STRUCTURE_OFFSET_FIELD, STRUCTURE_SIZE_FIELD, 4)?;
        let strings = block(blob, STRINGS_OFFSET_FIELD, STRINGS_SIZE_FIELD, 1)?;
        let (reservation_block, reserved) = reservations(blob)?;
        if overlaps(structure, strings)
            || overlaps(structure, reservation_block)
            || overlaps(strings, reservation_block)
        {
            return Err("overlapping FDT blocks");
        }
        Ok(Self {
            structure: &blob[structure.start..structure.end],
            strings: &blob[strings.start..strings.end],
            reserved,
        })
    }
}

fn reservations(blob: &[u8]) -> Result<(Region, Vec<Region>), &'static str> {
    let start = be32(blob, 16)? as usize;
    if start < HEADER_SIZE || start % 8 != 0 {
        return Err("invalid FDT reservation offset");
    }
    let mut reserved = Vec::new();
    let mut cursor = start;
    loop {
        let entry = blob
            .get(cursor..cursor.checked_add(16).ok_or("FDT offset overflow")?)
            .ok_or("unterminated FDT reservation map")?;
        let address = be64(entry, 0)?;
        let size = be64(entry, 8)?;
        cursor += 16;
        if address == 0 && size == 0 {
            break;
        }
        reserved.push(physical_region(address, size)?);
    }
    Ok((Region { start, end: cursor }, reserved))
}

fn block(
    blob: &[u8],
    offset_field: usize,
    size_field: usize,
    alignment: usize,
) -> Result<Region, &'static str> {
    let start = be32(blob, offset_field)? as usize;
    let block_size = be32(blob, size_field)? as usize;
    let region = Region::new(start, block_size)?;
    if start < HEADER_SIZE || start % alignment != 0 || region.end > blob.len() {
        return Err("invalid FDT block bounds");
    }
    Ok(region)
}

fn overlaps(left: Region, right: Region) -> bool {
    left.start < right.end && right.start < left.end
}
