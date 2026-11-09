//! 遍历结构 token、检查属性顺序和节点嵌套，不解释平台地址。

use alloc::vec::Vec;

use super::{
    header::Blob,
    memory::{Cells, NodeKind, Properties},
    wire::{align4, be32, cstring},
    BootMemory,
};

pub(super) const BEGIN_NODE: u32 = 1;
pub(super) const END_NODE: u32 = 2;
pub(super) const PROP: u32 = 3;
const NOP: u32 = 4;
pub(super) const END: u32 = 9;

pub(super) fn parse(blob: Blob<'_>) -> Result<BootMemory, &'static str> {
    let mut tree = Tree {
        structure: blob.structure,
        strings: blob.strings,
        cursor: 0,
        output: BootMemory {
            ram: Vec::new(),
            reserved: blob.reserved,
        },
    };
    if tree.token()? != BEGIN_NODE {
        return Err("missing FDT root node");
    }
    tree.node(Cells::default(), true, NodeKind::Root, 0)?;
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

struct Tree<'a> {
    structure: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
    output: BootMemory,
}

impl<'a> Tree<'a> {
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
        kind: NodeKind,
        depth: usize,
    ) -> Result<(), &'static str> {
        // 限制递归栈占用；超过限制明确失败，不跳过可能包含 RAM 的节点。
        if depth > 32 {
            return Err("FDT nesting exceeds supported depth");
        }
        let (name, next) = cstring(self.structure, self.cursor)?;
        self.cursor = align4(next)?;
        if kind == NodeKind::Root && !name.is_empty() {
            return Err("invalid FDT root name");
        }
        let (properties, mut token) = self.properties(parent_enabled)?;
        properties.collect(kind, depth, parent_cells, &mut self.output)?;
        while token == BEGIN_NODE {
            let (name, _) = cstring(self.structure, self.cursor)?;
            if kind == NodeKind::ReservedRegion {
                return Err("nested reserved-memory child is unsupported");
            }
            self.node(
                properties.cells,
                properties.enabled,
                kind.child(name),
                depth + 1,
            )?;
            token = self.token()?;
        }
        if token != END_NODE {
            return Err("invalid FDT node or property ordering");
        }
        Ok(())
    }

    // 返回属性及第一个非属性 token，使 node 独占子节点和 END_NODE 的处理。
    fn properties(&mut self, enabled: bool) -> Result<(Properties<'a>, u32), &'static str> {
        let mut properties = Properties::new(enabled);
        let mut names = Vec::new();
        let mut token = self.token()?;
        while token == PROP {
            let (name, value) = self.property()?;
            if names.contains(&name) {
                return Err("duplicate FDT property");
            }
            names.push(name);
            properties.set(name, value)?;
            token = self.token()?;
        }
        Ok((properties, token))
    }

    fn property(&mut self) -> Result<(&'a [u8], &'a [u8]), &'static str> {
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
        let (name, _) = cstring(self.strings, name_offset)?;
        Ok((name, value))
    }
}
