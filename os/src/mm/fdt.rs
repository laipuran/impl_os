//! 只解析启动内存所需的 FDT 信息；不进行设备枚举，也不分配动态保留区。

use alloc::vec::Vec;

use super::regions::Region;

const MAGIC: u32 = 0xd00d_feed;
const HEADER_SIZE: usize = 40;
const BEGIN_NODE: u32 = 1;
const END_NODE: u32 = 2;
const PROP: u32 = 3;
const NOP: u32 = 4;
const END: u32 = 9;

/// 从设备树取得的物理 RAM 和必须保留的区间。
#[derive(Debug)]
pub struct BootMemory {
    /// 所有启用的 RAM 区间。
    pub ram: Vec<Region>,
    /// 固件静态保留区及 initrd 占用。
    pub reserved: Vec<Region>,
}

/// 通过架构提供的映射读取完整 FDT，并保留其物理存储区间。
///
/// # Errors
///
/// 地址无效、头部无效、长度超过平台容量或解析失败时返回错误。
///
/// # Safety
///
/// `mapped` 必须是 `physical` 对应的可读内核地址，头部和头部描述的
/// 完整 FDT 必须可读且不变。`capacity` 是平台允许的最大 FDT 长度。
pub unsafe fn read(
    physical: usize,
    mapped: usize,
    capacity: usize,
) -> Result<BootMemory, &'static str> {
    if physical % 8 != 0 || mapped == 0 || mapped.checked_add(HEADER_SIZE).is_none() {
        return Err("invalid FDT address");
    }
    // SAFETY: 调用者保证映射后的头部可读且不变；已检查空地址和地址溢出。
    let header = unsafe { core::slice::from_raw_parts(mapped as *const u8, HEADER_SIZE) };
    let size = total_size(header)?;
    if size > capacity || mapped.checked_add(size).is_none() {
        return Err("FDT exceeds platform storage");
    }
    let region = Region::new(physical, size)?;
    // SAFETY: 调用者保证完整 FDT 可读；total_size 已限制长度，地址加长度
    // 无溢出，且读取期间没有写入者。
    let blob = unsafe { core::slice::from_raw_parts(mapped as *const u8, size) };
    let mut memory = parse(blob)?;
    memory.reserved.push(region);
    Ok(memory)
}

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

/// 校验并解析完整 FDT，拒绝截断、重叠的数据块及不支持的内存描述。
pub fn parse(blob: &[u8]) -> Result<BootMemory, &'static str> {
    let size = total_size(blob)?;
    let blob = blob.get(..size).ok_or("truncated FDT")?;
    let structure = block(blob, 8, 36, 4)?;
    let strings = block(blob, 12, 32, 1)?;
    let reserve_start = be32(blob, 16)? as usize;
    if reserve_start < HEADER_SIZE || reserve_start % 8 != 0 {
        return Err("invalid FDT reservation offset");
    }
    let mut output = BootMemory {
        ram: Vec::new(),
        reserved: Vec::new(),
    };
    let mut cursor = reserve_start;
    loop {
        let entry = blob
            .get(cursor..cursor.checked_add(16).ok_or("FDT offset overflow")?)
            .ok_or("unterminated FDT reservation map")?;
        let start = be64(entry, 0)?;
        let size = be64(entry, 8)?;
        cursor += 16;
        if start == 0 && size == 0 {
            break;
        }
        output.reserved.push(physical_region(start, size)?);
    }
    let reserved = Region {
        start: reserve_start,
        end: cursor,
    };
    if overlaps(structure, strings) || overlaps(structure, reserved) || overlaps(strings, reserved)
    {
        return Err("overlapping FDT blocks");
    }
    let mut tree = Tree {
        structure: &blob[structure.start..structure.end],
        strings: &blob[strings.start..strings.end],
        cursor: 0,
        output,
    };
    if tree.token()? != BEGIN_NODE {
        return Err("missing FDT root node");
    }
    tree.node(Cells::default(), true, Kind::Root, 0)?;
    if tree.token()? != END {
        return Err("missing FDT end token");
    }
    if tree.structure[tree.cursor..].iter().any(|byte| *byte != 0) {
        return Err("data after FDT end token");
    }
    if tree.output.ram.is_empty() {
        return Err("no enabled FDT memory node");
    }
    Ok(tree.output)
}

#[derive(Clone, Copy)]
struct Cells {
    address: usize,
    size: usize,
}

impl Default for Cells {
    fn default() -> Self {
        Self {
            address: 2,
            size: 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Root,
    Chosen,
    Reserved,
    Reservation,
    Other,
}

struct Properties<'a> {
    cells: Cells,
    enabled: bool,
    memory: bool,
    reg: Option<&'a [u8]>,
    dynamic_size: bool,
    ranges: Option<&'a [u8]>,
    initrd_start: Option<u64>,
    initrd_end: Option<u64>,
}

impl<'a> Properties<'a> {
    fn new(enabled: bool) -> Self {
        Self {
            cells: Cells::default(),
            enabled,
            memory: false,
            reg: None,
            dynamic_size: false,
            ranges: None,
            initrd_start: None,
            initrd_end: None,
        }
    }

    fn set(&mut self, name: &[u8], value: &'a [u8]) -> Result<(), &'static str> {
        match name {
            b"#address-cells" => self.cells.address = cell_count(value)?,
            b"#size-cells" => self.cells.size = cell_count(value)?,
            b"device_type" => self.memory = string_value(value)? == b"memory",
            b"status" => {
                let status = string_value(value)?;
                self.enabled &= status == b"okay" || status == b"ok";
            }
            b"reg" => self.reg = Some(value),
            b"size" => self.dynamic_size = true,
            b"ranges" => self.ranges = Some(value),
            b"linux,initrd-start" => self.initrd_start = Some(cell_value(value)?),
            b"linux,initrd-end" => self.initrd_end = Some(cell_value(value)?),
            _ => {}
        }
        Ok(())
    }
}

struct Tree<'a> {
    structure: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
    output: BootMemory,
}

impl Tree<'_> {
    fn token(&mut self) -> Result<u32, &'static str> {
        loop {
            let token = be32(self.structure, self.cursor)?;
            self.cursor += 4;
            if token != NOP {
                return Ok(token);
            }
        }
    }

    fn node(
        &mut self,
        parent_cells: Cells,
        parent_enabled: bool,
        kind: Kind,
        depth: usize,
    ) -> Result<(), &'static str> {
        // 限制递归栈占用；超过限制明确失败，不跳过可能包含 RAM 的节点。
        if depth > 32 {
            return Err("FDT nesting exceeds supported depth");
        }
        let (name, next) = cstring(self.structure, self.cursor)?;
        self.cursor = align4(next)?;
        if kind == Kind::Root && !name.is_empty() {
            return Err("invalid FDT root name");
        }
        let mut properties = Properties::new(parent_enabled);
        let mut names = Vec::new();
        let mut token = self.token()?;
        while token == PROP {
            let length = be32(self.structure, self.cursor)? as usize;
            let name_offset = be32(self.structure, self.cursor + 4)? as usize;
            self.cursor += 8;
            let end = self
                .cursor
                .checked_add(length)
                .ok_or("FDT length overflow")?;
            let value = self
                .structure
                .get(self.cursor..end)
                .ok_or("truncated FDT property")?;
            self.cursor = align4(end)?;
            let (property_name, _) = cstring(self.strings, name_offset)?;
            if names.contains(&property_name) {
                return Err("duplicate FDT property");
            }
            names.push(property_name);
            properties.set(property_name, value)?;
            token = self.token()?;
        }
        if properties.enabled {
            if kind == Kind::Chosen {
                match (properties.initrd_start, properties.initrd_end) {
                    (Some(start), Some(end)) if end > start => {
                        self.output
                            .reserved
                            .push(physical_region(start, end - start)?);
                    }
                    (None, None) => {}
                    _ => return Err("invalid initrd range"),
                }
            }
            if properties.memory {
                if depth != 1 || kind != Kind::Other {
                    return Err("unsupported FDT memory node location");
                }
                decode_regions(
                    properties.reg.ok_or("memory node without reg")?,
                    parent_cells,
                    &mut self.output.ram,
                )?;
            }
            if kind == Kind::Reserved
                && (properties.ranges.is_none_or(|ranges| !ranges.is_empty())
                    || properties.cells.address != parent_cells.address
                    || properties.cells.size != parent_cells.size)
            {
                return Err("unsupported reserved-memory addressing");
            }
            if kind == Kind::Reservation {
                if let Some(reg) = properties.reg {
                    decode_regions(reg, parent_cells, &mut self.output.reserved)?;
                } else if properties.dynamic_size {
                    return Err("dynamic reserved-memory allocation is unsupported");
                } else {
                    return Err("reserved-memory child without reg or size");
                }
            }
        }
        while token == BEGIN_NODE {
            let (child_name, _) = cstring(self.structure, self.cursor)?;
            let child_kind = if kind == Kind::Root && child_name == b"reserved-memory" {
                Kind::Reserved
            } else if kind == Kind::Root && child_name == b"chosen" {
                Kind::Chosen
            } else if kind == Kind::Reserved {
                Kind::Reservation
            } else {
                Kind::Other
            };
            if kind == Kind::Reservation {
                return Err("nested reserved-memory child is unsupported");
            }
            self.node(properties.cells, properties.enabled, child_kind, depth + 1)?;
            token = self.token()?;
        }
        if token != END_NODE {
            return Err("invalid FDT node or property ordering");
        }
        Ok(())
    }
}

fn decode_regions(reg: &[u8], cells: Cells, output: &mut Vec<Region>) -> Result<(), &'static str> {
    if !(1..=2).contains(&cells.address) || !(1..=2).contains(&cells.size) {
        return Err("unsupported memory address or size cells");
    }
    let stride = (cells.address + cells.size) * 4;
    if reg.is_empty() || reg.len() % stride != 0 {
        return Err("invalid memory reg length");
    }
    for entry in reg.chunks_exact(stride) {
        let address = cell_value(&entry[..cells.address * 4])?;
        let size = cell_value(&entry[cells.address * 4..])?;
        output.push(physical_region(address, size)?);
    }
    Ok(())
}

fn physical_region(start: u64, size: u64) -> Result<Region, &'static str> {
    Region::new(
        usize::try_from(start).map_err(|_| "physical address exceeds usize")?,
        usize::try_from(size).map_err(|_| "physical size exceeds usize")?,
    )
}

fn cell_count(value: &[u8]) -> Result<usize, &'static str> {
    if value.len() != 4 {
        return Err("invalid FDT cell count");
    }
    Ok(be32(value, 0)? as usize)
}

fn cell_value(value: &[u8]) -> Result<u64, &'static str> {
    match value.len() {
        4 => Ok(be32(value, 0)? as u64),
        8 => be64(value, 0),
        _ => Err("unsupported FDT cell width"),
    }
}

fn string_value(value: &[u8]) -> Result<&[u8], &'static str> {
    let (string, end) = cstring(value, 0)?;
    if end != value.len() {
        return Err("invalid FDT string property");
    }
    Ok(string)
}

fn cstring(data: &[u8], start: usize) -> Result<(&[u8], usize), &'static str> {
    let tail = data.get(start..).ok_or("invalid FDT string offset")?;
    let length = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or("unterminated FDT string")?;
    Ok((&tail[..length], start + length + 1))
}

fn align4(value: usize) -> Result<usize, &'static str> {
    Ok(value.checked_add(3).ok_or("FDT alignment overflow")? & !3)
}

fn be32(data: &[u8], offset: usize) -> Result<u32, &'static str> {
    let end = offset.checked_add(4).ok_or("FDT offset overflow")?;
    let bytes = data.get(offset..end).ok_or("truncated FDT word")?;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

fn be64(data: &[u8], offset: usize) -> Result<u64, &'static str> {
    Ok(((be32(data, offset)? as u64) << 32) | be32(data, offset + 4)? as u64)
}

fn block(
    blob: &[u8],
    offset: usize,
    length: usize,
    alignment: usize,
) -> Result<Region, &'static str> {
    let start = be32(blob, offset)? as usize;
    let length = be32(blob, length)? as usize;
    let region = Region::new(start, length)?;
    if start < HEADER_SIZE || start % alignment != 0 || region.end > blob.len() {
        return Err("invalid FDT block bounds");
    }
    Ok(region)
}

fn overlaps(left: Region, right: Region) -> bool {
    left.start < right.end && right.start < left.end
}

#[cfg(any(test, feature = "ktest"))]
#[path = "fdt/tests/mod.rs"]
pub mod tests;
