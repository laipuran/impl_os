# 开发约定

- 内核使用 Rust 2021、`no_std`、panic abort；通过 Makefile 指定裸机 target。
- 合并前运行 `just check` 并手动验证启动，提交前运行宿主侧 `just judge-build`。命令职责见[分工表](targets.md)，检查范围见[开发检查](../testing/README.md)。
- unsafe 必须说明地址、权限、生命周期、别名或中断状态等安全前提；不要用裸指针掩盖资源所有权设计。
- 硬件/架构代码留在边界；共享业务代码不遍布 `cfg(target_arch)`。
- 当前控制台只支持单启动 CPU；引入中断/多核输出前先解决同步与重入。
- 新增依赖前核对许可证、`no_std`/target 支持和工具链兼容性；版本由 `Cargo.lock` 锁定，依赖源码随仓库提交到 `vendor/`。更新流程见下文“更新离线依赖”，不依赖本地或评测镜像的 registry 缓存。
- `make all` 不能下载依赖、修改宿主环境、读取相邻仓库或依赖被忽略的 `artifacts/`。
- 修改环境或测试版本时，同步更新环境约束和检查说明。

## 更新离线依赖

新增或更新依赖后，在可联网的开发环境执行 `just vendor`，一起提交清单、`Cargo.lock` 和 `vendor/` 的变更。该命令按锁文件收集依赖源码，将各依赖的 `.cargo-checksum.json` 改为非隐藏的 `cargo-checksum.json`，再创建原名的相对符号链接。重复执行会复用未变化的版本目录；不要手动修改第三方源码或校验内容。更新前仍须核对依赖许可证和双架构支持。

Makefile 的 `prepare-vendor` 目标负责恢复被评测过滤的隐藏校验链接，`make all`、`just vendor` 和 `just lint` 共用该实现。该目标不会联网或更新依赖。链接不纳入版本控制；若原名已被普通文件或错误链接占用，准备步骤会报错而不覆盖。依赖源码目录必须可写，且更新 vendor 时不能并行构建。

Cargo 配置保存在非隐藏的 `cargo/config.toml`，构建和 Clippy 均通过 `--config cargo/config.toml` 显式加载；Clippy 必须将该选项放在 `clippy` 子命令之后。Cargo 按配置文件上两级目录解析相对路径，因此此处的 `directory = "vendor"` 指向仓库根目录的 `vendor/`，不能直接把配置移到根目录而保持路径不变。工具链、目标标准库和系统构建工具仍由开发或评测环境提供，不属于 vendor 的内容。

提交前运行宿主侧 `just judge-build`，验证隐藏文件被过滤、Cargo 缓存为空且网络关闭时仍能构建两个架构。

## 提交前清单

1. 干净构建生成两个 ELF，`readelf -h` 架构正确。
2. 验证隐藏文件/目录被过滤后仍可执行比赛构建入口。
3. 运行已实现的真实回归用例，保留原始失败信息。
4. 检查 `git status`，不要提交测试盘、构建产物、凭据或第三方二进制。
5. 阶段截止前按比赛要求公开仓库并提交纯仓库链接。
