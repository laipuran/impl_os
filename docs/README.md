# 文档导航

## 从哪里开始

1. [环境与 Dev Container](getting-started/environment.md)
2. [日常构建、运行和调试](getting-started/workflow.md)
3. [代码布局与模块职责](architecture/layout.md)
4. [启动流程](architecture/boot.md)
5. [Make / Just 任务分工](development/targets.md)
6. [开发检查](testing/README.md)

## 各子目录职责

| 目录 | 放什么 | 不放什么 |
|---|---|---|
| `getting-started/` | 可直接执行的环境、开发流程说明 | 尚未落地的架构设想 |
| `architecture/` | 当前代码结构、模块边界、启动流程 | 未实现模块的占位设计 |
| `development/` | 编码、安全、依赖及提交约定 | 单次运行的原始日志 |
| `testing/` | 现有命令的检查范围、手动验证方式 | 未接入的测试计划、原始日志与验证流水账 |

文档只描述当前代码、配置和约束，不预设后续模块或目录。变更代码结构、工具链或运行约定时，同步更新对应页面。命令需注明宿主或容器内的执行位置。
