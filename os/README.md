# Rust 内核

`src/main.rs` 是共享 Rust 入口；`src/arch/` 提供两架构的入口汇编、链接布局、trap 上下文及特权级切换、早期串口和停机操作。`build.rs` 按 target 选链接脚本，避免依赖可能被评测过滤的隐藏配置。

当前执行流程为 BSS 清零、日志与 trap 初始化、启动输出、调用架构停机入口。`src/trap.rs` 定义共享内核异常策略；启用 `ktest` 时，由 `src/ktest/mod.rs` 的统一入口按具名测试组列表执行各模块自检，目前覆盖内核异常和用户态往返。测试代码放在所属模块的 `tests/` 子目录；基础能力的初始化不依赖自检。接口约束及未实现的内存隔离见 `docs/architecture/boot.md`。

在仓库根执行 `make all`，不在这里维护第二套 Makefile。依赖、模块边界和物理/虚拟地址约定见 `docs/architecture/`。
