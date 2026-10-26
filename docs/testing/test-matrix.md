# 官方用例接入矩阵

外部依据：[工具链与测试源码调研](../competition-toolchain-and-tests.md)。分支 `pre-2025` 不是已确认的 2026 最终评分器。

目标矩阵为 **RISC-V/LoongArch × musl/glibc**；basic 使用自己的无 libc 用户库。以下均为**尚未接入**：

| 组 | 优先依赖 | 当前状态 |
|---|---|---|
| basic | ELF/用户态、trap、进程、内存、fd、目录、pipe、时钟 | 待实现 |
| BusyBox | VFS、exec/wait、shell、管道/重定向 | 待实现 |
| Lua | ELF、文件、时间、基础 libc 运行环境 | 待实现 |
| libctest | runner 的信号/等待/超时，再到 TLS/futex/动态加载 | 待实现 |
| Iozone | 文件读写、定位/向量 I/O、并发与同步 | 待实现 |
| UnixBench | 进程、pipe、exec、定时与文件 I/O | 待实现 |
| libcbench | libc 程序运行、内存、真实计时 | 待实现 |
| lmbench | 进程、信号、select、文件页错误/mmap | 待实现 |
| iperf / netperf | loopback TCP/UDP、socket、多连接 | 待实现 |
| cyclictest | 线程、定时唤醒、调度优先级、压力负载 | 待实现 |
| LTP | 按实际安装用例拆分 Linux ABI/边界语义 | 待实现 |

## 接入约定

- 使用实际安装的 `*_testcode.sh`，不盲用有已知问题的 `test_all.sh`。
- 进入 `/musl` 或 `/glibc`，检查 PATH、shell、可写目录和解释器路径。
- 组之间串行，保留官方 START/END 标志及真实子用例输出；组内部并发必须正常支持。
- 同时记录两架构/两 libc 的真实结果；未运行不标通过。
- 最终测试结束应主动关机，超时和停机需分开判断。
