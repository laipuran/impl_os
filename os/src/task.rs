//! 单核、关闭中断的用户任务执行循环。
//!
//! 任务独占整数现场，不分配内核栈，也不拥有或切换地址空间。调用者先准备
//! 用户代码、栈和当前映射，再执行到退出或未处理的用户陷阱；不提供抢占或阻塞。

use crate::arch::trap::{run_user, TrapContext};
use crate::syscall::{self, Outcome};

/// 任务不可再次进入用户态的终止结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// 正常退出；保留 Linux 可观察的低八位退出码。
    Exited(u8),
    /// 未处理的用户陷阱或 syscall 无法完成；诊断信息保留在任务现场中。
    Faulted,
}

/// 同步执行期间的任务生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 已拥有初始现场，尚未执行。
    Ready,
    /// 正在用户态执行或在内核中处理该任务的 syscall。
    Running,
    /// 已终止，不能恢复执行。
    Finished(ExitStatus),
}

/// 拥有一个用户整数现场及其生命周期的最小任务。
///
/// 不持有用户内存；调用者须保证执行期间代码、栈及所需数据的生命周期。
pub struct Task {
    context: TrapContext,
    state: TaskState,
}

#[allow(dead_code)] // ELF 装载器接入前，由 ktest 提供已准备好的用户程序。
impl Task {
    /// 接收初始用户现场，创建尚未执行的任务；不检查地址或激活映射。
    pub fn new(context: TrapContext) -> Self {
        Self {
            context,
            state: TaskState::Ready,
        }
    }

    /// 返回当前生命周期状态。
    pub fn state(&self) -> TaskState {
        self.state
    }

    /// 借用初始现场或最后一次用户陷阱现场，包括退出和故障处的 PC。
    pub fn context(&self) -> &TrapContext {
        &self.context
    }

    /// 同步执行到退出、未处理的用户陷阱或 syscall 无法完成，并保存终止结果。
    ///
    /// 正常返回的 syscall 写回结果并推进 PC，然后重新进入用户态；退出和
    /// 故障不修改最后的现场。PC 推进溢出时按故障终止，不恢复该现场。
    /// 已结束任务直接返回原结果，不再次执行用户代码。
    /// 不保证执行时间有界：用户必须主动退出或触发可终止的陷阱。
    ///
    /// # Safety
    /// 本核须已初始化 trap，保持单 CPU、关闭中断且没有嵌套用户切换。
    /// 整个调用期间须保持该任务的用户映射有效且处于激活状态，并满足
    /// [`run_user`] 对代码、栈、数据及 ABI 对齐的安全要求。正常 syscall
    /// 返回后的继续执行也须满足这些要求。本接口不提供内存隔离；当前裸地址
    /// 环境只能执行不会覆盖内核或其他任务内存的可信用户代码。
    pub unsafe fn run(&mut self) -> ExitStatus {
        if let TaskState::Finished(status) = self.state {
            return status;
        }
        self.state = TaskState::Running;
        let status = loop {
            // SAFETY: 调用者保证整个执行期间的用户映射、私有栈和代码有效，
            // 并独占已初始化且关闭中断的本核；现场由此任务独占。
            unsafe { run_user(&mut self.context) };
            let Some(call) = syscall::decode(&self.context) else {
                break ExitStatus::Faulted;
            };
            match syscall::dispatch(call) {
                Outcome::Return(result) => {
                    // 每次陷阱仅完成一次；PC 无法推进时保留现场并终止任务。
                    if !syscall::complete(&mut self.context, result) {
                        break ExitStatus::Faulted;
                    }
                }
                Outcome::Exit(code) => break ExitStatus::Exited(code),
            }
        };
        self.state = TaskState::Finished(status);
        status
    }
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
