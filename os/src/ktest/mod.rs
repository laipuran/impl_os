//! 启动期内核白盒自检入口（仅在启用 `ktest` feature 时编译）。

pub(crate) mod support;
pub(crate) mod user_env;

use user_env::UserEnv;

use crate::mm::frame::FrameAllocator;

struct Suite {
    name: &'static str,
    run: fn(&FrameAllocator),
}

// 按顺序执行的内核自检套件列表。
const SUITES: &[Suite] = &[
    Suite {
        name: "user execution",
        run: |_| user_execution(),
    },
    Suite {
        name: "heap",
        run: |_| crate::mm::heap::tests::kernel_test(),
    },
    Suite {
        name: "physical regions",
        run: |_| crate::mm::regions::tests::kernel_test(),
    },
    Suite {
        name: "physical frames",
        run: crate::mm::frame::tests::kernel_test,
    },
    #[cfg(target_arch = "riscv64")]
    Suite {
        name: "Sv39 paging",
        run: crate::arch::paging::tests::kernel_test,
    },
    Suite {
        name: "FDT memory",
        run: |_| crate::mm::fdt::tests::kernel_test(),
    },
    Suite {
        name: "boot FDT mapping",
        run: |_| crate::arch::boot_data::tests::kernel_test(),
    },
    #[cfg(target_arch = "loongarch64")]
    Suite {
        name: "boot tables",
        run: |_| crate::arch::boot::tests::kernel_test(),
    },
];

// 在同一个 `UserEnv` 中顺序运行架构 Trap、系统调用 ABI 与任务生命周期三组用户态探针测试。
fn user_execution() {
    // SAFETY: 当前仅主核在关中断状态下执行自检，无其他并发的用户态切换。
    let mut env = unsafe { UserEnv::new() };
    crate::arch::trap::tests::kernel_test(&mut env);
    let stack = env.stack();
    // SAFETY: 测试探针与 4 KiB 测试栈均位于当前 `UserEnv` 可访问的 RAM 中；
    // 全程保持中断关闭，且前一组测试同步返回后才会复用同一个测试栈。
    unsafe {
        crate::syscall::tests::kernel_test(env.address(crate::syscall::tests::entry()), stack);
        crate::task::tests::kernel_test(env.address(crate::task::tests::entry()), stack);
    }
}

/// 在内核基础子系统初始化完成后，按 `SUITES` 顺序依次运行全部自检套件。
///
/// `frames` 为启动流程创建的物理页框分配器引用。每个测试套件返回前都必须恢复其临时修改的硬件状态；
/// 任何断言失败都会直接触发内核 panic 停机，只有全部通过才会打印最终的 `all kernel tests passed`。
pub fn run(frames: &FrameAllocator) {
    for suite in SUITES {
        crate::println!("{}: running {} tests", crate::arch::NAME, suite.name);
        (suite.run)(frames);
    }
    crate::println!("{}: all kernel tests passed", crate::arch::NAME);
}
