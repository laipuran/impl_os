//! 解释节点中的内存属性，收集 RAM、静态保留区和 initrd。

use alloc::vec::Vec;

use super::{
    physical_region,
    wire::{be32, be64, cstring},
    BootMemory, Region,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Cells {
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
pub(super) enum NodeKind {
    Root,
    Chosen,
    ReservedMemory,
    ReservedRegion,
    Other,
}

impl NodeKind {
    pub fn child(self, name: &[u8]) -> Self {
        match (self, name) {
            (Self::Root, b"reserved-memory") => Self::ReservedMemory,
            (Self::Root, b"chosen") => Self::Chosen,
            (Self::ReservedMemory, _) => Self::ReservedRegion,
            _ => Self::Other,
        }
    }
}

pub(super) struct Properties<'a> {
    pub cells: Cells,
    pub enabled: bool,
    is_memory_node: bool,
    reg: Option<&'a [u8]>,
    has_size_property: bool,
    ranges: Option<&'a [u8]>,
    initrd_start: Option<u64>,
    initrd_end: Option<u64>,
}

impl<'a> Properties<'a> {
    pub fn new(enabled: bool) -> Self {
        Self {
            cells: Cells::default(),
            enabled,
            is_memory_node: false,
            reg: None,
            has_size_property: false,
            ranges: None,
            initrd_start: None,
            initrd_end: None,
        }
    }

    pub fn set(&mut self, name: &[u8], value: &'a [u8]) -> Result<(), &'static str> {
        match name {
            b"#address-cells" => self.cells.address = cell_count(value)?,
            b"#size-cells" => self.cells.size = cell_count(value)?,
            b"device_type" => self.is_memory_node = string_value(value)? == b"memory",
            b"status" => {
                let status = string_value(value)?;
                self.enabled &= status == b"okay" || status == b"ok";
            }
            b"reg" => self.reg = Some(value),
            b"size" => self.has_size_property = true,
            b"ranges" => self.ranges = Some(value),
            b"linux,initrd-start" => self.initrd_start = Some(cell_value(value)?),
            b"linux,initrd-end" => self.initrd_end = Some(cell_value(value)?),
            _ => {}
        }
        Ok(())
    }

    pub fn collect(
        &self,
        kind: NodeKind,
        depth: usize,
        parent_cells: Cells,
        output: &mut BootMemory,
    ) -> Result<(), &'static str> {
        if !self.enabled {
            return Ok(());
        }

        if kind == NodeKind::Chosen {
            self.collect_initrd(&mut output.reserved)?;
        }

        if self.is_memory_node {
            if depth != 1 || kind != NodeKind::Other {
                return Err("unsupported FDT memory node location");
            }
            decode_regions(
                self.reg.ok_or("memory node without reg")?,
                parent_cells,
                &mut output.ram,
            )?;
        }

        if kind == NodeKind::ReservedMemory
            && (self.ranges.is_none_or(|ranges| !ranges.is_empty()) || self.cells != parent_cells)
        {
            return Err("unsupported reserved-memory addressing");
        }

        if kind == NodeKind::ReservedRegion {
            self.collect_reserved(parent_cells, &mut output.reserved)?;
        }
        Ok(())
    }

    fn collect_initrd(&self, reserved: &mut Vec<Region>) -> Result<(), &'static str> {
        match (self.initrd_start, self.initrd_end) {
            (Some(start), Some(end)) if end > start => {
                reserved.push(physical_region(start, end - start)?);
                Ok(())
            }
            (None, None) => Ok(()),
            _ => Err("invalid initrd range"),
        }
    }

    fn collect_reserved(
        &self,
        cells: Cells,
        reserved: &mut Vec<Region>,
    ) -> Result<(), &'static str> {
        if let Some(reg) = self.reg {
            decode_regions(reg, cells, reserved)
        } else if self.has_size_property {
            Err("dynamic reserved-memory allocation is unsupported")
        } else {
            Err("reserved-memory child without reg or size")
        }
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
