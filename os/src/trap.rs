//! Trap 初始化与内核致命异常处理策略。
//!
//! 用户异常从架构的 `run_user` 返回，并保留保存的 frame；
//! 不进入此内核异常处理函数。

use crate::arch::trap::{self, TrapContext};

/// 在进入用户代码前安装 boot CPU 的异常入口。
pub fn init() {
    trap::init();
}

/// 处理执行内核代码时发生的异常。
///
/// 中断保持禁用。目前尚无 scheduler 或可恢复的内核异常策略；
/// 意外 trap 会报告保存的机器状态，并通过 panic 停机。
pub fn handle_kernel(context: &mut TrapContext) {
    #[cfg(feature = "ktest")]
    if trap::tests::handle_kernel_test(context) {
        return;
    }

    panic!(
        "kernel trap: {}: pc={:#x}, status={:#x}, raw_cause={:#x}, bad_addr={:#x}, sp={:#x}",
        context.cause(),
        context.pc(),
        context.raw_status(),
        context.raw_cause(),
        context.bad_addr(),
        context.sp(),
    );
}
