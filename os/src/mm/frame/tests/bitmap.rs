//! 验证非空字索引不随回收产生重复，稀疏回收无需扫描整个池。

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

        // 同一个字回收两页也只能有一个索引；无论池多大，候选都只有这两个字。
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

/// 检查两个规模的合成页池，在耗尽后反复回收及分配稀疏页面。
pub fn kernel_test() {
    sparse_recycling(128);
    sparse_recycling(8192);
}
