//! 单 CPU 启动测试共用的可信用户执行环境；不提供内存隔离。

#[cfg(target_arch = "loongarch64")]
use core::arch::asm;

#[repr(C, align(16))]
struct UserStack([u8; 4096]);

static mut USER_STACK: UserStack = UserStack([0; 4096]);

/// 独占测试栈及架构用户地址映射，正常离开作用域时恢复机器状态。
pub(crate) struct UserEnv {
    #[cfg(target_arch = "loongarch64")]
    previous_window: usize,
}

impl UserEnv {
    /// 创建独占用户执行环境。
    ///
    /// # Safety
    /// 仅在禁用中断的启动 CPU 上调用，且不得与其他环境或用户执行并发。
    pub(crate) unsafe fn new() -> Self {
        #[cfg(target_arch = "loongarch64")]
        {
            let previous_window: usize;
            // SAFETY: 单 CPU 测试独占 DMW1，内核运行在不变的 DMW0；
            // 临时窗口允许 PLV3 访问 RAM，并不提供隔离。
            unsafe {
                asm!(
                    "csrrd {saved}, 0x181",
                    "li.w {tmp}, 0x18",
                    "dbar 0",
                    "csrwr {tmp}, 0x181",
                    "invtlb 0x0, $zero, $zero",
                    "ibar 0",
                    saved = out(reg) previous_window,
                    tmp = out(reg) _,
                    options(nostack),
                );
            }
            Self { previous_window }
        }
        #[cfg(target_arch = "riscv64")]
        {
            // bare 地址空间依赖固件 PMP 允许用户访问 RAM，不提供隔离。
            Self {}
        }
    }

    pub(crate) fn address(&self, address: usize) -> usize {
        #[cfg(target_arch = "loongarch64")]
        {
            address - 0x9000_0000_0000_0000
        }
        #[cfg(target_arch = "riscv64")]
        {
            address
        }
    }

    pub(crate) fn stack(&self) -> usize {
        let top = core::ptr::addr_of_mut!(USER_STACK) as usize + core::mem::size_of::<UserStack>();
        self.address(top)
    }

    /// 撤销用户窗口，使下一次用户取指触发 refill。
    #[cfg(target_arch = "loongarch64")]
    pub(crate) fn revoke(&mut self) {
        // SAFETY: 测试独占 DMW1，内核及其栈仍由 DMW0 映射。
        unsafe {
            asm!(
                "dbar 0",
                "csrwr $zero, 0x181",
                "invtlb 0x0, $zero, $zero",
                "ibar 0",
                options(nostack),
            );
        }
    }

    /// 重建用户窗口以继续执行受控探针。
    #[cfg(target_arch = "loongarch64")]
    pub(crate) fn enable(&mut self) {
        // SAFETY: 测试独占 DMW1，受控探针与栈位于 RAM 中。
        unsafe {
            asm!(
                "li.w {tmp}, 0x18",
                "dbar 0",
                "csrwr {tmp}, 0x181",
                "invtlb 0x0, $zero, $zero",
                "ibar 0",
                tmp = out(reg) _,
                options(nostack),
            );
        }
    }
}

#[cfg(target_arch = "loongarch64")]
impl Drop for UserEnv {
    fn drop(&mut self) {
        // SAFETY: 用户执行已返回 DMW0，恢复进入测试前的窗口并同步翻译状态。
        unsafe {
            asm!(
                "dbar 0",
                "csrwr {saved}, 0x181",
                "invtlb 0x0, $zero, $zero",
                "ibar 0",
                saved = inout(reg) self.previous_window => _,
                options(nostack),
            );
        }
    }
}
