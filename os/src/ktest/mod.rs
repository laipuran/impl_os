//! 启动期内核测试，仅在启用 `ktest` feature 时编译。

struct Suite {
    name: &'static str,
    run: fn(),
}

// 将测试套件选择与运行器及正常启动路径分离。
const SUITES: &[Suite] = &[Suite {
    name: "trap",
    run: crate::arch::trap::tests::kernel_test,
}];

/// 正常初始化后，按声明顺序运行配置的测试套件。
///
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
