# 文档导航

## 从哪里开始

1. [环境与 Dev Container](getting-started/environment.md)
2. [日常构建、运行和调试](getting-started/workflow.md)
3. [代码布局与模块职责](architecture/layout.md)
4. [启动流程](architecture/boot.md)
5. [Make / Just 任务分工](development/targets.md)
6. [当前测试状态](testing/README.md)、[比赛容器实测](testing/container-validation.md)与[测试矩阵](testing/test-matrix.md)
7. [实施路线图](roadmap.md)

## 各子目录职责

| 目录 | 放什么 | 不放什么 |
|---|---|---|
| `getting-started/` | 可直接执行的环境、开发流程说明 | 尚未落地的架构设想 |
| `architecture/` | 已有结构、模块边界、启动/内存等设计 | 假定已经实现的占位能力 |
| `development/` | 编码、安全、依赖及提交约定 | 单次运行的原始日志 |
| `testing/` | 验证步骤、环境、覆盖矩阵、回归记录 | 测试镜像与大体积二进制 |
| `research/` | 外部源码/规范调研导航与来源 | 不加区分地当作当前实现的规则 |

根目录现有的 [competition-toolchain-and-tests.md](competition-toolchain-and-tests.md) 保留原位置；它是有版本锚点的外部调研，不是本项目的通过记录。

记录规则：区分 **已实现 / 已验证 / 待实现 / 待官方确认**；命令应注明运行位置。变更模块边界、工具链、运行约定时同步更新对应页面。
