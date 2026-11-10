//! 在局部数组上验证原始启动存储读取，不解引用合成物理地址。

use super::{fdt, MappedFdt, Region};

#[repr(align(8))]
struct Storage([u8; 1024]);

fn mapped_blob_read() {
    let blob = fdt::tests::memory_blob();
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;

    // SAFETY: 对齐的局部数组包含完整且不变的 FDT，在映射句柄存活期间有效；
    // 物理地址仅作为合成的占用元数据，不用于实际页框分配或解引用。
    let source = unsafe { MappedFdt::from_raw(0x100000, mapped, storage.0.len()) }.unwrap();
    assert_eq!(source.bytes, blob.as_slice());
    assert_eq!(source.storage, Region::new(0x100000, blob.len()).unwrap());
    let memory = source.parse_memory().unwrap();
    assert!(memory.reserved.contains(&source.storage));
    assert_eq!(
        memory.ram,
        alloc::vec![Region::new(0x80000000, 0x40000000).unwrap()]
    );

    // SAFETY: 同一有效存储；恰好容纳 FDT 的容量也应被接受。
    assert!(unsafe { MappedFdt::from_raw(0x100000, mapped, blob.len()) }.is_ok());
}

fn storage_bounds() {
    let blob = fdt::tests::memory_blob();
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;

    for capacity in [0, fdt::HEADER_SIZE - 1, blob.len() - 1] {
        // SAFETY: 头部和完整 FDT 均来自有效数组；缩小容量只会使读取提前失败。
        assert!(unsafe { MappedFdt::from_raw(0x100000, mapped, capacity) }.is_err());
    }
    for physical in [0x100001, usize::MAX - 7] {
        // SAFETY: 映射仍指向同一有效数组；合成物理地址仅用于检查对齐和溢出。
        assert!(unsafe { MappedFdt::from_raw(physical, mapped, blob.len()) }.is_err());
    }
}

fn malformed_structure_is_a_safe_parse_error() {
    let mut blob = fdt::tests::memory_blob();
    let structure = u32::from_be_bytes(blob[8..12].try_into().unwrap()) as usize;
    blob[structure..structure + 4].copy_from_slice(&0u32.to_be_bytes());
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;

    // SAFETY: 头部和描述的完整字节范围均有效且不变；结构 token 损坏不影响
    // 存储可读性，由后续安全的 parse_memory 拒绝，而不是裸地址读取器处理。
    let source = unsafe { MappedFdt::from_raw(0x100000, mapped, blob.len()) }.unwrap();
    assert!(source.parse_memory().is_err());
}

/// 验证映射存储、容量边界、物理占用及读取与解析的职责分离。
pub fn kernel_test() {
    mapped_blob_read();
    storage_bounds();
    malformed_structure_is_a_safe_parse_error();
}
