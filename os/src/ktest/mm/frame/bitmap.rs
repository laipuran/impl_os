//! 页框位图与非空位图字索引栈的稀疏回收压测。

use super::{Bitmap, WORD_BITS};

fn sparse_recycling(words: usize) {
    let total = words * WORD_BITS;
    let mut bitmap = Bitmap::new(total).unwrap();
    let index_capacity = bitmap.nonempty.capacity();
    let bitmap_capacity = bitmap.free.capacity();

    for _ in 0..total {
        assert!(bitmap.alloc().unwrap() < total);
    }
    assert!(bitmap.alloc().is_none());
    assert!(bitmap.nonempty.is_empty());
    assert!(bitmap.free.iter().all(|word| *word == 0));

    for _ in 0..5000 {
        bitmap.dealloc(0);
        bitmap.dealloc(1);
        bitmap.dealloc(WORD_BITS);

        // 即使第 0 个字连续归还了两个页，索引栈中也只能压入一次字下标 `0`。
        assert_eq!(bitmap.available(), 3);
        assert_eq!(bitmap.nonempty.len(), 2);
        let mut indexed = [bitmap.nonempty[0], bitmap.nonempty[1]];
        indexed.sort_unstable();
        assert_eq!(indexed, [0, 1]);

        let mut allocated = [
            bitmap.alloc().unwrap(),
            bitmap.alloc().unwrap(),
            bitmap.alloc().unwrap(),
        ];
        allocated.sort_unstable();
        assert_eq!(allocated, [0, 1, WORD_BITS]);
        assert!(bitmap.alloc().is_none());
        assert_eq!(bitmap.available(), 0);
        assert!(bitmap.nonempty.is_empty());

        assert_eq!(bitmap.nonempty.capacity(), index_capacity);
        assert_eq!(bitmap.free.capacity(), bitmap_capacity);
    }

    for index in 0..total {
        bitmap.dealloc(index);
    }
    assert_eq!(bitmap.available(), total);
    assert_eq!(bitmap.nonempty.len(), words);
    assert!(bitmap.free.iter().all(|word| *word == usize::MAX));
    assert_eq!(bitmap.nonempty.capacity(), index_capacity);
}

/// 在 128 字与 8192 字两种规模的位图上验证耗尽后的多轮稀疏回收与重分配。
pub fn kernel_test() {
    sparse_recycling(128);
    sparse_recycling(8192);
}
