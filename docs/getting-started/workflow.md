# 日常开发流程

以下命令在开发容器内的**仓库根目录**执行：

```bash
just
just doctor
just build                    # 调用 make all，双架构 release ELF
just build-rv debug           # 单架构 debug ELF
just check                    # 格式与双架构 Clippy
just run-rv                   # 构建 release、启动、输出、关机
just run-la                   # 构建 release、启动、输出、停放（关机待实现）
MEM=128M SMP=1 just run-rv
```

比赛直接执行 `make all`，不依赖 Just。每个命令的完整职责见[分工表](../development/targets.md)。不要并发运行同一架构的 debug/release 导出操作，因为根目录 ELF 同名。

`run-*` 和 `debug-*` 的额外参数直接传给 QEMU，使用其原生选项。

## GDB

```bash
just debug-rv                 # 构建 debug ELF，QEMU 暂停，监听容器内 :1234
# 第二个容器终端：
gdb-multiarch kernel-rv
# GDB 内：
target remote :1234
break kernel_main
continue
```

LoongArch 对应 `just debug-la` / `kernel-la`。不向公网暴露 GDB 端口。

## 手动验证与清理

RISC-V 应看到启动日志并正常退出；LoongArch 应看到启动和未实现关机提示，需要 `Ctrl-a` 后按 `x` 退出。

宿主可执行 `just judge-build`，在原始比赛镜像禁网构建。`just clean` 只清理构建产物，保留测试盘和日志。各命令的覆盖范围见[开发检查](../testing/README.md)。
