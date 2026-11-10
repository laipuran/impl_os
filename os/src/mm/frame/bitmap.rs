//! 空闲页位图及非空字索引；不访问物理地址，也不依赖地址区间布局。

use alloc::vec::Vec;

use super::WORD_BITS;

pub(super) struct Bitmap {
    // 置位表示空闲；尾部不对应物理页的位始终为零。
    free: Vec<usize>,
    // 每个非零位图字恰好出现一次，最后一个索引是下一次分配的候选字。
    nonempty: Vec<usize>,
    available: usize,
}

impl Bitmap {
    /// 建立全空闲位图，并一次性预留所有非空字索引槽。
    pub(super) fn new(total: usize) -> Result<Self, &'static str> {
        let words = total.div_ceil(WORD_BITS);
        let mut free = Vec::new();
        free.try_reserve_exact(words)
            .map_err(|_| "insufficient heap for frame bitmap")?;
        free.resize(words, usize::MAX);
        if total % WORD_BITS != 0 {
            free[words - 1] = (1usize << (total % WORD_BITS)) - 1;
        }

        let mut nonempty = Vec::new();
        nonempty
            .try_reserve_exact(words)
            .map_err(|_| "insufficient heap for frame word index")?;
        nonempty.extend(0..words);

        Ok(Self {
            free,
            nonempty,
            available: total,
        })
    }

    pub(super) fn available(&self) -> usize {
        self.available
    }

    /// 从索引栈顶的非空字取一页；字耗尽时移除其索引。
    pub(super) fn alloc(&mut self) -> Option<usize> {
        if self.available == 0 {
            return None;
        }

        let word = *self
            .nonempty
            .last()
            .expect("free frame count disagrees with word index");
        let bits = self.free[word];
        debug_assert_ne!(bits, 0);
        let bit = bits.trailing_zeros() as usize;

        self.free[word] &= !(1usize << bit);
        if self.free[word] == 0 {
            self.nonempty.pop();
        }
        self.available -= 1;

        Some(word * WORD_BITS + bit)
    }

    /// 归还此前从本位图分配且尚未回收的页索引。
    pub(super) fn dealloc(&mut self, index: usize) {
        let word: usize = index / WORD_BITS;
        let mask = 1usize << (index % WORD_BITS);
        debug_assert_eq!(self.free[word] & mask, 0);
        if self.free[word] == 0 {
            // 只有空字变为非空字时才入栈；每个字至多一个索引，预留容量足够，
            // 因而回收不触发堆分配，也不重复加入已经存在的索引。
            self.nonempty.push(word);
        }
        self.free[word] |= mask;
        self.available += 1;
    }
}

#[cfg(feature = "ktest")]
#[path = "tests/bitmap.rs"]
pub(super) mod tests;
