//! 校验 FDT 头、保留表及数据块布局，向树解析器提供已验证的切片。
//!
//! 依据 [Devicetree Specification v0.4 第 5 章]：§5.1 规定格式版本兼容，
//! §5.2 定义大端头部字段，§5.3～5.6 定义数据块布局及对齐。
//! 规范版本 v0.4 与其描述的 DTB 二进制格式版本 17 是不同的版本号。
//!
//! [Devicetree Specification v0.4 第 5 章]: https://github.com/devicetree-org/devicetree-specification/blob/v0.4/source/chapter5-flattened-format.rst

use alloc::vec::Vec;

use super::{
    physical_region,
    wire::{be32, be64},
    Region,
};

pub(super) const MAGIC: u32 = 0xd00d_feed;
/// DTB 二进制格式 v17 的固定头部长度，单位为字节。
pub(crate) const HEADER_SIZE: usize = 40;
const STRUCTURE_OFFSET_FIELD: usize = 8;
const STRINGS_OFFSET_FIELD: usize = 12;
const STRINGS_SIZE_FIELD: usize = 32;
const STRUCTURE_SIZE_FIELD: usize = 36;

/// 校验 FDT 头并返回大端编码的总长度，不解引用物理地址。
pub fn total_size(header: &[u8]) -> Result<usize, &'static str> {
    if header.len() < HEADER_SIZE || be32(header, 0)? != MAGIC {
        return Err("invalid FDT magic or truncated header");
    }
    let size = be32(header, 4)? as usize;
    if size < HEADER_SIZE || size > isize::MAX as usize {
        return Err("invalid FDT total size");
    }
    // 只支持布局已固定的 v17；未来兼容版本可使用同一布局。
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
