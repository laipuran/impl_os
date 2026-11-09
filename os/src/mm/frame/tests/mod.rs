//! 验证合成区间记账与真实启动分配器的回收，不解引用地址或耗尽真实物理内存。

use alloc::{vec, vec::Vec};

use super::{FrameAllocator, PAGE_SIZE, WORD_BITS};
use crate::mm::regions::{MemoryMap, Region};

fn allocator(ram: Vec<Region>, reserved: Vec<Region>) -> FrameAllocator {
    let map = MemoryMap::new(ram, reserved, PAGE_SIZE).unwrap();
    // SAFETY: 该分配器仅用于独立的合成地址记账；测试不访问物理内存，
    // 不将句柄交给真实内存使用者，因而不与实际 RAM 所有者发生访问冲突。
    unsafe { FrameAllocator::new(map) }.unwrap()
}

fn region(start_page: usize, end_page: usize) -> Region {
    Region {
        start: start_page * PAGE_SIZE,
        end: end_page * PAGE_SIZE,
    }
}

fn disjoint_regions_and_recycling() {
    let frames = allocator(vec![region(0, 4), region(100, 103)], vec![region(1, 3)]);
    assert_eq!(frames.total(), 5);
    let mut allocated = Vec::new();
    for page in [0, 3, 100, 101, 102] {
        let frame = frames.alloc().unwrap();
        assert_eq!(frame.start_address(), page * PAGE_SIZE);
        allocated.push(frame);
    }
    assert_eq!(frames.available(), 0);
    assert!(frames.alloc().is_none());
    let released = allocated.swap_remove(2);
    let address = released.start_address();
    drop(released);
    assert_eq!(frames.available(), 1);
    let reused = frames.alloc().unwrap();
    assert_eq!(reused.start_address(), address);
    assert!(frames.alloc().is_none());
    drop(reused);
    drop(allocated);
    assert_eq!(frames.available(), frames.total());
}

fn bitmap_boundaries() {
    for total in [
        1,
        WORD_BITS - 1,
        WORD_BITS,
        WORD_BITS + 1,
        WORD_BITS * 2 + 3,
    ] {
        let frames = allocator(vec![region(7, 7 + total)], vec![]);
        for _ in 0..3 {
            let allocated: Vec<_> = (0..total).map(|_| frames.alloc().unwrap()).collect();
            let mut addresses: Vec<_> = allocated.iter().map(|f| f.start_address()).collect();
            addresses.sort_unstable();
            assert_eq!(
                addresses,
                (7..7 + total).map(|p| p * PAGE_SIZE).collect::<Vec<_>>()
            );
            assert_eq!(frames.available(), 0);
            assert!(frames.alloc().is_none());
            drop(allocated);
            assert_eq!(frames.available(), total);
        }
    }
}

fn exhaustive_reservations() {
    for mask in 0..256usize {
        let reserved = (0..8)
            .filter(|page| mask & (1 << page) != 0)
            .map(|page| region(page, page + 1))
            .collect();
        let frames = allocator(vec![region(0, 8)], reserved);
        let expected = 8 - mask.count_ones() as usize;
        let allocated: Vec<_> = (0..expected).map(|_| frames.alloc().unwrap()).collect();
        let mut seen = 0;
        for frame in &allocated {
            let page = frame.start_address() / PAGE_SIZE;
            assert!(page < 8);
            assert_eq!(mask & (1 << page), 0);
            assert_eq!(seen & (1 << page), 0);
            seen |= 1 << page;
        }
        assert_eq!(seen | mask, 255);
        assert!(frames.alloc().is_none());
        drop(allocated);
        assert_eq!(frames.available(), expected);
    }
}

fn empty_usable() {
    for frames in [
        allocator(vec![region(0, 8)], vec![region(0, 8)]),
        allocator(
            vec![Region {
                start: 1,
                end: PAGE_SIZE,
            }],
            vec![],
        ),
    ] {
        assert_eq!(frames.total(), 0);
        assert_eq!(frames.available(), 0);
        for _ in 0..3 {
            assert!(frames.alloc().is_none());
            assert_eq!(frames.available(), 0);
        }
    }
}

fn invalid_maps() {
    let invalid = [
        vec![Region {
            start: 1,
            end: PAGE_SIZE,
        }],
        vec![Region {
            start: 0,
            end: PAGE_SIZE + 1,
        }],
        vec![Region {
            start: 0,
            end: usize::MAX & !(PAGE_SIZE - 1),
        }],
    ];
    for ram in invalid {
        // 用一字节页构造合法地图，覆盖与分配器页大小不兼容的情况。
        let map = MemoryMap::new(ram, vec![], 1).unwrap();
        // SAFETY: 仅检查合成区间的校验和元数据分配失败，不访问任何物理地址。
        assert!(unsafe { FrameAllocator::new(map) }.is_err());
    }
}

fn boot_allocator(frames: &FrameAllocator) {
    let available = frames.available();
    {
        let Some(frame) = frames.alloc() else {
            assert_eq!(available, 0);
            return;
        };
        assert_eq!(frames.available(), available - 1);
        let address = frame.start_address();
        assert_eq!(address % PAGE_SIZE, 0);
        assert!(frames
            .regions
            .iter()
            .any(|region| { region.start <= address && address < region.end }));
    }
    assert_eq!(frames.available(), available, "boot frame was not recycled");
}

/// 验证真实启动页框的分配回收，以及合成区间的耗尽、边界和保留组合。
pub fn kernel_test(frames: &FrameAllocator) {
    boot_allocator(frames);
    disjoint_regions_and_recycling();
    bitmap_boundaries();
    empty_usable();
    exhaustive_reservations();
    invalid_maps();
    crate::println!("{}: physical frame kernel tests passed", crate::arch::NAME);
}
