//! 为分页自检构造由真实页框支撑的有限分配池。

use alloc::vec::Vec;

use crate::arch::user_paging::is_user_inactive;
use crate::mm::{
    frame::{FrameAllocator, PAGE_SIZE},
    regions::{MemoryMap, Region},
};

/// 从外层分配器借出 `count` 个真实页框，在 Bare 下运行自检并检查页框是否全部归还。
pub(crate) fn with_small_pool(
    frames: &FrameAllocator,
    count: usize,
    run: impl FnOnce(&FrameAllocator),
) {
    assert!(is_user_inactive());
    let backing: Vec<_> = (0..count).map(|_| frames.alloc().unwrap()).collect();
    let mut ram: Vec<_> = backing
        .iter()
        .map(|frame| Region {
            start: frame.start_address(),
            end: frame.start_address() + PAGE_SIZE,
        })
        .collect();
    let reserved = if ram.is_empty() {
        // 空池使用一段完全保留的占位区间，不分配或访问该地址。
        ram.push(Region {
            start: 0,
            end: PAGE_SIZE,
        });
        ram.clone()
    } else {
        Vec::new()
    };
    let pool = FrameAllocator::new(MemoryMap::new(ram, reserved, PAGE_SIZE).unwrap()).unwrap();
    // 外层句柄在小池之后销毁，借出期间只有小池分配这些页框。
    run(&pool);
    assert!(is_user_inactive());
    assert_eq!(pool.available(), count);
}
