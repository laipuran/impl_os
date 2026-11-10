//! 多段 RAM、保留区及禁用节点的安全切片解析。

use alloc::vec;

use super::super::{parse, Region};
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

pub(super) fn run() {
    multiple_banks_and_reservations();
    single_cells_and_disabled_nodes();
    disabled_reserved_memory_skips_children();
}
