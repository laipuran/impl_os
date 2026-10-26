# 比赛容器实测记录

日期：2026-10-26。本仓库当时尚无 Git 提交，验证的是当前基架工作树。此次确实拉取并运行镜像，不是只阅读 Dockerfile。

## 原始镜像与工具版本

```text
zhouzhouyi/os-contest:20260510
sha256:85dec949df7cef41fd03d30c6ad69f952204540e18d2c62bced9d2e262fef12d
平台：linux/amd64
```

| 项目 | 原始镜像内实测 |
|---|---|
| OS | Ubuntu 24.04.4 LTS |
| QEMU RISC-V / LoongArch | 10.0.2 |
| RISC-V 默认固件 | OpenSBI 1.5.1 |
| Rust 默认工具链 | nightly-2025-01-18 |
| rustc | 1.86.0-nightly (6067b3631 2025-01-17) |
| Cargo | 1.86.0-nightly (088d49608 2025-01-10) |
| Make | 4.3 |
| GNU readelf | 2.42 |
| GDB | 15.1 |

两个正式裸机 target 已预装，rustfmt、Clippy、llvm-tools 也已存在。原始 Cargo 配置使用旧文件名 `/root/.cargo/config`，会产生弃用警告，但没有导致构建失败；未修改官方配置来掩盖它。

**版本差异：截图写 QEMU 9.2.1，而指定镜像实际是 10.0.2。** 当前开发跟随明确指定的镜像 digest；不能据此断言最终评测机一定使用哪个版本，仍应向官方确认。

### 交叉 GCC 实测

当前纯 Rust 基架不通过这些 GCC 链接，但它们影响以后编译用户程序/测试集。

| 命令 | 版本 | 实际路径 |
|---|---|---|
| `riscv64-linux-gnu-gcc` | 13.3.0 | `/usr/bin/riscv64-linux-gnu-gcc` |
| `riscv64-unknown-elf-gcc` | 13.2.0 | `/usr/bin/riscv64-unknown-elf-gcc` |
| `riscv64-buildroot-linux-musl-gcc` | 13.2.0 | `/opt/riscv64-lp64d--musl--bleeding-edge-2024.02-1/bin/` |
| `riscv64-linux-musl-gcc` | 11.2.1 20211120 | `/opt/riscv64-linux-musl-cross/bin/` |
| `loongarch64-linux-gnu-gcc` | 13.2.0 | `/opt/gcc-13.2.0-loongarch64-linux-gnu/bin/` |
| `loongarch64-linux-musl-gcc` | 13.2.0 | `/opt/loongarch64-linux-musl-cross/bin/` |

裸机 GCC 实际命中 apt 版本，而不是 Kendryte 8.2.0；这说明仅看配方中 COPY 的工具链会误判。旧测试 Makefile 的 Buildroot 路径风险仍存在，本次没有构建完整官方测试集。

## 原始镜像断网验证

以下是删除 Python 工具前的**历史实测**，不是当前命令使用说明。原 `judge-check` 已删除，当前只有 `just judge-build`，只验证构建。历史验证流程为：

1. 使用未修改的比赛镜像，Docker `--network none`，设置 `RUSTUP_AUTO_INSTALL=0`。
2. 将源码复制到容器临时目录，过滤所有隐藏文件/目录、旧 target/ELF、测试盘和 artifacts。
3. 执行 `make all`，Cargo 使用 `--locked --offline`；不调用 just，不执行安装脚本。
4. 用 readelf 查看两 ELF 的架构、入口和加载段。
5. 用该镜像内 QEMU 启动两个刚生成的 ELF，默认 1 GiB 内存、1 CPU；无测试盘。

结果：

```text
riscv64: PASS: boot and poweroff
loongarch64: PASS: boot; UNSUPPORTED: poweroff (expected park, terminated by harness)
```

LoongArch 的预期停放通过了**启动冒烟**检查，但不代表满足比赛主动关机要求。原始日志在 `artifacts/logs/judge/smoke-*.log`。

## 开发镜像验证

开发镜像 `impl-os-dev:20260510` 共用 VS Code 的 `.devcontainer/Dockerfile`，当前基于上述 digest **只添加 just 1.40.0**，校验发布方 SHA256，不替换 QEMU/GCC/固件。原始镜像本身已预装所需 Rust 开发组件。

当前可执行的开发检查为：

```bash
just image-build
just container just doctor
just container just check
just judge-build
```

历史上通过的 16 项单元测试仅覆盖现已删除的 Python 工具；这些测试也已删除，不再作为当前项目测试能力。Docker CLI 已实际验证，VS Code UI 尚未实际启动；二者共用同一 Dockerfile。

## 本次没有证明的内容

- 没有运行官方用户态测试，也没有评分。
- 没有验证磁盘驱动、EXT4、用户进程、系统调用或网络栈。
- 没有验证 SMP、多种内存配置或 LoongArch 正常关机。
- 历史兼容性检查和当前 `judge-build` 都不是官方评测器或完整提交沙箱。
