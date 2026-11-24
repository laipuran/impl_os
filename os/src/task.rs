//! 用户任务启动数据准备与单核关中断环境下的最小执行循环。
//!
//! [`Task`] 仅封装一个用户态整数上下文（[`TrapContext`]）及其运行状态机，
//! 暂不持有独立的内核栈或用户地址空间。调用方需先准备好用户代码与栈映射，
//! 再调用 [`Task::run`] 同步执行用户程序，直到任务调用 `exit` 或触发未处理异常。
//!
//! [`elf`] 和 [`user_stack`] 分别解析用户程序镜像、构造初始用户栈，不建立映射；
//! RISC-V 上由 `loader` 串联解析、初始栈构造、用户地址空间映射与任务运行。

pub(crate) mod elf;
#[cfg(target_arch = "riscv64")]
pub(crate) mod loader;
pub(crate) mod user_stack;

use crate::arch::trap::{run_user, TrapContext};
use crate::syscall::{self, Outcome};

/// 用户任务的最终终止原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// 通过 `exit` 或 `exit_group` 正常退出，保存低 8 位退出码。
    Exited(u8),
    /// 触发了未处理的用户态异常，或系统调用返回时 PC 推进溢出。
    Faulted,
}

/// 用户任务的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 已完成初始化，尚未进入用户态执行。
    Ready,
    /// 正在用户态运行，或正在内核中处理其系统调用。
    Running,
    /// 已终止，不可再次进入用户态。
    Finished(ExitStatus),
}

/// 持有单个用户态整数上下文的最小任务控制结构。
///
/// `Task` 本身不管理虚拟内存生命周期；调用方必须保证任务运行期间其代码、栈与数据映射持续有效。
pub struct Task {
    context: TrapContext,
    state: TaskState,
}

impl Task {
    /// 使用给定的初始用户态上下文创建处于 [`TaskState::Ready`] 状态的任务。
    #[cfg_attr(
        all(target_arch = "loongarch64", not(feature = "ktest")),
        expect(dead_code, reason = "LoongArch 正常启动尚不创建用户任务")
    )]
    pub fn new(context: TrapContext) -> Self {
        Self {
            context,
            state: TaskState::Ready,
        }
    }

    /// 返回任务的当前生命周期状态。
    #[cfg(feature = "ktest")]
    pub fn state(&self) -> TaskState {
        self.state
    }

    /// 返回任务当前的寄存器与异常上下文引用（含退出或故障时刻的现场）。
    #[cfg(feature = "ktest")]
    pub fn context(&self) -> &TrapContext {
        &self.context
    }

    /// 同步运行用户任务，直到其主动退出或因异常终止。
    ///
    /// 循环进入用户态并拦截 Trap：
    /// - 若为普通系统调用，写回返回值并将 PC 加 4 后继续进入用户态；
    /// - 若为 `exit` / `exit_group` 或触发非系统调用异常，则保留当前现场并将状态置为 [`TaskState::Finished`]。
    ///
    /// 对已经处于 [`TaskState::Finished`] 状态的任务再次调用本方法会直接返回原退出结果，不会重复进入用户态。
    ///
    /// # Safety
    ///
    /// - 调用时当前 CPU 必须已完成 [`crate::trap::init`] 初始化，保持单核、关中断且无嵌套的 `run_user` 调用。
    /// - 调用期间必须保持该任务对应的用户代码、栈及数据映射有效且处于激活状态，并满足 [`run_user`] 的对齐与内存隔离要求。
    /// - 裸地址模式或用户可见的直接映射仅可用于不会修改内核存储的可信探针。
    #[cfg_attr(
        all(target_arch = "loongarch64", not(feature = "ktest")),
        expect(dead_code, reason = "LoongArch 正常启动尚不运行用户任务")
    )]
    pub unsafe fn run(&mut self) -> ExitStatus {
        if let TaskState::Finished(status) = self.state {
            return status;
        }
        self.state = TaskState::Running;
        let status = loop {
            // SAFETY: 调用者保证单核关中断环境及用户代码、栈映射有效，且 `self.context` 由当前任务独占。
            unsafe { run_user(&mut self.context) };
            let Some(call) = syscall::decode(&self.context) else {
                break ExitStatus::Faulted;
            };
            match syscall::dispatch(call) {
                Outcome::Return(result) => {
                    // 若 PC + 4 溢出则保留原现场并按故障终止任务，不再恢复执行。
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
#[path = "ktest/task.rs"]
pub(crate) mod tests;
