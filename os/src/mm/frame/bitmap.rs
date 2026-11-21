//! 空闲页框位图与非空位图字索引栈。

use alloc::vec::Vec;

use super::WORD_BITS;

pub(super) struct Bitmap {
    // 1 表示空闲，0 表示已分配；末尾不足一个机器字的无效高位恒置为 0。
    free: Vec<usize>,
    // 记录当前非零位图字下标的索引栈：每个非零字恰好在栈中出现一次，分配时直接取栈顶字。
    nonempty: Vec<usize>,
    available: usize,
}

impl Bitmap {
    /// 初始化全空闲位图，并预先分配好容纳全部位图字所需的索引栈空间。
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

    /// 从索引栈顶的非零位图字中分配一个空闲位；若该字就此耗尽则将其弹出索引栈。
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

    /// 归还先前由本位图分配出的页下标。
    pub(super) fn dealloc(&mut self, index: usize) {
        let word: usize = index / WORD_BITS;
        let mask = 1usize << (index % WORD_BITS);
        debug_assert_eq!(self.free[word] & mask, 0);
        if self.free[word] == 0 {
            // 仅当该位图字从全 0 恢复为非 0 时才压入索引栈，既避免栈中出现重复下标，
            // 又保证元素数量永不超过构造时预留的 `words` 容量，回收过程绝不触发堆重分配。
            self.nonempty.push(word);
        }
        self.free[word] |= mask;
        self.available += 1;
    }
}

#[cfg(feature = "ktest")]
#[path = "../../ktest/mm/frame/bitmap.rs"]
pub(super) mod tests;
