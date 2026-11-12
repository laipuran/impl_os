//! 启动期内核测试，仅在启用 `ktest` feature 时编译。

pub(crate) mod support;
pub(crate) mod user_env;

use user_env::UserEnv;

struct Suite {
    name: &'static str,
    run: fn(),
}

// 将测试套件选择与运行器及正常启动路径分离。
const SUITES: &[Suite] = &[
    Suite {
        name: "user execution",
        run: user_execution,
    },
    Suite {
        name: "heap",
        run: crate::mm::heap::tests::kernel_test,
    },
    Suite {
        name: "physical regions",
        run: crate::mm::regions::tests::kernel_test,
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
        run: crate::mm::fdt::tests::kernel_test,
    },
    Suite {
        name: "boot FDT mapping",
        run: crate::arch::boot_data::tests::kernel_test,
    },
    #[cfg(target_arch = "loongarch64")]
    Suite {
        name: "boot tables",
        run: crate::arch::boot::tests::kernel_test,
    },
];

// 所有用户探针顺序执行，独占栈与架构映射贯穿整个执行组。
fn user_execution() {
    // SAFETY: 仅启动 CPU 执行此组，禁用中断且没有其他用户执行。
    let mut env = unsafe { UserEnv::new() };
    crate::arch::trap::tests::kernel_test(&mut env);
    let stack = env.stack();
    // SAFETY: 固定探针和独占栈均处于当前可信用户环境中；中断关闭，
    // 各探针同步返回后才复用栈。RV 依赖 bare 模式下固件 PMP 允许访问 RAM；
    // LA 的临时窗口映射 RAM。这些机制均不提供用户隔离。
    unsafe {
        crate::syscall::tests::kernel_test(env.address(crate::syscall::tests::entry()), stack);
        crate::task::tests::kernel_test(env.address(crate::task::tests::entry()), stack);
    }
}

/// 正常初始化后，按声明顺序运行配置的测试套件。
///
/// 需要真实页框的套件通过 [`crate::mm::frames`] 借用全局分配器，不重复初始化。
/// 在禁用中断的 boot CPU 上调用。各套件必须在返回前恢复临时映射和机器状态；
/// 断言失败会通过内核 panic handler 停机，
/// 因此不会打印最终成功标记。
pub fn run() {
    for suite in SUITES {
        crate::println!("{}: running {} tests", crate::arch::NAME, suite.name);
        (suite.run)();
    }
    crate::println!("{}: all kernel tests passed", crate::arch::NAME);
}
