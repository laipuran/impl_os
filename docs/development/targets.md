# Makefile 与 Justfile 任务对照表

## 职责划分原则

- **`Makefile`（比赛构建契约）**：负责恢复 `vendor/` 校验软链接、调用 Cargo 离线编译双架构内核并将 ELF 产物复制到仓库根目录。评测机只运行 `make all`，不依赖 `just` 或 Docker。
- **`justfile`（日常开发工作流）**：提供环境检查、代码格式化、Clippy 静态分析、QEMU 运行/调试、内核自检及宿主机容器管理等快捷命令；所有涉及内核编译的步骤均委托给 `Makefile` 执行。

---

## `Makefile` 目标一览

| Target | 说明 |
| --- | --- |
| `all`（默认） | 编译双架构内核，在仓库根目录生成 `kernel-rv` 与 `kernel-la` |
| `prepare-vendor` | 遍历 `vendor/*/cargo-checksum.json` 创建 `.cargo-checksum.json` 软链接 |
| `kernel-rv` | 离线编译 RISC-V 内核并复制到根目录 `kernel-rv` |
| `kernel-la` | 离线编译 LoongArch 内核并复制到根目录 `kernel-la` |
| `clean` | 执行 `cargo clean` 并删除根目录的 `kernel-rv`、`kernel-la` |

**支持覆盖的 Make 变量：**
- `CARGO`：Cargo 可执行文件路径（默认 `cargo`）。
- `MODE`：`release`（默认）或 `debug`。
- `FEATURES`：传给 Cargo `--features` 的特性列表（默认空；自检时传入 `ktest`）。
- `TARGET_DIR`：编译中间产物目录（默认 `$(CURDIR)/target`；`just judge-build` 会将其指向容器 `/tmp` 下的独立目录）。

---

## `justfile`：容器内开发命令

以下命令在**开发容器内**执行：

| 命令 | 说明 |
| --- | --- |
| `just` | 列出所有可用任务 |
| `just setup` | 联网安装固定版本的 Rust 工具链、双架构 target 及开发组件（禁止 rustup 自更新） |
| `just doctor` | 只读检查工具链、target、QEMU 与 GDB 是否齐备（缺失时直接报错，不自动联网安装） |
| `just build [all\|rv\|la] [release\|debug] [features]` | 转发给 `make` 编译指定架构与模式的内核（默认 `all release`） |
| `just clean` | 调用 `make clean` 清理编译产物 |
| `just fmt` | 运行 `cargo fmt --all` 格式化代码 |
| `just fmt-check` | 检查代码格式是否符合规范（不修改文件） |
| `just lint` | 对双架构的默认配置与 `ktest` 配置（共 4 个组合）运行 Clippy 静态检查（`-D warnings`） |
| `just check` | 依次执行 `fmt-check` 与 `lint` |
| `just vendor` | 根据 `Cargo.lock` 更新 `vendor/` 依赖源码并重命名校验文件（需联网） |
| `just run <rv\|la> [QEMU 参数…]` | 编译对应架构的 release 内核并在 QEMU 中启动 |
| `just test [rv\|la] [release\|debug]` | 编译开启 `ktest` 特性的内核并在 QEMU 中执行启动自检（默认 `rv release`） |
| `just debug <rv\|la> [QEMU 参数…]` | 编译对应架构的 debug 内核，启动 QEMU 并暂停在入口，监听 `:1234` 等待 GDB |

> **QEMU 资源配置：** 运行 `just run`、`just test` 或 `just debug` 时，可通过环境变量 `MEM`（默认 `1G`）和 `SMP`（默认 `1`）调整虚拟机内存与核数，例如 `MEM=768M just run la`。

---

## `justfile`：宿主机容器命令

以下命令在**宿主机**执行（切勿在开发容器内运行）：

| 命令 | 说明 |
| --- | --- |
| `just image-pull` | 拉取 `.devcontainer/Dockerfile` 中固定 SHA256 digest 的官方比赛镜像 |
| `just image-build` | 构建本地开发镜像 `impl-os-dev:20260510`（自动注入宿主机 UID/GID 与开发工具） |
| `just container [命令…]` | 以 `dev` 用户在开发容器中启动交互式终端，或单次执行传入的命令（如 `just container just check`） |
| `just judge-build` | 在未修改的官方比赛镜像中禁网（`--network none`）、过滤所有隐藏文件与构建残留，使用空 Cargo 缓存执行 `make all` 并将生成的 ELF 拷回工作区 |
