//! 多段 RAM、保留区、禁用节点和已映射 FDT 的读取。

use alloc::vec;

use super::super::{parse, read, Region};
use super::builder::{basic, Builder};

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
fn disabled_reserved_memory_skips_children() {
    let mut builder = basic();
    builder.begin(b"reserved-memory");
    builder.prop(b"status", b"disabled\0");
    builder.begin(b"dynamic");
    builder.cells(b"size", &[0, 4096]);
    builder.end();
    builder.end();
    let memory = parse(&builder.finish()).unwrap();
    assert!(memory.reserved.is_empty());
    assert_eq!(
        memory.ram,
        vec![Region::new(0x80000000, 0x40000000).unwrap()]
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
    let memory = unsafe { read(0x100000, mapped, storage.0.len()) }.unwrap();
    assert!(memory
        .reserved
        .contains(&Region::new(0x100000, blob.len()).unwrap()));
    // SAFETY: 同一有效 FDT；较小容量应在构造完整切片前被拒绝。
    assert!(unsafe { read(0x100000, mapped, blob.len() - 1) }.is_err());
}

pub(super) fn run() {
    multiple_banks_and_reservations();
    single_cells_and_disabled_nodes();
    disabled_reserved_memory_skips_children();
    mapped_blob_read();
}
