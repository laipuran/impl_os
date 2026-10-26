# 日常开发流程

以下命令在开发容器内的**仓库根目录**执行：

```bash
just
just doctor
just build                    # 调用 make all，双架构 release ELF
just build-rv debug           # 单架构 debug ELF
just check                    # 格式与双架构 Clippy，不是功能测试
just run-rv                   # 构建 release、启动、输出、关机
just run-la                   # 构建 release、启动、输出、停放（关机待实现）
MEM=128M SMP=1 just run-rv
```

比赛直接执行 `make all`，不依赖 Just。每个命令的完整职责见[分工表](../development/targets.md)。不要并发运行同一架构的 debug/release 导出操作，因为根目录 ELF 同名。

## 挂载官方测试盘

参数直接传给 QEMU，不再使用 Python 封装。先取得版本匹配并解压的 raw EXT4 镜像，记录 SHA256：

```bash
just run-rv \
  -drive file=artifacts/test-images/sdcard-rv.img,if=none,format=raw,id=x0 \
  -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0

just run-la \
  -drive file=artifacts/test-images/sdcard-la.img,if=none,format=raw,id=x0 \
  -device virtio-blk-pci,drive=x0
```

可选辅助盘使用 `id=x1`；RISC-V 块设备接 `virtio-mmio-bus.1`，LoongArch 使用 PCI 设备自动连接 PCI 总线。不要照搬历史说明中 PCI 设备接 MMIO 总线的错误参数。

**当前内核不能读取测试盘或执行用户态测试。** 磁盘默认可写，使用可丢弃副本，或在上述命令末尾追加 QEMU 原生参数 `-snapshot`。

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

RISC-V 应看到启动日志并正常退出；LoongArch 应看到启动和未实现关机提示，需要 `Ctrl-a` 后按 `x` 退出。看到日志不等于官方用户态测试通过。

宿主可执行 `just judge-build`，在原始比赛镜像禁网构建；它不启动 QEMU，也不运行测试。`just clean` 只清理构建产物，不删除测试盘或日志。
