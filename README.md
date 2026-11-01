# impl_os

面向 OS 内核实现赛的 Rust `no_std` 内核，支持 RISC-V 和 LoongArch 两个构建目标，使用 QEMU `virt` 运行。

## 构建目标

| 架构 | Rust target | 输出文件 |
| --- | --- | --- |
| RISC-V 64 | `riscv64gc-unknown-none-elf` | `kernel-rv` |
| LoongArch 64 | `loongarch64-unknown-none` | `kernel-la` |

构建产物均为 ELF。Rust 工具链固定为 `nightly-2025-01-18`，配置见 `rust-toolchain.toml`。

## 开始开发

1. 宿主安装 Docker、VS Code 和 Dev Containers 扩展。
2. 打开本目录，执行 **Dev Containers: Reopen in Container**。开发容器基于固定的比赛镜像，补充 just、配套的 rust-analyzer 和标准库源码。
3. 在容器内执行：

```bash
just                 # 列出任务及职责
just doctor
just build           # 调用 make all，生成 ELF：kernel-rv、kernel-la
just run rv          # 构建并在 QEMU 中运行 RISC-V 内核
just run la          # 构建并在 QEMU 中运行 LoongArch 内核
just debug rv        # 构建 debug 内核，等待 GDB 连接 :1234
just check           # 检查格式和双架构 Clippy，不执行启动测试
```

RISC-V 启动后通过 SBI 请求关机；LoongArch 启动后停放 CPU，QEMU 不会自动退出，可按 `Ctrl-a x` 退出。

`make all` 使用 Cargo 的 `--locked --offline`，需要预先准备固定工具链、目标标准库和 `Cargo.lock` 中依赖的本地缓存。环境配置见[环境准备](docs/getting-started/environment.md)。普通的 `cargo build` 没有指定裸机 target，不是本项目的构建入口。

## 目录职责

| 路径 | 职责 |
|---|---|
| `.devcontainer/` | VS Code 开发容器，固定比赛镜像 tag + digest |
| `os/` | `no_std` Rust 内核、构建脚本与架构链接脚本 |
| `docs/` | 架构、环境、开发命令和检查说明 |
| `artifacts/` | 被忽略的本地测试镜像、串口日志等大文件 |
| `target/` | Cargo 生成目录，不提交 |
| `Makefile` | 本地开发与比赛 `make all` 的统一入口 |
| `justfile` | 开发任务编排；构建委托 Make，不复制构建逻辑 |
| `rust-toolchain.toml` | 固定 Rust nightly、裸机 targets 与组件 |

命令职责见[任务分工表](docs/development/targets.md)，模块边界见[代码结构](docs/architecture/layout.md)。完整导航见 [docs/README.md](docs/README.md)。

## 不使用 VS Code

在**宿主**执行（需要 Docker、just）：

```bash
just image-pull       # 显式下载比赛基础镜像
just image-build      # 构建薄开发镜像
just container        # 进入开发终端
just container just check
just judge-build      # 原始比赛镜像，断网、独立构建目录；不启动内核
```

`judge-build` 使用基础镜像中的 Cargo 缓存，不下载依赖；缓存缺失时离线构建会失败。

## 构建与提交约定

- 根目录 `make all` 生成 ELF，不以 raw binary 替代。
- 构建不依赖 `.cargo/` 或 `.vscode/` 等隐藏配置：链接参数来自 `os/build.rs`，因此隐藏目录被评测过滤后仍可构建。
- Rust 依赖由 `os/Cargo.toml` 声明，版本由仓库内的 `Cargo.lock` 锁定；`log` 禁用默认特性以用于 `no_std` 环境。
- 比赛 `make all` 不依赖 just、Docker 或 Dev Container 配置。
- 开发命令直接写在 `justfile` 中，构建逻辑集中在 `Makefile`。
- 仓库公开、HTTPS 提交、最终启动参数及评分规则以比赛平台为准。
