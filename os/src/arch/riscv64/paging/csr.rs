//! 单核、ASID 为零的作用域切换；不向调用者暴露可遗忘的激活守卫。

use core::arch::asm;

const SV39: usize = 8 << 60;

pub(super) struct RestoreSatp(usize);

impl RestoreSatp {
    /// 切换到 Sv39，并在守卫销毁时恢复原配置。
    ///
    /// # Safety
    ///
    /// 仅在关闭中断的 boot CPU 的 S-mode 调用。root 必须是有效的 Sv39
    /// 根表物理地址；新旧地址空间须保持执行代码、栈及所有使用中对象有效，
    /// 并保证守卫在原页表失效前销毁，不允许其他路径修改 satp。
    pub unsafe fn switch(root: usize) -> Self {
        let previous;
        // SAFETY: 调用者保证 S-mode、单核及切换前后仍可执行并访问当前栈。
        // 不使用 nomem：禁止编译器把页表写入移动到启用分页之后。
        unsafe {
            asm!(
                "csrrw {previous}, satp, {next}",
                "sfence.vma zero, zero",
                previous = out(reg) previous,
                next = in(reg) (SV39 | (root >> 12)),
                options(nostack),
            );
        }
        Self(previous)
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
