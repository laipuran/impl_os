# 实施路线图

这是一份按依赖排序的工程路线，不代表比赛权重。每阶段以真实运行结果验收。

- [x] Dev Container 配置、Rust workspace、双架构构建入口、文档职责。
- [x] RISC-V 替代环境的启动/串口/SBI 关机冒烟。
- [x] 固定 nightly 下双架构 ELF、Clippy、格式检查。
- [x] LoongArch 替代环境的启动/串口冒烟。
- [x] 在比赛指定原始镜像内完成固定工具链、断网/过滤源码、双 ELF 和启动复核。
- [x] Make 产物契约与 Just 开发任务分离，薄开发镜像回归。
- [x] 删除 Python 开发工具及其测试，简单开发命令直接写在 Justfile。
- [ ] LoongArch 正常关机。
- [ ] trap、时钟、物理页/地址空间、用户 ELF 最小闭环。
- [ ] VirtIO block、EXT4、VFS 与最小用户态 runner。
- [ ] basic 逐项回归；进程、等待、fd/pipe 和内存语义。
- [ ] BusyBox/Lua，统一 shell、工作目录与测试输出协议。
- [ ] libctest runner、信号、TLS/futex、动态 ELF/glibc。
- [ ] 文件/进程性能基准，正确性稳定后再优化。
- [ ] loopback 网络、线程实时负载、LTP 分层扩展。
- [ ] 最终官方环境、测试版本、资源限制与评分协议核对。

暂不预建网络、用户库和多 crate 框架；达到相应阶段时再按实际所有权和依赖扩展结构。
