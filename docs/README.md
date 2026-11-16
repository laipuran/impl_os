# 文档导航

## 阅读顺序推荐

1. [开发环境与 Dev Container 配置](getting-started/environment.md)
2. [日常编译、运行与 GDB 调试](getting-started/workflow.md)
3. [代码结构与模块职责](architecture/layout.md)
4. [启动流程、内存管理与异常处理](architecture/boot.md)
5. [Makefile 与 Justfile 任务对照表](development/targets.md)
6. [开发规范与离线依赖维护](development/README.md)
7. [内核自检与测试验证](testing/README.md)

## 文档目录结构

| 子目录 | 内容范围 |
| --- | --- |
| `getting-started/` | 容器环境搭建、工具链配置、日常编译运行与 GDB 调试步骤 |
| `architecture/` | 当前已实现的模块划分、启动流程、物理内存管理、Trap 与最小任务机制 |
| `development/` | 编码规范、`Makefile`/`justfile` 命令分工、离线依赖（`vendor/`）更新与提交流程 |
| `testing/` | 静态检查、`ktest` 启动期自检套件覆盖范围及手动故障注入验证方法 |

> **文档维护约定：** `docs/` 仅记录当前仓库已落地的设计与命令，不提前堆砌尚未实现的功能设想。调整模块边界、构建脚本或运行参数时，请同步更新对应文档，并标明命令是在**宿主机**还是**开发容器内**执行。
