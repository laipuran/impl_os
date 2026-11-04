//! 通过 Rust 集合验证全局堆，并用独立堆验证耗尽与伙伴合并。

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

    // 同一容量反复申请和释放，不应持续消耗堆空间。
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
        // SAFETY: 分配成功且仍未释放；在申请长度内写入，不访问空闲块元数据。
        unsafe { ptr.as_ptr().write_bytes(index as u8 + 1, layout.size()) };
        allocations[index] = Some(ptr);
    }
    for (index, layout) in layouts.iter().enumerate() {
        let ptr = allocations[index].unwrap();
        // SAFETY: 当前块仍由本测试独占，整个申请区间已初始化。
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

/// 检查已初始化的全局堆，以及独立测试堆的布局、回收和耗尽行为。
///
/// 仅由关闭中断的 boot CPU 顺序执行一次；所有申请在返回前释放。
pub(crate) fn kernel_test() {
    assert_eq!(ALLOCATOR.lock().stats_total_bytes(), HEAP_SIZE);
    assert_collections();

    // SAFETY: 仅获取测试独占静态数组的裸地址，不创建引用或访问其内容。
    let start = unsafe { core::ptr::addr_of_mut!(TEST_SPACE.0) as *mut u8 as usize };
    let mut heap = Heap::<HEAP_ORDER>::empty();
    // SAFETY: 测试静态空间独立于全局堆且按 8192 字节对齐；本测试独占它，
    // 仅初始化一次，并在返回前释放所有块，不向外泄露指针或引用。
    unsafe { heap.init(start, 8192) };
    assert_layouts(&mut heap, start);
    assert_exhaustion_and_merge(&mut heap, start);
    crate::println!("{}: heap kernel tests passed", crate::arch::NAME);
}
