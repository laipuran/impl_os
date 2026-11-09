//! 与裸机自检共享的纯区间测试。

use alloc::vec;

use super::{MemoryMap, Region};

fn region(start: usize, end: usize) -> Region {
    Region { start, end }
}

#[cfg_attr(test, test)]
fn merge_and_subtract() {
    let map = MemoryMap::new(
        vec![
            region(32, 64),
            region(0, 20),
            region(16, 32),
            region(80, 96),
        ],
        vec![
            region(12, 24),
            region(8, 16),
            region(40, 44),
            region(60, 88),
        ],
        4,
    )
    .unwrap();
    assert_eq!(map.ram(), &[region(0, 64), region(80, 96)]);
    assert_eq!(
        map.reserved(),
        &[region(8, 24), region(40, 44), region(60, 88)]
    );
    assert_eq!(
        map.usable(),
        &[region(0, 8), region(24, 40), region(44, 60), region(88, 96)]
    );
}

#[cfg_attr(test, test)]
fn alignment_and_errors() {
    let map = MemoryMap::new(vec![region(1, 33)], vec![region(9, 10)], 4).unwrap();
    assert_eq!(map.usable(), &[region(4, 8), region(12, 32)]);
    assert!(MemoryMap::new(vec![], vec![], 4).is_err());
    assert!(MemoryMap::new(vec![region(0, 16)], vec![], 3).is_err());
    assert!(MemoryMap::new(vec![region(4, 4)], vec![], 4).is_err());
    assert!(MemoryMap::new(vec![region(8, 4)], vec![], 4).is_err());
    assert!(MemoryMap::new(vec![region(0, 16)], vec![region(8, 4)], 4).is_err());
    assert!(Region::new(usize::MAX - 3, 8).is_err());
    assert!(Region::new(0, 0).is_err());
}

#[cfg_attr(test, test)]
fn empty_usable_is_valid() {
    for (ram, reserved) in [
        (vec![region(1, 3)], vec![]),
        (vec![region(0, 16)], vec![region(0, 16)]),
        (vec![region(usize::MAX - 2, usize::MAX)], vec![]),
    ] {
        let map = MemoryMap::new(ram.clone(), reserved.clone(), 4).unwrap();
        assert_eq!(map.ram(), ram);
        assert_eq!(map.reserved(), reserved);
        assert!(map.usable().is_empty());
    }
}

#[cfg_attr(test, test)]
fn exhaustive_reservations() {
    for mask in 0..256 {
        let mut reserved = vec![];
        for bit in 0..8 {
            if mask & (1 << bit) != 0 {
                reserved.push(region(bit * 4 + 1, bit * 4 + 2));
            }
        }
        let map = MemoryMap::new(vec![region(0, 32)], reserved, 4).unwrap();
        for page in 0..8 {
            let available = map
                .usable()
                .iter()
                .any(|r| r.start <= page * 4 && page * 4 + 4 <= r.end);
            assert_eq!(available, mask & (1 << page) == 0);
        }
    }
}

/// 验证区间合并、扣除、对齐、空可用区及非法布局的错误路径。
pub fn kernel_test() {
    merge_and_subtract();
    alignment_and_errors();
    empty_usable_is_valid();
    exhaustive_reservations();
}
