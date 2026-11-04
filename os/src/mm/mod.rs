//! 内核内存管理，目前提供固定大小的内核堆。

pub(crate) mod heap;

/// 初始化内核内存管理，必须早于首次堆分配。
///
/// 仅在关闭中断的 boot CPU 上调用，不支持并发分配或中断上下文中的分配。
///
/// # Panics
///
/// 重复初始化时 panic。
pub fn init() {
    heap::init();
}
