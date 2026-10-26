# Rust 内核

`src/main.rs` 是共享 Rust 入口；`src/arch/` 提供两架构的入口汇编、链接布局、早期串口和停机操作。`build.rs` 按 target 选链接脚本，避免依赖可能被评测过滤的隐藏配置。

目前只编译 `arch`、`console`。其他子目录是职责预留，只有 README，**不代表对应模块已经实现**。实现前先明确拥有的状态、对外操作和同步方式，不为了目录对称而拆出空 crate。

在仓库根执行 `make all`，不在这里维护第二套 Makefile。依赖、模块边界和物理/虚拟地址约定见 `docs/architecture/`。
