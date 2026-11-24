//! 内核堆分配器自检：验证全局堆的动态容器分配回收，以及独立测试池上的对齐、耗尽与伙伴块合并。

use alloc::{boxed::Box, vec::Vec};
use core::{alloc::Layout, ptr::NonNull};

use buddy_system_allocator::Heap;

use super::{ALLOCATOR, HEAP_ORDER, HEAP_SIZE};

#[repr(C, align(4096))]
struct AlignedValue([u8; 64]);

#[repr(C, align(8192))]
struct TestSpace([u8; 8192]);

static mut TEST_SPACE: TestSpace = TestSpace([0; 8192]);

fn assert_collections() {
    let before = ALLOCATOR.lock().stats_alloc_actual();
    {
        let mut boxed = Box::new(AlignedValue([0x5a; 64]));
        assert_eq!((&*boxed as *const AlignedValue as usize) % 4096, 0);
        boxed.0[63] = 0xa5;
        let mut values = Vec::new();
        for value in 0..2048usize {
            values.push(value ^ 0x1234);
        }
        assert_eq!(boxed.0[0], 0x5a);
        assert_eq!(boxed.0[63], 0xa5);
        drop(boxed);
        for (index, value) in values.iter().enumerate() {
            assert_eq!(*value, index ^ 0x1234);
        }
    }
    assert_eq!(ALLOCATOR.lock().stats_alloc_actual(), before);

    // 反复申请并释放相同容量的 `Vec`，验证堆空间可正常回收复用。
    for _ in 0..32 {
        let mut values = Vec::<u8>::new();
        values.try_reserve_exact(8192).unwrap();
        values.resize(8192, 0x3c);
        assert!(values.iter().all(|&value| value == 0x3c));
    }
    let mut too_large = Vec::<u8>::new();
    assert!(too_large.try_reserve_exact(HEAP_SIZE + 1).is_err());
    assert_eq!(ALLOCATOR.lock().stats_alloc_actual(), before);
}

fn assert_layouts(heap: &mut Heap<HEAP_ORDER>, start: usize) {
    let layouts = [
        Layout::from_size_align(1, 1).unwrap(),
        Layout::from_size_align(17, 8).unwrap(),
        Layout::from_size_align(100, 256).unwrap(),
        Layout::from_size_align(2049, 4096).unwrap(),
    ];
    let mut allocations: [Option<NonNull<u8>>; 4] = [None; 4];
    for (index, layout) in layouts.iter().enumerate() {
        let ptr = heap.alloc(*layout).unwrap();
        let addr = ptr.as_ptr() as usize;
        assert_eq!(addr % layout.align(), 0);
        assert!(addr >= start && addr + layout.size() <= start + 8192);
        for previous in 0..index {
            let other = allocations[previous].unwrap().as_ptr() as usize;
            assert!(
                addr + layout.size() <= other || other + layouts[previous].size() <= addr,
                "heap allocations overlap",
            );
        }
        // SAFETY: 块刚成功分配且未释放，仅在申请的 `layout.size()` 字节范围内填充测试数据。
        unsafe { ptr.as_ptr().write_bytes(index as u8 + 1, layout.size()) };
        allocations[index] = Some(ptr);
    }
    for (index, layout) in layouts.iter().enumerate() {
        let ptr = allocations[index].unwrap();
        // SAFETY: 该块仍由当前测试独占且已完成初始化，读取完毕后立即归还给测试堆。
        let bytes = unsafe { core::slice::from_raw_parts(ptr.as_ptr(), layout.size()) };
        assert!(bytes.iter().all(|&value| value == index as u8 + 1));
        heap.dealloc(ptr, *layout);
    }
    assert_eq!(heap.stats_alloc_actual(), 0);
    assert_eq!(heap.stats_alloc_user(), 0);
}

fn assert_exhaustion_and_merge(heap: &mut Heap<HEAP_ORDER>, start: usize) {
    let half = Layout::from_size_align(4096, 4096).unwrap();
    let whole = Layout::from_size_align(8192, 8192).unwrap();
    let first = heap.alloc(half).unwrap();
    let second = heap.alloc(half).unwrap();
    assert_ne!(first, second);
    assert!(heap.alloc(Layout::new::<usize>()).is_err());

    heap.dealloc(first, half);
    assert!(heap.alloc(whole).is_err());
    let reused = heap.alloc(half).unwrap();
    assert_eq!(reused, first);
    heap.dealloc(second, half);
    heap.dealloc(reused, half);

    let merged = heap.alloc(whole).unwrap();
    assert_eq!(merged.as_ptr() as usize, start);
    heap.dealloc(merged, whole);
    assert_eq!(heap.stats_alloc_actual(), 0);
    assert_eq!(heap.stats_total_bytes(), 8192);
}

/// 验证全局静态堆的容器分配，并在 8 KiB 独立测试池上验证多对齐分配、耗尽报错与伙伴块合并。
pub(crate) fn kernel_test() {
    assert_eq!(ALLOCATOR.lock().stats_total_bytes(), HEAP_SIZE);
    assert_collections();

    // SAFETY: 仅获取测试专用静态数组 `TEST_SPACE` 的起始地址。
    let start = unsafe { core::ptr::addr_of_mut!(TEST_SPACE.0) as *mut u8 as usize };
    let mut heap = Heap::<HEAP_ORDER>::empty();
    // SAFETY: `TEST_SPACE` 独立于全局堆且按 8192 字节对齐；仅在本函数内初始化一次并在返回前释放全部块。
    unsafe { heap.init(start, 8192) };
    assert_layouts(&mut heap, start);
    assert_exhaustion_and_merge(&mut heap, start);
    crate::println!("{}: heap kernel tests passed", crate::arch::NAME);
}
