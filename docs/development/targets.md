# 构建 targets 与开发 recipes 的职责

## 原则

- **Make：产物契约。** 只编译、导出、清理比赛 ELF；评测机只需 `make all`。
- **Just：开发命令。** 直接调用 Rust、Docker、QEMU 工具；构建委托 Make。
- 环境安装与构建分开；命令的检查范围见[开发检查](../testing/README.md)。

## Make：共四个 target

| Target | 职责 |
|---|---|
| `all`（默认） | 构建两个架构，输出根目录 `kernel-rv`、`kernel-la` |
| `kernel-rv` | Cargo 离线增量构建 RISC-V，导出 ELF |
| `kernel-la` | Cargo 离线增量构建 LoongArch，导出 ELF |
| `clean` | 删除 `target/` 和两个根目录 ELF，保留测试盘/日志 |

日常变量：`CARGO`、`MODE=release|debug`。`TARGET_DIR` 可由命令行覆盖，用于原始镜像中的独立缓存构建。所有目标为 phony，源码增量判断交给 Cargo；链接参数唯一来源为 `os/build.rs`。

## Just：容器内任务

| Recipe | 职责 |
|---|---|
| 默认 `just` | 列出任务，不构建、不安装 |
| `setup` | 显式联网准备固定 Rust、targets、开发组件；禁止 rustup 自更新 |
| `doctor` | 只读检查环境，缺工具即失败；不自动安装 |
| `build [MODE=debug]` | 转发给 `make all` |
| `build-rv [release\|debug]` | 转发给 `make kernel-rv MODE=...` |
| `build-la [release\|debug]` | 转发给 `make kernel-la MODE=...` |
| `clean` | 转发给 `make clean` |
| `fmt` | 修改 Rust 源码格式 |
| `fmt-check` | 只检查格式 |
| `lint` | 两个裸机 target 的 Clippy，不导出根目录 ELF |
| `check` | 组合 `fmt-check` 和 `lint` |
| `run-rv [QEMU 参数…]` | 构建 release ELF，直接启动 RISC-V QEMU |
| `run-la [QEMU 参数…]` | 同上，LoongArch；关机未实现 |
| `debug-rv [QEMU 参数…]` | 构建 debug ELF，QEMU 暂停并监听 GDB `:1234` |
| `debug-la [QEMU 参数…]` | 同上，LoongArch |

运行时用环境变量 `MEM`、`SMP` 设置内存/核数，默认 1G/1。额外参数直接传给 QEMU，使用其原生选项。

## Just：宿主任务

| Recipe | 职责 |
|---|---|
| `image-pull` | 显式拉取固定 digest 的比赛原始镜像 |
| `image-build` | 构建 `impl-os-dev:20260510`：原始镜像只补充固定版本 just |
| `container [命令…]` | 进入开发容器或执行命令，不隐式 pull；仅终端交互时加 `-it` |
| `judge-build` | 原始镜像禁网，使用容器临时 Cargo 缓存执行 `make all`，输出两个 ELF 到工作区 |

基础镜像唯一来源是 `.devcontainer/Dockerfile` 的 `ARG CONTEST_IMAGE`，VS Code 和 Just 共用该 Dockerfile。

宿主示例：`just container just check`。不要在 Dev Container 中执行宿主 Docker 任务，容器不挂载 Docker socket。
