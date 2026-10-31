# 开发检查

## 命令覆盖范围

| 命令 | 检查范围 | 不覆盖 |
| --- | --- | --- |
| 容器内 `just check` | Rust 格式、双架构 Clippy | 链接产物、启动行为、内核功能 |
| 容器内 `just build` / `make all` | 两架构 ELF 构建与导出 | 启动及运行时语义 |
| 宿主 `just judge-build` | 原始比赛镜像断网，使用独立 Cargo 缓存构建 | 源码过滤、启动、评分 |
| 容器内 `just run rv` / `just run la` | 手动观察启动日志和进程退出 | 官方用户程序的正确性 |

当前没有自动化内核测试或官方测试集运行入口。格式和构建检查不能替代运行时功能测试。

## 当前运行行为

- 共享入口清零 BSS，输出架构及基架提示，然后调用 `arch::shutdown()`。
- RISC-V 使用 SBI System Reset 关机；固件不支持时停放 CPU。
- LoongArch 输出未实现关机提示并停放 CPU，需要手动退出 QEMU。
- panic 输出错误并停放 CPU，不走正常关机路径。

RISC-V 正常启动应输出日志后退出 QEMU。LoongArch 的持续运行是当前停放逻辑，不代表已经完成主动关机。
