//! 在栈上字节数组中测试 [`MappedFdt::from_raw`] 的边界检查与物理区间保留逻辑。

use super::{fdt, MappedFdt, Region};
use crate::ktest::support::fdt::basic;

#[repr(align(8))]
struct Storage([u8; 1024]);

fn mapped_blob_read() {
    let blob = basic().finish();
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;

    // SAFETY: `storage` 按 8 字节对齐且包含完整的测试 DTB，在 `source` 存活期间保持有效；
    // 合成物理地址 `0x100000` 仅用于验证保留区间记账，不会用于解引用。
    let source = unsafe { MappedFdt::from_raw(0x100000, mapped, storage.0.len()) }.unwrap();
    assert_eq!(source.bytes, blob.as_slice());
    assert_eq!(source.storage, Region::new(0x100000, blob.len()).unwrap());
    let memory = source.parse_memory().unwrap();
    assert!(memory.reserved.contains(&source.storage));
    assert_eq!(
        memory.ram,
        alloc::vec![Region::new(0x80000000, 0x40000000).unwrap()]
    );

    // SAFETY: 同一有效存储，验证容量恰好等于 `blob.len()` 时也能正常通过。
    assert!(unsafe { MappedFdt::from_raw(0x100000, mapped, blob.len()) }.is_ok());
}

fn storage_bounds() {
    let blob = basic().finish();
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;

    for capacity in [0, fdt::HEADER_SIZE - 1, blob.len() - 1] {
        // SAFETY: `mapped` 指向有效的局部数组；故意传入不足的 `capacity` 验证提前拦截。
        assert!(unsafe { MappedFdt::from_raw(0x100000, mapped, capacity) }.is_err());
    }
    for physical in [0x100001, usize::MAX - 7] {
        // SAFETY: `mapped` 指向有效的局部数组；故意传入未对齐或溢出的物理地址验证错误返回。
        assert!(unsafe { MappedFdt::from_raw(physical, mapped, blob.len()) }.is_err());
    }
}

fn malformed_structure_is_a_safe_parse_error() {
    let mut blob = basic().finish();
    let structure = u32::from_be_bytes(blob[8..12].try_into().unwrap()) as usize;
    blob[structure..structure + 4].copy_from_slice(&0u32.to_be_bytes());
    let mut storage = Storage([0; 1024]);
    storage.0[..blob.len()].copy_from_slice(&blob);
    let mapped = storage.0.as_ptr() as usize;

    // SAFETY: 头部与完整字节范围均有效可读；结构块内部 token 损坏应在后续纯安全的 `parse_memory` 中报错。
    let source = unsafe { MappedFdt::from_raw(0x100000, mapped, blob.len()) }.unwrap();
    assert!(source.parse_memory().is_err());
}

/// 验证 [`MappedFdt`] 的容量上限校验、物理区间保留以及“裸指针读取与安全解析分离”的行为。
pub fn kernel_test() {
    mapped_blob_read();
    storage_bounds();
    malformed_structure_is_a_safe_parse_error();
}
