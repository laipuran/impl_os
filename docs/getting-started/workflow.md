# 日常编译、运行与调试

除特别注明外，以下命令均在**开发容器内的仓库根目录**执行。各命令的详细参数定义见 [任务对照表](../development/targets.md)。

## 编译与运行

```bash
just                          # 列出所有可用任务
just doctor                   # 检查开发环境中的工具版本
just build                    # 调用 make all，编译双架构 release 内核（kernel-rv、kernel-la）
just build rv debug           # 仅编译 RISC-V 的 debug 内核（双架构 debug 可用 just build all debug）
just check                    # 运行代码格式检查与双架构 Clippy 检查
just run rv                   # 编译 release 版 RISC-V 内核并在 QEMU 中运行
just run la                   # 编译 release 版 LoongArch 内核并在 QEMU 中运行
MEM=128M SMP=1 just run rv    # 自定义 QEMU 内存大小与核心数
```

- **输出文件注意**：无论 `release` / `debug` 模式还是是否启用 `ktest` 等不同 feature，同一架构构建导出的根目录产物均同名（`kernel-rv` 和 `kernel-la`）。请勿并发执行同一架构的导出构建、`just run`、`just test` 或 `just debug`，以免覆盖或启动错误的内核 ELF。
- **透传 QEMU 参数**：`just run <rv|la>` 和 `just debug <rv|la>` 之后追加的任意参数都会原样传给 QEMU（例如 `-initrd ...`）。
- **退出方式**：RISC-V 运行结束后通过 OpenSBI 自动退出 QEMU；LoongArch 尚未实现关机，打印提示后进入 `idle` 等待，按 `Ctrl-a` 松开后再按 `x` 即可退出 QEMU。

## 使用 GDB 调试

在第一个容器终端启动调试目标（自动以 debug 模式编译，并在入口处暂停等待 GDB 连接）：

```bash
just debug rv                 # RISC-V（监听容器内 :1234）
# 或
just debug la                 # LoongArch（监听容器内 :1234）
```

在**同一个容器实例**中打开第二个终端，启动 `gdb-multiarch` 连接调试（VS Code Dev Containers 直接新建终端即可；若使用宿主机命令行工作流，请执行 `docker exec -it <容器名或ID> bash` 进入正在运行 QEMU 的那个容器，切勿再次执行 `just container` 启动新容器，否则独立的网络命名空间会导致 `:1234` 无法连通）：

```bash
gdb-multiarch kernel-rv       # LoongArch 请改为 kernel-la
```

在 GDB 交互界面中：

```gdb
target remote :1234
break kernel_main
continue
```

## 提交前验证与清理

- 在**开发容器内**运行 `just test rv`、`just test la` 和 `just check`，确保双架构自检与静态检查通过（详见 [内核自检与测试验证](../testing/README.md)）。
- 在**宿主机**运行 `just judge-build`，模拟评测机断网、无隐藏文件的纯净环境编译双架构 ELF。
- 运行 `just clean` 可清理 `target/` 及根目录的 `kernel-rv`、`kernel-la`（不会删除 `artifacts/` 下的测试镜像与日志）。
