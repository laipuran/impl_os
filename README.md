# impl_os

面向全国大学生计算机系统能力大赛操作系统设计赛（内核实现赛）的 Rust `no_std` 内核，支持 RISC-V 64 与 LoongArch 64 双架构，运行于 QEMU `virt` 平台。

## 构建目标

| 架构 | Rust Target | 产物路径 |
| --- | --- | --- |
| RISC-V 64 | `riscv64gc-unknown-none-elf` | `kernel-rv` |
| LoongArch 64 | `loongarch64-unknown-none` | `kernel-la` |

构建产物均为 ELF 格式。编译器工具链固定为 `nightly-2025-01-18`（见 [`rust-toolchain.toml`](rust-toolchain.toml)）。

## 快速上手

推荐使用 VS Code Dev Containers 开发。开发容器基于官方比赛镜像构建，已预装 `just`、`rust-analyzer` 与标准库源码：

1. 在宿主机安装 Docker、VS Code 及 Dev Containers 扩展。
2. 用 VS Code 打开仓库根目录，执行 **Dev Containers: Reopen in Container**。
3. 在容器终端中执行常用命令：

```bash
just                 # 查看所有可用的开发任务
just doctor          # 检查工具链与模拟器环境是否就绪
just build           # 调用 make all 编译双架构内核，生成 kernel-rv 和 kernel-la
just run rv          # 编译并在 QEMU 中启动 RISC-V 内核
just run la          # 编译并在 QEMU 中启动 LoongArch 内核
just debug rv        # 以 debug 模式启动 QEMU，在 :1234 端口等待 GDB 连接
just check           # 运行 rustfmt 格式检查与双架构 Clippy 静态检查
just test            # 运行内核启动自检（默认 rv release，可传 la 或 la debug）
```

> **提示：** RISC-V 内核运行结束后会通过 SBI 请求关机并自动退出 QEMU；LoongArch 暂未实现关机接口，内核在末尾会让 CPU 进入 `idle` 循环，可按 `Ctrl-a x` 退出 QEMU。

### 不使用 VS Code（命令行容器工作流）

若不使用 VS Code，可在**宿主机**通过 `just` 和 Docker 直接操作容器：

```bash
just image-pull                  # 拉取固定版本的官方比赛基础镜像
just image-build                 # 构建本地开发镜像
just container                   # 启动并进入开发容器交互终端
just container just check        # 在容器内单次执行指定命令
just judge-build                 # 模拟评测机环境：使用原始比赛镜像断网编译双架构内核
```

## 目录结构

| 路径 | 说明 |
| --- | --- |
| `os/` | 内核主 crate，包含共享内核逻辑、架构相关汇编与链接脚本 |
| `docs/` | 环境配置、架构设计、构建任务分工与测试文档（见 [文档导航](docs/README.md)） |
| `cargo/config.toml` | Cargo 离线构建配置，将 crates.io 源重定向至仓库内的 `vendor/` |
| `vendor/` | 随仓库提交的第三方依赖源码，供评测环境离线编译 |
| `.devcontainer/` | 开发容器配置与 Dockerfile |
| `artifacts/` | 本地测试镜像、日志等大文件存放目录（Git 忽略） |
| `Makefile` | 比赛评测入口（`make all`）与底层编译逻辑 |
| `justfile` | 日常开发命令封装（构建任务统一转发给 `Makefile`） |
| `rust-toolchain.toml` | 固定的 Rust 工具链版本、编译目标与必装组件 |

## 构建与评测约定

- **构建入口：** 评测机固定在仓库根目录执行 `make all`。直接运行不带 `--target` 的 `cargo build` 无法编译本内核。
- **离线构建：** `make all` 使用 `--locked --offline` 并读取 `vendor/` 下的依赖源码，编译过程无需联网。如果新增或升级了依赖，需在开发环境运行 `just vendor` 并一并提交 `Cargo.toml`、`Cargo.lock` 和 `vendor/`（详见 [依赖维护说明](docs/development/README.md#维护离线依赖)）。
- **不依赖隐藏目录：** 评测平台可能过滤 `.cargo/`、`.vscode/` 等隐藏目录，因此链接脚本与编译参数统一由 `os/build.rs` 注入，Cargo 配置放在非隐藏的 `cargo/config.toml` 中。
