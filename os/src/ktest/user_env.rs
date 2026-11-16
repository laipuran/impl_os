//! 供启动期自检运行受控用户态探针的临时执行环境（在接入用户页表前不提供内存隔离）。

#[cfg(target_arch = "loongarch64")]
use core::arch::asm;

#[repr(C, align(16))]
struct UserStack([u8; 4096]);

static mut USER_STACK: UserStack = UserStack([0; 4096]);

/// 管理 4 KiB 独占测试栈与临时用户态映射窗口的 RAII 守卫，离开作用域时恢复原机器状态。
pub(crate) struct UserEnv {
    #[cfg(target_arch = "loongarch64")]
    previous_window: usize,
}

impl UserEnv {
    /// 建立单核测试专用的用户态执行环境。
    ///
    /// # Safety
    ///
    /// 仅可在关闭中断的启动主核上调用，且存活期间不得与任何其他 `UserEnv` 或用户态执行并发。
    pub(crate) unsafe fn new() -> Self {
        #[cfg(target_arch = "loongarch64")]
        {
            let previous_window: usize;
            // SAFETY: 单核测试独占 `DMW1`（CSR `0x181`），内核自身运行在不变的 `DMW0` 窗口；
            // 此处临时设置 `DMW1 = 0x18`（PLV3 可缓存恒等映射），供 PLV3 测试探针访问低地址 RAM。
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
            // RISC-V 当前处于 `satp = Bare` 物理直通模式，直接借助 OpenSBI 默认 PMP 允许 U-mode 访问 RAM。
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

    /// 临时清零 `DMW1` 并刷新 TLB，使下一次进入 PLV3 取指时触发 TLB 重填异常。
    #[cfg(target_arch = "loongarch64")]
    pub(crate) fn revoke(&mut self) {
        // SAFETY: 测试独占 `DMW1`，内核代码与栈仍由 `DMW0` 正常映射。
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

    /// 重新开启 `DMW1` 的 PLV3 恒等映射并刷新 TLB。
    #[cfg(target_arch = "loongarch64")]
    pub(crate) fn enable(&mut self) {
        // SAFETY: 测试独占 `DMW1`，测试探针与测试栈均位于低地址 RAM 中。
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
        // SAFETY: 用户态测试探针已全部返回 PLV0，此处将 `DMW1` 恢复为进入测试前的原值并刷新 TLB。
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
