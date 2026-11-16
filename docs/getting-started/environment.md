# 开发环境与 Dev Container 配置

## 基础环境与工具链规格

- **官方基础镜像**：`zhouzhouyi/os-contest:20260510`（Dockerfile 中通过 SHA256 digest 锁定版本，防止标签漂移）。镜像内置 Ubuntu 24.04.4、QEMU 10.0.2 以及 RISC-V 默认固件 OpenSBI 1.5.1。
- **本地开发镜像**：基于上述官方镜像构建（`impl-os-dev:20260510`），仅额外安装固定版本的 `just`（1.40.0，校验 SHA256）以及当前 Rust 工具链对应的 `rust-analyzer` 和 `rust-src`，不改动比赛镜像自带的编译器、QEMU 或固件。
- **Rust 工具链**：`nightly-2025-01-18`（Edition 2021），裸机编译目标为 `riscv64gc-unknown-none-elf` 与 `loongarch64-unknown-none`。
- **第三方依赖**：当前使用 `buddy_system_allocator`、`log` 和 `thiserror`（均关闭默认特性以适配 `no_std`）。所有依赖版本由 `Cargo.lock` 锁定，源码已提交在 `vendor/` 目录下，首次克隆仓库即可直接离线编译。

## 方式一：使用 VS Code Dev Containers（推荐）

1. 确保宿主机已安装 Docker（支持运行 `linux/amd64` 容器）及 VS Code 的 **Dev Containers** 扩展。
2. 用 VS Code 打开仓库根目录，执行 **Dev Containers: Reopen in Container**。
   - 首次拉取和解压基础镜像约需 6 GB 磁盘空间；若宿主机为 Apple Silicon 等 ARM 架构，Docker 会通过 `linux/amd64` 仿真运行。
3. 容器启动后会自动执行 `just doctor` 检查工具链与模拟器是否就绪。

> **关于语言服务器：** 容器内配置了 `/home/dev/.cargo/bin/rust-analyzer`，通过 `rustup` 调用与 `nightly-2025-01-18` 配套的语言服务器，避免 VS Code 扩展自动下载的新版 `rust-analyzer` 与旧版 nightly 标准库产生兼容问题。`rust-analyzer` 与 `rust-src` 仅在构建开发镜像时安装，不写入 [`rust-toolchain.toml`](../../rust-toolchain.toml)，以免影响比赛评测构建。

## 方式二：在宿主机使用命令行容器

如果不使用 VS Code，可在**宿主机**通过 `just` 直接管理开发容器（Windows 建议使用 Git Bash，[`justfile`](../../justfile) 已自动禁用 MSYS 路径转换）：

```bash
just image-pull                  # 拉取固定 digest 的官方比赛镜像
just image-build                 # 构建本地开发镜像 impl-os-dev:20260510
just container                   # 启动并进入容器交互式 bash
just container just check        # 在容器内单次执行命令
just judge-build                 # 使用未修改的官方镜像断网编译，验证评测构建契约
```

## 容器用户与文件权限

开发容器默认以普通用户 `dev` 运行：
- **Linux 宿主机**：VS Code Dev Containers 会在容器创建时自动映射宿主机的 UID/GID；命令行工作流则在执行 `just image-build` 时将当前宿主机的 UID/GID 写入开发镜像。若更换了 Linux 宿主机账号，请重新运行 `just image-build`。
- **Windows / macOS 宿主机**：统一使用默认 UID/GID `1000:1000`。
- `just judge-build` 在官方原始镜像中以 `root` 编译完成后，会自动将导出的 `kernel-rv` 和 `kernel-la` 属主修正为上述 UID/GID，确保后续开发容器能够正常覆盖。

如果早期的容器在工作区留下了 `root` 属主的文件，可先关闭容器，再在 Linux 宿主机仓库根目录排查并修复：

```bash
find "$PWD" -xdev -uid 0 -print
# 确认列出的文件均为旧容器生成后，修改属主：
sudo find "$PWD" -xdev -uid 0 -exec chown --no-dereference "$(id -u):$(id -g)" {} +
```

## 手动补装 Rust 组件

若在自定义环境中运行 `just doctor` 提示缺少 Rust 组件，可在**开发容器内**联网执行：

```bash
just setup
just doctor
make all
```

`just setup` 会显式安装 [`rust-toolchain.toml`](../../rust-toolchain.toml) 指定的工具链、双架构 target 以及 `rustfmt`、`clippy`、`llvm-tools-preview`、`rust-analyzer`、`rust-src`。注意 Cargo 的 `--offline` 仅禁止下载 crate 依赖，不会阻止 `rustup` 尝试联网安装缺失的工具链，因此评测环境测试（`just judge-build`）会同时设置 `RUSTUP_AUTO_INSTALL=0`。
