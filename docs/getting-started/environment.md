# 环境与 Dev Container

## 固定环境

- 基础镜像：`zhouzhouyi/os-contest:20260510`，Dev Container 固定 digest，避免 tag 漂移。
- 开发镜像：从上述原始镜像派生，仅安装校验 SHA256 的 just **1.40.0**；不替换 QEMU/GCC/固件。
- 开发语言：Rust；`nightly-2025-01-18`，edition 2021。
- 目标：`riscv64gc-unknown-none-elf`、`loongarch64-unknown-none`。
- 无第三方 crate；不需要 `cargo install` 或通过交叉 GCC 链接当前内核。

该镜像提供 Ubuntu **24.04.4**、QEMU **10.0.2** 和 RISC-V 默认固件 OpenSBI **1.5.1**。开发环境固定这套工具；最终比赛的启动参数和资源限制由比赛规则确定。

## 进入容器

1. Docker 可运行 Linux/amd64 容器；安装 VS Code Dev Containers 扩展。
2. 打开仓库，执行 **Reopen in Container**。
3. 初次下载镜像约 6 GB，留出镜像解包和构建空间。容器平台固定 Linux/amd64，ARM 宿主需要额外的宿主架构仿真。
4. 构建薄开发镜像时只安装 just，完成后自动执行 `just doctor`。doctor 只读检查，缺少必需开发工具时失败。

容器用 `root` 是因为官方工具链位于 `/root/.cargo` 和 `/root/.rustup`，不盲目替换用户以免丢失预装环境。在 Linux bind mount 上生成文件可能属于 root；不要对整个仓库递归改权限，必要时仅修正自己的生成文件。

不启用 `--privileged`、不挂载 Docker socket、不暴露 GDB 端口。

## 补齐 Rust（仅在 doctor 报缺失时）

在开发容器终端执行显式联网任务，不把它加入 `make all`：

```bash
just setup
just doctor
make all
```

`cargo`/`rustc` shim 会读取 `rust-toolchain.toml`，缺少指定工具链/组件时也可能自动联网。`--offline` 约束 Cargo，不是 rustup。配置文件只声明构建所需的组件/targets，不把 rustfmt/Clippy 变成评测构建的隐式安装要求。开发组件由 `just setup` 显式准备，并禁止 rustup 自更新。

## 无 VS Code 的等价入口（在宿主运行）

```bash
just image-pull
just image-build
just container
# 或一次执行容器内任务：
just container just check
# 使用未修改的比赛基础镜像，断网复核：
just judge-build
```

宿主需要 Docker、just 和常规 Shell 工具；容器内不需要 Docker。`container` 默认不自动拉取镜像，显式执行前两个任务。镜像唯一配置源是 `.devcontainer/Dockerfile` 的 `ARG CONTEST_IMAGE`，Just 与 VS Code 共用同一 Dockerfile。

镜像升级需要同步核对 QEMU、固件和 Rust target，并更新对应配置与环境说明。
