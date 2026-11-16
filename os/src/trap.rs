//! 异常入口初始化与内核态 Trap 处理。
//!
//! 用户态触发的异常和系统调用会直接从 [`crate::arch::trap::run_user`] 返回给调用方，
//! 不会进入本模块的 [`handle_kernel`]。

use crate::arch::trap::{self, TrapContext};

/// 初始化当前主核的异常向量与 Trap 控制寄存器。
pub fn init() {
    trap::init();
}

/// 处理内核态执行期间触发的异常或中断。
///
/// 当前内核全程保持中断关闭，且不具备可恢复的内核缺页处理机制；
/// 除 `ktest` 注册的已知测试探针外，任何内核态异常都会打印异常原因、PC、状态/原因 CSR、故障地址与 SP 等诊断信息并 panic 停机。
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
