# 构建 targets 与开发 recipes 的职责

## 原则

- **Make：产物契约。** 准备离线依赖校验链接，编译、导出、清理比赛 ELF；评测机只需 `make all`。
- **Just：开发命令。** 直接调用 Rust、Docker、QEMU 工具；构建委托 Make。
- 环境安装与构建分开；命令的检查范围见[开发检查](../testing/README.md)。

## Make targets

| Target | 职责 |
|---|---|
| `all`（默认） | 构建两个架构，输出根目录 `kernel-rv`、`kernel-la` |
| `prepare-vendor` | 从非隐藏校验文件恢复 Cargo 校验链接；两个架构共用此先决条件 |
| `kernel-rv` | Cargo 离线增量构建 RISC-V，导出 ELF |
| `kernel-la` | Cargo 离线增量构建 LoongArch，导出 ELF |
| `clean` | 删除 `target/` 和两个根目录 ELF，保留测试盘/日志 |

日常变量：`CARGO`、`MODE=release|debug`、`FEATURES`（默认空；全部内核自检统一使用 `ktest`）。`TARGET_DIR` 可由命令行覆盖，用于原始镜像中的独立缓存构建。所有目标为 phony，源码增量判断交给 Cargo；链接参数唯一来源为 `os/build.rs`。

调整支持架构或 Rust target 时，同步核对 `rust-toolchain.toml` 的目标列表、Makefile 的产物映射、Just 的 `setup` / `doctor` / `lint` / QEMU 命令，以及 `os/build.rs` 和架构实现。各入口保留各自职责，不另建一套构建逻辑。

## Just：容器内任务

| Recipe | 职责 |
|---|---|
| 默认 `just` | 列出任务，不构建、不安装 |
| `setup` | 显式联网准备固定 Rust、targets、开发组件；禁止 rustup 自更新 |
| `doctor` | 只读检查环境，缺工具即失败；不自动安装 |
| `build [all\|rv\|la] [release\|debug] [features]` | 默认构建双架构 release，按参数转发给对应 Make 目标 |
| `clean` | 转发给 `make clean` |
| `fmt` | 修改 Rust 源码格式 |
| `fmt-check` | 只检查格式 |
| `lint` | 两个裸机 target 的 Clippy，不导出根目录 ELF |
| `vendor` | 按锁文件更新依赖源码和校验链接，可联网；不属于构建前置步骤 |
| `check` | 组合 `fmt-check` 和 `lint` |
| `run <rv\|la> [QEMU 参数…]` | 构建所选架构的 release ELF，启动 QEMU |
| `test [rv\|la] [release\|debug]` | 默认 rv/release；构建启用 `ktest` 的 ELF，在 QEMU 中执行所选架构的全部已接入自检 |
| `debug <rv\|la> [QEMU 参数…]` | 构建所选架构的 debug ELF，QEMU 暂停并监听 GDB `:1234` |

运行时用环境变量 `MEM`、`SMP` 设置内存/核数，默认 1G/1。额外参数直接传给 QEMU，使用其原生选项。

## Just：宿主任务

| Recipe | 职责 |
| --- | --- |
| `image-pull` | 显式拉取固定 digest 的比赛原始镜像 |
| `image-build` | 构建 `impl-os-dev:20260510`：补充开发工具，并缓存 Linux 宿主 UID/GID；其他宿主使用 `1000:1000` |
| `container [命令…]` | 直接以镜像中的 `dev` 用户执行，不迁移所有权、不隐式 pull；仅终端交互时加 `-it` |
| `judge-build` | 原始镜像禁网，过滤隐藏文件后以空 Cargo 缓存和独立临时构建目录执行 `make all`；导出 ELF 使用 Linux 宿主 UID/GID，其他宿主使用 `1000:1000` |

基础镜像唯一来源是 `.devcontainer/Dockerfile` 的 `ARG CONTEST_IMAGE`，VS Code 和 Just 共用该 Dockerfile。

宿主示例：`just container just check`。不要在 Dev Container 中执行宿主 Docker 任务，容器不挂载 Docker socket。
