//! 非法与损坏设备树测试：验证不支持的保留区格式、截断/重叠数据块及超深节点树的错误拦截。

use super::super::parse;
use crate::ktest::support::fdt::{basic, Builder};

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
        assert!(
            parse(&blob[..length]).is_err(),
            "truncated FDT accepted: length={length}"
        );
    }
    for (offset, value) in [
        (0, 0),         // 非法魔数
        (4, 39),        // totalsize 小于 40 字节头部
        (8, 40),        // 结构块与内存保留表重叠
        (12, u32::MAX), // 字符串块偏移越界
        (16, 41),       // 内存保留表未按 8 字节对齐
        (20, 16),       // 版本号低于 v17
        (24, 18),       // last_comp_version 高于 v17
        (32, u32::MAX), // 字符串块长度越界
        (36, u32::MAX), // 结构块长度越界
    ] {
        let mut damaged = blob.clone();
        damaged[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert!(
            parse(&damaged).is_err(),
            "damaged FDT header accepted: offset={offset:#x}, value={value:#x}"
        );
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
fn properties_after_children_fail() {
    let mut builder = basic();
    builder.cells(b"#address-cells", &[2]);
    assert_eq!(
        parse(&builder.finish()).unwrap_err(),
        "invalid FDT node or property ordering"
    );
}

#[cfg_attr(test, test)]
fn nested_reserved_memory_fails() {
    let mut builder = basic();
    builder.begin(b"reserved-memory");
    builder.cells(b"#address-cells", &[2]);
    builder.cells(b"#size-cells", &[2]);
    builder.prop(b"ranges", &[]);
    builder.begin(b"firmware");
    builder.cells(b"reg", &[0, 0x81000000, 0, 4096]);
    builder.begin(b"nested");
    builder.end();
    builder.end();
    builder.end();
    assert_eq!(
        parse(&builder.finish()).unwrap_err(),
        "nested reserved-memory child is unsupported"
    );
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

pub(super) fn run() {
    unsupported_descriptions_fail();
    malformed_blobs_fail();
    properties_after_children_fail();
    nested_reserved_memory_fails();
    byte_mutations_do_not_panic();
    deep_nodes_fail();
}
