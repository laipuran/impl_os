# 开发规范与离线依赖维护

## 编码与架构约定

- **语言与编译配置**：内核使用 Rust 2021 Edition，处于 `#![no_std]`、`#![no_main]` 环境，panic 策略为 `abort`。始终通过 `Makefile` 或 `just` 指定裸机 target 编译。
- **架构解耦**：与特定 ISA、CSR、汇编或启动表相关的实现统一收敛在 `os/src/arch/` 下，共享内核模块通过 `crate::arch` 调用，避免在通用代码中散落 `#[cfg(target_arch = ...)]`。
- **`unsafe` 规范**：所有 `unsafe fn` 的文档注释需包含 `# Safety` 章节；所有 `unsafe` 块需附上具体的 `// SAFETY:` 注释，说明当前上下文如何满足地址有效性、对齐、生命周期、独占访问及关中断等前提。
- **单核假设**：当前控制台与分配器均针对“单核、关中断”阶段设计。后续引入外设中断或多核支持前，需先补齐关中断自旋锁与重入保护。
- **评测构建纯净性**：`make all` 必须能够在断网、无隐藏目录（`.cargo/` 等被过滤）、无外部缓存的环境中独立完成，不得访问外部网络、相邻目录或被 Git 忽略的 `artifacts/`。

## 维护离线依赖（`vendor/`）

由于比赛评测环境完全断网，且评测平台在拷贝源码时可能会过滤以 `.` 开头的隐藏文件（如 Cargo 默认生成的 `.cargo-checksum.json`），仓库采用以下机制维护 `vendor/`：

1. **添加或更新依赖**：
   - 先确认新依赖支持 `no_std`、兼容 `riscv64` 与 `loongarch64` 且许可证合规。
   - 修改 `os/Cargo.toml` 后，先在可联网的开发容器中按变更范围更新并审阅 `Cargo.lock`（例如执行 `cargo update -p <crate>` 或 `cargo metadata`），随后执行：
     ```bash
     just vendor
     ```
   - `just vendor` 会运行 `cargo vendor --locked --versioned-dirs vendor`（注意 `--locked` 仅按现有 `Cargo.lock` 同步源码，不会自动升级锁文件；若 `Cargo.lock` 与 `Cargo.toml` 不一致会直接报错），并将每个依赖包下的隐藏文件 `.cargo-checksum.json` 重命名为非隐藏的 `cargo-checksum.json`，最后调用 `make prepare-vendor` 在本地建立同名软链接 `.cargo-checksum.json -> cargo-checksum.json`。
   - 执行 `just vendor` 时 `vendor/` 目录必须可写，且更新期间请勿并发运行任何读取 `vendor/` 的编译、Clippy 或其他 vendor 更新任务。
   - 将更新后的 `os/Cargo.toml`、`Cargo.lock` 以及 `vendor/` 目录一并提交到 Git（符号链接 `.cargo-checksum.json` 被 `.gitignore` 忽略，无需提交）。

2. **构建时自动恢复校验链接**：
   - `Makefile` 中的 `prepare-vendor` 目标会在编译前遍历 `vendor/*/cargo-checksum.json`，自动补齐 `.cargo-checksum.json` 软链接。这样即使评测机过滤了所有隐藏文件，`make all` 依然能通过 Cargo 的 vendor 完整性校验。
   - Cargo 配置文件存放在非隐藏路径 [`cargo/config.toml`](../../cargo/config.toml)，编译和 Clippy 均通过 `--config cargo/config.toml` 显式指定（注意 Cargo 会相对于配置文件所在目录的上两级解析相对路径，因此请勿随意移动 `cargo/config.toml` 的位置）。

## 合并与提交前检查清单

1. **静态检查与自检**：在开发容器内执行 `just check`、`just test rv` 和 `just test la`，确认无格式问题、无 Clippy 警告且双架构自检全部通过。
2. **评测构建验证**：在宿主机执行 `just judge-build`，确认在断网、清空 Cargo 缓存、过滤所有隐藏文件与目录的原始比赛镜像中，仍能成功生成 `kernel-rv` 与 `kernel-la`；随后在容器内执行 `readelf -h kernel-rv kernel-la`，核验两个产物均为 `ELF64`，且 `Machine` 分别为 `RISC-V` 与 `LoongArch`。
3. **仓库卫生检查**：检查 `git status` 与 `git diff --staged`，确保未误暂存测试镜像、临时日志、根目录编译出的 ELF 二进制或本地工具配置。
