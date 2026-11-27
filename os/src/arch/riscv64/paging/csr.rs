//! 单核、ASID 为零的作用域切换；不向调用者暴露可遗忘的激活守卫。

use core::arch::asm;
use riscv::register::satp::{self, Mode, Satp};

fn encode_sv39(root: usize) -> usize {
    let mut value = Satp::from_bits(0);
    value.set_mode(Mode::Sv39);
    value.set_ppn(root >> 12);
    value.bits()
}

/// 在 S-mode 读取当前完整的 `satp` 值。
pub(super) fn satp() -> usize {
    satp::read().bits()
}

pub(super) struct RestoreSatp(usize);

impl RestoreSatp {
    /// 切换到 Sv39 并验证生效；失败时立即恢复原配置。
    ///
    /// 成功时由返回的守卫在作用域结束后恢复配置。
    ///
    /// # Safety
    ///
    /// - 仅在关闭中断的启动主核 S-mode 调用。
    /// - `root` 必须是按 4 KiB 对齐、落在 56 位物理地址范围内的有效 Sv39 根页表物理地址。
    /// - 新旧地址空间须保持执行代码、栈及所有使用中对象有效。
    /// - 必须保证守卫按后进先出顺序在原页表失效前销毁；允许通过嵌套调用本函数切换并恢复 `satp`，但不允许绕过该守卫机制直接修改 `satp`。
    pub(super) unsafe fn switch(root: usize) -> Option<Self> {
        // SAFETY: 本函数与 `switch_checked` 的安全前提相同。
        unsafe { Self::switch_checked(root, encode_sv39(root)) }
    }

    /// 切换到 `root` 对应的 Sv39 配置，并将硬件回读值与 `expected` 比较。
    ///
    /// # Safety
    ///
    /// 调用方须满足 [`Self::switch`] 的全部安全前提，无论 `expected` 为何值。
    pub(super) unsafe fn switch_checked(root: usize, expected: usize) -> Option<Self> {
        let next = encode_sv39(root);
        let previous;
        let actual: usize;
        // SAFETY: 调用者保证 S-mode、单核及切换前后仍可执行并访问当前栈。
        // 不使用 nomem：禁止编译器将页表写入移到切换之后。
        // 切换前排序 PTE 写入，让切换后的首次取指可见；切换后清除旧 ASID 0 的缓存项。
        unsafe {
            asm!(
                "sfence.vma zero, zero",
                "csrrw {previous}, satp, {next}",
                "sfence.vma zero, zero",
                "csrr {actual}, satp",
                previous = out(reg) previous,
                next = in(reg) next,
                actual = lateout(reg) actual,
                options(nostack),
            );
        }
        let restore = Self(previous);
        if actual != expected {
            // drop 会写回原值并再次刷新 TLB，即使硬件忽略了切换。
            return None;
        }
        Some(restore)
    }
}

impl Drop for RestoreSatp {
    fn drop(&mut self) {
        // SAFETY: switch 的调用者保证原地址空间和返回现场仍然有效；
        // 全量失效避免 ASID=0 的临时地址翻译泄漏到原地址空间。
        unsafe {
            asm!(
                "csrw satp, {previous}",
                "sfence.vma zero, zero",
                previous = in(reg) self.0,
                options(nostack),
            );
        }
    }
}
