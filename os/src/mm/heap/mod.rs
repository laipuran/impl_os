//! 基于静态 BSS 空间的 buddy 堆，不依赖页框分配或用户地址空间。

use buddy_system_allocator::LockedHeap;

#[cfg(feature = "ktest")]
pub(crate) mod tests;

const HEAP_SIZE: usize = 1024 * 1024;
// 参数是空闲链表数量，索引 0..=20 对应最多 1 MiB 的块。
const HEAP_ORDER: usize = HEAP_SIZE.ilog2() as usize + 1;

#[repr(C, align(4096))]
struct HeapSpace([u8; HEAP_SIZE]);

static mut HEAP_SPACE: HeapSpace = HeapSpace([0; HEAP_SIZE]);

// 锁只串行化分配器状态，不会关闭中断；目前仅支持禁用中断的 boot CPU。
#[global_allocator]
static ALLOCATOR: LockedHeap<HEAP_ORDER> = LockedHeap::empty();

/// 将静态堆空间交给分配器；调用约束与 [`super::init`] 相同。
pub(super) fn init() {
    let mut allocator = ALLOCATOR.lock();
    assert_eq!(allocator.stats_total_bytes(), 0, "heap already initialized");
    // SAFETY: BSS 已清零；该静态数组在整个内核运行期间有效且可写，
    // 不与启动栈或其他对象重叠。初始化仅执行一次，此后只有分配器访问它。
    unsafe {
        let start = core::ptr::addr_of_mut!(HEAP_SPACE.0) as *mut u8 as usize;
        allocator.init(start, HEAP_SIZE);
    }
}
