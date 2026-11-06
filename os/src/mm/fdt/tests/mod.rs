//! 构造小型 FDT 验证解析，不依赖宿主工具或实际物理地址。

use alloc::{vec, vec::Vec};

use super::{parse, Region, BEGIN_NODE, END, END_NODE, MAGIC, PROP};

struct Builder {
    tree: Vec<u8>,
    strings: Vec<u8>,
    reservations: Vec<(u64, u64)>,
}

impl Builder {
    fn new(address_cells: u32, size_cells: u32) -> Self {
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

    fn begin(&mut self, name: &[u8]) {
        self.tree.extend_from_slice(&BEGIN_NODE.to_be_bytes());
        self.tree.extend_from_slice(name);
        self.tree.push(0);
        self.pad();
    }

    fn end(&mut self) {
        self.tree.extend_from_slice(&END_NODE.to_be_bytes());
    }

    fn prop(&mut self, name: &[u8], value: &[u8]) {
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

    fn cells(&mut self, name: &[u8], values: &[u32]) {
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect();
        self.prop(name, &bytes);
    }

    fn memory(&mut self, reg: &[u32]) {
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

    fn finish(mut self) -> Vec<u8> {
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

fn basic() -> Builder {
    let mut builder = Builder::new(2, 2);
    builder.memory(&[0, 0x80000000, 0, 0x40000000]);
    builder
}

#[cfg_attr(test, test)]
fn multiple_banks_and_reservations() {
    let mut builder = Builder::new(2, 2);
    builder.reservations.push((0x81000000, 4096));
    builder.memory(&[0, 0x80000000, 0, 0x10000000, 1, 0, 0, 0x20000000]);
    builder.begin(b"memory@200000000");
    builder.prop(b"device_type", b"memory\0");
    builder.cells(b"reg", &[2, 0, 0, 0x10000000]);
    builder.end();
    builder.begin(b"reserved-memory");
    builder.cells(b"#address-cells", &[2]);
    builder.cells(b"#size-cells", &[2]);
    builder.prop(b"ranges", &[]);
    builder.begin(b"firmware@80000000");
    builder.cells(b"reg", &[0, 0x80000000, 0, 0x60000]);
    builder.prop(b"no-map", &[]);
    builder.end();
    builder.end();
    builder.begin(b"chosen");
    builder.cells(b"linux,initrd-start", &[0, 0x82000000]);
    builder.cells(b"linux,initrd-end", &[0, 0x82100000]);
    builder.end();
    let memory = parse(&builder.finish()).unwrap();
    assert_eq!(
        memory.ram,
        vec![
            Region::new(0x80000000, 0x10000000).unwrap(),
            Region::new(0x100000000, 0x20000000).unwrap(),
            Region::new(0x200000000, 0x10000000).unwrap(),
        ]
    );
    assert_eq!(
        memory.reserved,
        vec![
            Region::new(0x81000000, 4096).unwrap(),
            Region::new(0x80000000, 0x60000).unwrap(),
            Region::new(0x82000000, 0x100000).unwrap(),
        ]
    );
}

#[cfg_attr(test, test)]
fn single_cells_and_disabled_nodes() {
    let mut builder = Builder::new(1, 1);
    builder.memory(&[0x80000000, 0x8000000]);
    builder.begin(b"memory@90000000");
    builder.prop(b"device_type", b"memory\0");
    builder.prop(b"status", b"disabled\0");
    builder.end();
    let memory = parse(&builder.finish()).unwrap();
    assert_eq!(
        memory.ram,
        vec![Region::new(0x80000000, 0x8000000).unwrap()]
    );
}

#[cfg_attr(test, test)]
fn unsupported_descriptions_fail() {
    let mut builder = basic();
    builder.begin(b"reserved-memory");
    builder.cells(b"#address-cells", &[2]);
    builder.cells(b"#size-cells", &[2]);
    builder.prop(b"ranges", &[]);
    builder.begin(b"dynamic");
    builder.cells(b"size", &[0, 4096]);
    builder.end();
    builder.end();
    assert_eq!(
        parse(&builder.finish()).unwrap_err(),
        "dynamic reserved-memory allocation is unsupported"
    );

    let mut builder = basic();
    builder.begin(b"reserved-memory");
    builder.prop(b"ranges", &[0; 16]);
    builder.end();
    assert!(parse(&builder.finish()).is_err());

    let mut builder = Builder::new(3, 2);
    builder.memory(&[0; 5]);
    assert!(parse(&builder.finish()).is_err());

    let mut builder = basic();
    builder.begin(b"chosen");
    builder.cells(b"linux,initrd-start", &[0x82000000]);
    builder.end();
    assert!(parse(&builder.finish()).is_err());
}

#[cfg_attr(test, test)]
fn malformed_blobs_fail() {
    let blob = basic().finish();
    for length in 0..blob.len() {
        assert!(parse(&blob[..length]).is_err());
    }
    for (offset, value) in [
        (0, 0),         // magic
        (4, 39),        // total size
        (8, 40),        // structure overlaps reservation map
        (12, u32::MAX), // strings outside blob
        (16, 41),       // reservation map alignment
        (20, 16),       // unsupported version
        (24, 18),       // incompatible layout
        (32, u32::MAX), // strings length
        (36, u32::MAX), // structure length
    ] {
        let mut damaged = blob.clone();
        damaged[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert!(parse(&damaged).is_err());
    }
    let mut builder = Builder::new(2, 2);
    builder.cells(b"#address-cells", &[2]);
    builder.memory(&[0, 0x80000000, 0, 0x1000]);
    assert!(parse(&builder.finish()).is_err());

    let mut builder = Builder::new(2, 2);
    builder.memory(&[0, 0x80000000, 0]);
    assert!(parse(&builder.finish()).is_err());

    let mut builder = Builder::new(2, 2);
    builder.memory(&[u32::MAX, u32::MAX, 0, 1]);
    assert!(parse(&builder.finish()).is_err());
}

#[cfg_attr(test, test)]
fn byte_mutations_do_not_panic() {
    let original = basic().finish();
    for index in 0..original.len() {
        let mut blob = original.clone();
        blob[index] ^= 0xff;
        let _ = parse(&blob);
    }
}

#[cfg_attr(test, test)]
fn deep_nodes_fail() {
    let mut builder = basic();
    for _ in 0..34 {
        builder.begin(b"bus");
    }
    for _ in 0..34 {
        builder.end();
    }
    assert_eq!(
        parse(&builder.finish()).unwrap_err(),
        "FDT nesting exceeds supported depth"
    );
}

#[cfg_attr(test, test)]
fn mapped_blob_read() {
    #[repr(align(8))]
    struct Storage([u8; 1024]);

    let blob = basic().finish();
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;
    // SAFETY: 对齐的局部数组包含完整且不变的 FDT，调用期间始终有效；
    // 物理地址仅作为元数据记录，不用于解引用。
    let memory = unsafe { super::read(0x100000, mapped, storage.0.len()) }.unwrap();
    assert!(memory
        .reserved
        .contains(&Region::new(0x100000, blob.len()).unwrap()));
    // SAFETY: 同一有效 FDT；较小容量应在构造完整切片前被拒绝。
    assert!(unsafe { super::read(0x100000, mapped, blob.len() - 1) }.is_err());
}

/// 验证多 RAM 区间、保留区、initrd、错误描述和截断输入。
pub fn kernel_test() {
    multiple_banks_and_reservations();
    single_cells_and_disabled_nodes();
    unsupported_descriptions_fail();
    malformed_blobs_fail();
    byte_mutations_do_not_panic();
    deep_nodes_fail();
    mapped_blob_read();
}
