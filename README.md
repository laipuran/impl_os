# impl_os

面向 2026 OS 内核实现赛的 Rust 内核项目基架。借鉴相邻 `rCore-Tutorial-v3` 的开发组织方式，但不复制其代码，也不使用其旧版 QEMU/自带固件作为比赛环境。

**当前是启动基架，不是已通过比赛测试的操作系统。** 尚未实现用户态、系统调用、EXT4、VirtIO、调度或网络。两架构已在比赛指定原始镜像完成离线构建和串口冒烟；RISC-V 关机成功，LoongArch 关机未实现，详见[测试状态](docs/testing/README.md)。

## 开始开发

1. 宿主安装 Docker、VS Code 和 Dev Containers 扩展。
2. 打开本目录，执行 **Dev Containers: Reopen in Container**。以比赛镜像为基础，仅补充 just；首次基础镜像约 6 GB。不启用特权模式或挂载 Docker socket。
3. 在容器内执行：

```bash
just                 # 列出任务及职责
just doctor
just build           # 调用 make all，生成 ELF：kernel-rv、kernel-la
just run-rv          # 构建 → 启动 → 输出日志 → SBI 关机
just run-la          # 构建 → 启动 → 输出日志 → 停放 CPU（关机未实现）
just check           # 只检查格式和双架构 Clippy，不代表功能测试通过
```

`make all` 使用 Cargo 的 `--locked --offline`，不负责下载工具链；先完成[环境准备](docs/getting-started/environment.md)。普通的 `cargo build` 没有指定裸机 target，不是本项目的构建入口。

## 目录职责

| 路径 | 职责 |
|---|---|
| `.devcontainer/` | VS Code 开发容器，固定比赛镜像 tag + digest |
| `os/` | `no_std` Rust 内核；架构启动、链接脚本、通用代码 |
| `tests/` | 预留真实内核回归测试，目前只有说明 |
| `docs/` | 环境、架构、开发约定、测试、路线图与调研 |
| `artifacts/` | 被忽略的本地测试镜像、串口日志等大文件 |
| `target/` | Cargo 生成目录，不提交 |
| `Makefile` | 本地开发与比赛 `make all` 的统一入口 |
| `justfile` | 开发任务编排；构建委托 Make，不复制构建逻辑 |
| `rust-toolchain.toml` | 固定 Rust nightly、裸机 targets 与组件 |

命令职责见[任务分工表](docs/development/targets.md)，详细模块边界见[代码结构](docs/architecture/layout.md)，文档导航见 [docs/README.md](docs/README.md)。官方测试源码的研究见[工具链与测试调研](docs/competition-toolchain-and-tests.md)。

不用 VS Code 时，在**宿主**执行（需要 Docker、just）：

```bash
just image-pull       # 显式下载比赛基础镜像
just image-build      # 构建薄开发镜像
just container        # 进入开发终端
just container just check
just judge-build      # 原始比赛镜像，断网、独立 Cargo 缓存构建；不启动内核
```

## 提交约定

- 根目录 `make all` 生成 ELF，不以 raw binary 替代。
- 构建不依赖 `.cargo/` 或 `.vscode/` 等隐藏配置：链接参数来自 `os/build.rs`，因此隐藏目录被评测过滤后仍可构建。
- 当前没有第三方 Rust 依赖；未来引入依赖时必须提交锁文件及必要源码并验证离线构建。
- 比赛 `make all` 不依赖 just、Docker 或 Dev Container 配置。
- 开发命令直接写在 Justfile 中，没有项目自带的 Python 工具或工具单元测试。
- 仓库公开、HTTPS 提交、最终启动参数及评分规则仍须以比赛平台为准。本地 `just judge-build` 不是官方评分器。
