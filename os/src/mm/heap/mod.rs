//! 基于 BSS 静态缓冲区的全局伙伴系统堆（`buddy_system_allocator`）。

use buddy_system_allocator::LockedHeap;

#[cfg(feature = "ktest")]
#[path = "../../ktest/mm/heap/mod.rs"]
pub(crate) mod tests;

const HEAP_SIZE: usize = 1024 * 1024;
// 空闲链表阶数上限：阶数 0..=20 覆盖最大 2^20 = 1 MiB 的内存块。
const HEAP_ORDER: usize = HEAP_SIZE.ilog2() as usize + 1;

#[repr(C, align(4096))]
struct HeapSpace([u8; HEAP_SIZE]);

static mut HEAP_SPACE: HeapSpace = HeapSpace([0; HEAP_SIZE]);

// `LockedHeap` 内部仅使用普通自旋锁，不会自动关中断；当前仅供单核关中断环境使用。
#[global_allocator]
static ALLOCATOR: LockedHeap<HEAP_ORDER> = LockedHeap::empty();

/// 将 BSS 段中的 1 MiB 静态数组注册给全局堆分配器（仅允许在启动时调用一次）。
pub(super) fn init() {
    let mut allocator = ALLOCATOR.lock();
    assert_eq!(allocator.stats_total_bytes(), 0, "heap already initialized");
    // SAFETY: `HEAP_SPACE` 位于已清零的 BSS 段，与启动栈及其他静态变量互不重叠；
    // 本函数通过上方的 `assert_eq!` 保证仅初始化一次，此后该内存区间由 `ALLOCATOR` 独占管理。
    unsafe {
        let start = core::ptr::addr_of_mut!(HEAP_SPACE.0) as *mut u8 as usize;
        allocator.init(start, HEAP_SIZE);
    }
}
