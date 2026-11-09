//! 构造测试用 FDT；只负责编码，不调用待测解析器。

use alloc::{vec, vec::Vec};

use super::super::{
    header::MAGIC,
    tree::{BEGIN_NODE, END, END_NODE, PROP},
};

pub(super) struct Builder {
    tree: Vec<u8>,
    strings: Vec<u8>,
    pub reservations: Vec<(u64, u64)>,
}

impl Builder {
    pub fn new(address_cells: u32, size_cells: u32) -> Self {
        let mut builder = Self {
            tree: vec![],
            strings: vec![],
            reservations: vec![],
        };
        builder.begin(b"");
        builder.prop(b"#address-cells", &address_cells.to_be_bytes());
        builder.prop(b"#size-cells", &size_cells.to_be_bytes());
        builder
    }

    pub fn begin(&mut self, name: &[u8]) {
        self.tree.extend_from_slice(&BEGIN_NODE.to_be_bytes());
        self.tree.extend_from_slice(name);
        self.tree.push(0);
        self.pad();
    }

    pub fn end(&mut self) {
        self.tree.extend_from_slice(&END_NODE.to_be_bytes());
    }

    pub fn prop(&mut self, name: &[u8], value: &[u8]) {
        self.tree.extend_from_slice(&PROP.to_be_bytes());
        self.tree
            .extend_from_slice(&(value.len() as u32).to_be_bytes());
        self.tree
            .extend_from_slice(&(self.strings.len() as u32).to_be_bytes());
        self.strings.extend_from_slice(name);
        self.strings.push(0);
        self.tree.extend_from_slice(value);
        self.pad();
    }

    pub fn cells(&mut self, name: &[u8], values: &[u32]) {
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect();
        self.prop(name, &bytes);
    }

    pub fn memory(&mut self, reg: &[u32]) {
        self.begin(b"memory@80000000");
        self.prop(b"device_type", b"memory\0");
        self.cells(b"reg", reg);
        self.end();
    }

    fn pad(&mut self) {
        while self.tree.len() % 4 != 0 {
            self.tree.push(0);
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.end();
        self.tree.extend_from_slice(&END.to_be_bytes());
        let mut blob = vec![0; 40];
        for (start, size) in self.reservations {
            blob.extend_from_slice(&start.to_be_bytes());
            blob.extend_from_slice(&size.to_be_bytes());
        }
        blob.extend_from_slice(&[0; 16]);
        let tree_offset = blob.len();
        blob.extend_from_slice(&self.tree);
        let string_offset = blob.len();
        blob.extend_from_slice(&self.strings);
        let header = [
            MAGIC,
            blob.len() as u32,
            tree_offset as u32,
            string_offset as u32,
            40,
            17,
            16,
            0,
            self.strings.len() as u32,
            self.tree.len() as u32,
        ];
        for (word, bytes) in header.iter().zip(blob[..40].chunks_exact_mut(4)) {
            bytes.copy_from_slice(&word.to_be_bytes());
        }
        blob
    }
}

pub(super) fn basic() -> Builder {
    let mut builder = Builder::new(2, 2);
    builder.memory(&[0, 0x80000000, 0, 0x40000000]);
    builder
}
