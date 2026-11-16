# 仓库开发与协作指南（`AGENTS.md`）

## 项目概述

`impl_os` 是面向全国大学生计算机系统能力大赛操作系统设计赛（内核实现赛）的 Rust `#![no_std]`、`#![no_main]` 内核，同时支持 RISC-V 64（`riscv64gc-unknown-none-elf`）与 LoongArch 64（`loongarch64-unknown-none`）双架构，运行于 QEMU `virt` 平台。

## 构建架构与目录职责

- **构建链路**：日常开发命令通过 `justfile` 编排，其中内核编译统一委托给根目录 `Makefile`。比赛评测入口为 `make all`，通过 Cargo `--locked --offline` 读取仓库内的 `vendor/` 离线编译双架构 ELF（`kernel-rv`、`kernel-la`）。`os/build.rs` 根据目标架构自动注入对应的链接脚本与链接参数，不依赖可能被评测机过滤的 `.cargo/` 隐藏目录。
- **主要目录**：
  - `os/`：内核主 crate、构建脚本（`build.rs`）、共享内核模块及双架构启动/Trap/链接代码（`os/src/arch/{riscv64,loongarch64}/`）。
  - `docs/`：环境搭建、日常工作流、架构设计、构建任务分工与测试验证文档（详见 [`docs/README.md`](docs/README.md)）。
  - `cargo/config.toml` 与 `vendor/`：Cargo 离线源重定向配置与随仓库提交的第三方依赖源码。
  - `.devcontainer/`：基于官方固定比赛镜像派生的开发容器配置。
  - `artifacts/`：被 Git 忽略的本地测试磁盘镜像、串口日志与调试临时文件（详见 [`artifacts/README.md`](artifacts/README.md)）。
- **关键文件**：
  - `os/src/main.rs`：内核 Rust 入口 `kernel_main`、BSS 清零与早期初始化流程。
  - `os/src/console.rs` 与 `os/src/lang_items.rs`：早期串口输出、`println!` 宏、全局堆分配失败与 panic 停机处理。
  - `Makefile`、`justfile`、`Cargo.toml`、`os/Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`：构建契约与工具链锁定配置。

## 运行环境与工具链约束

- **开发环境**：使用 Linux/amd64 开发容器（VS Code Reopen in Container，或在宿主机依次执行 `just image-pull`、`just image-build`、`just container`）。
- **工具链锁定**：固定为 `nightly-2025-01-18`。缺失工具链组件时可在容器内显式执行 `just setup` 安装；注意 Cargo 的 `--offline` 不会阻止 `rustup` 自动联网，评测复核统一使用宿主机的 `just judge-build`。
- **禁止无 target 编译**：直接执行不带 `--target` 的普通 `cargo build` 不是本项目的构建入口，请始终使用 `make all` 或 `just build` / `just check` / `just test`。

## 编码约定与内核开发准则

- **代码风格**：保持清晰的模块边界，函数与模块使用 `snake_case`，提交前通过 `just fmt`（`cargo fmt --all`）保持格式统一。
- **架构解耦**：与特定 ISA、CSR、汇编或平台启动表相关的实现收敛在 `os/src/arch/` 下，共享内核模块通过 `crate::arch` 访问硬件能力，避免在业务代码中散落 `#[cfg(target_arch = ...)]`。
- **底层安全与硬件假设**：
  - 严格遵守启动栈（`.bss.stack`）与 BSS 清零区间（`sbss..ebss`）的边界，以及当前单核、关中断的执行假设。
  - 底层 MMIO 访问需使用 `read_volatile` / `write_volatile`；不得将未经校验的用户态裸指针直接转换为 Rust 引用。
  - 内核发生 panic 或致命异常时必须调用 `arch::halt()` 挂起 CPU，严禁调用 `arch::shutdown()` 伪造正常关机。
- **依赖管理**：仅引入兼容双架构的 `no_std` 依赖。新增或更新依赖前参阅 [`docs/development/README.md`](docs/development/README.md)，通过 `just vendor` 更新离线源码并一并提交 `Cargo.toml`、`Cargo.lock` 与 `vendor/`。

## Rust 注释与文档规范

遵循 [Rust API Guidelines (Documentation)](https://rust-lang.github.io/api-guidelines/documentation.html) 与 [The rustdoc Book](https://doc.rust-lang.org/rustdoc/)，统一使用自然、准确的中文编写注释，避免生硬的机器直译、句中硬换行或中英混杂。

### 文档注释（`///` 与 `//!`）

- 使用 `//!` 概述模块职责与核心不变量，使用 `///` 描述公开结构体、枚举、trait 和函数；私有条目仅在职责、边界条件或安全前提不直观时补充文档注释。
- 首行用简明的一句话概括用途，详细说明另起一段。重点交代输入约束、边界行为、地址类型（物理地址 vs 虚拟地址）、生命周期与副作用，不机械重复函数签名已表达的类型信息。
- 按实际需要添加标准章节（不生成空章节或生搬硬套模板）：
  - `# Errors`：说明返回 `Err` 的触发条件；
  - `# Panics`：说明已知会触发 panic 的条件；
  - `# Safety`：逐条列出 `unsafe fn` 调用方必须满足的安全前提（如特权级、关中断、地址映射、对齐与独占访问要求）；
  - `# Examples`：需要展示典型用法时提供示例，切勿通过随意添加 `ignore` / `no_run` 掩盖错误的示例代码。
- 引用代码条目时优先使用 rustdoc 内部链接（如 `[`Self::new`]`）。

### 实现注释（`//`）

- 使用 `//` 解释**为什么这样设计（Why）**、硬件规范约束、数据布局不变量以及非显然的边界处理；不要逐行翻译代码逻辑，也不要添加无意义的分隔线或显而易见的废话注释。
- 每个 `unsafe` 块和 `unsafe impl` 必须紧邻附上 `// SAFETY:` 注释，具体说明当前上下文如何满足对应的安全前提，不能只写笼统的“此处是安全的”。
- 临时绕过方案（`TODO` / `FIXME`）需注明原因及后续移除或补齐的条件。
- 修改代码时同步更新或删除已失效的注释，不保留大段被注释掉的废弃代码。

### 验证要求

- 修改代码或注释后，运行 `just check`（格式检查 + 双架构默认与 `ktest` 配置的 Clippy）、`just test rv`、`just test la`，以及双架构 `cargo doc --no-deps`，修复所有编译、Clippy 或 rustdoc 警告。

## AI 相关文件与仓库卫生

- 除 `AGENTS.md` / `CLAUDE.md` 等项目规范文件外，严禁将任何 AI 工具配置、提示词、技能脚本（`.agents/`）、对话记录或会话元数据提交到 Git。
- 本地开发使用的未跟踪文件应通过 `.git/info/exclude` 或全局 Git 忽略配置管理，不要为了忽略本地个人工具文件而修改共享的 `.gitignore`。
- 严禁在源代码、注释、文档或 Commit / PR 信息中留下 AI 工具名称、生成归属或任何 AI 参与痕迹。
- 保持改动聚焦于当前任务，不顺手混入无关的重构、大范围格式刷写或未请求的依赖升级。

## Git 与提交纪律

- **原子提交**：每次提交聚焦一个独立的逻辑变更；不相关的重构、修复或功能应拆分为独立提交。不提交未完成或编译/测试不过的改动。
- **提交信息格式**：遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范：`<type>(<optional scope>): <description>`（如 `feat(mm): add SV39 page table mapper`、`fix(trap): preserve sscratch on nested kernel trap`、`docs: rework project documentation and code comments`）。标题简洁具体，准确传达改动意图。
- **显式确认**：完成并验证每个工作单元后，先向用户展示暂存文件列表（`git status`）、差异摘要与拟议的提交信息。**每次执行 `git commit` 前都必须获得用户的明确授权**，不得将“同意代码实现”或“同意上一次提交”视为新提交的授权。
- **暂存与历史保护**：有选择地暂存目标文件，提交前复核 `git diff --staged`，确保未混入测试镜像、编译产物（`kernel-rv`、`kernel-la`）、临时文件或本地配置。未经用户明确要求，严禁包含或丢弃用户已有的未提交改动，严禁执行 `commit --amend`、`rebase`、改写历史或推送到远程仓库（`git push`）。

## Pull Request（PR）正文规范

参照 Google Engineering Practices 与主流开源内核项目（Linux / Asterinas / Rust）的 PR 规范，PR 描述应面向代码审阅者与未来维护者，基于分支相对目标分支的**最终净变更（net diff）与提交序列**撰写，独立可读：

1. **标题与粒度**：标题遵循 Conventional Commits 格式；单个 PR 聚焦同一主题或里程碑子任务，避免将不相关的模块改动塞入巨型 PR。
2. **推荐正文结构**（按实际变更复杂度裁剪，不堆砌空章节）：
   - **背景与目的（Motivation / Why）**：简明交代为什么要做这项改动——解决什么问题、对应哪个架构约束或比赛阶段目标，而非仅仅复述改了哪些文件。
   - **核心变更与设计决策（Key Changes & Design）**：按抽象层级从高到低说明架构设计、模块职责边界、关键不变量（如内存所有权、页表生命周期、特权级与关中断假设）以及重要的取舍原因。局部符号调整、辅助函数抽取等实现细节归入所属特性，不与核心功能并列，也不逐文件罗列修改流水账。
   - **验证方式与边界（Verification）**：如实列出已执行的检查命令与测试结果（如 `just check`、`just test rv`、`just test la`、`just judge-build` 或具体测例输出），并明确说明当前尚未覆盖的边界或后续计划。
3. **文风要求**：保持客观、专业、精炼。严禁把开发过程中的排错弯路、多轮修改记录写成变更摘要，严禁出现“按你的要求”“最新一轮修改”等依赖聊天上下文的表述。

## PR Review 规范

遵循高召回率、建设性与行内锚定原则（参考 Google Code Review Guidelines 与 Conventional Comments）：

1. **审阅重心**：
   - **正确性与硬件/ABI 契约**：特权级切换、CSR 读写、汇编栈帧对齐、被调用者保存寄存器、页表/TLB 同步、Linux 系统调用 ABI 语义是否正确。
   - **内存安全与 `unsafe` 不变量**：`unsafe` 块是否具备充分的安全前提，是否存在悬垂物理页、未校验用户指针、越界读写、整数溢出或并发/重入隐患。
   - **架构可维护性与文档一致性**：是否破坏了 `arch` 与通用内核模块的边界，注释与文档是否与代码实际行为完全一致。
2. **行内评论（Inline Comments）**：
   - 针对具体代码或文档的缺陷，**必须将评论锚定在 diff 对应的行或行范围上**，让 GitHub 直接展示代码上下文；不要把具体代码问题全部堆在 Review 总结正文里靠文件名和行号口述。
   - 每条评论应清晰说明：**问题是什么（What）**、**为什么会引发错误或违反哪条规范（Why）** 以及 **具体可落地的修复建议（Fix）**。
   - 明确区分严重程度（如阻塞合并的 `bug` / `security` / ` soundness` 缺陷，与非阻塞的代码风格/可读性建议 `nit`）。
3. **Review 总结正文（Summary Body）**：
   - 正文只保留整体审阅结论、验证覆盖范围以及无法归属到具体单行代码的跨模块架构问题，不重复罗列行内评论的细节。
