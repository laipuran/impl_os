# 本地外部资源与运行记录（不提交大文件）

建议本地放置：

```text
artifacts/
  test-images/   # 官方测试盘；运行时解压为原始 EXT4 镜像
  logs/          # 串口日志、环境核查输出
```

本目录除 README 外均被 Git 忽略。下载镜像时另外记录来源、测试 commit、SHA256；不要将压缩文件直接传给 QEMU 的 raw drive。

内核 ELF 按比赛约定输出到仓库根目录；Rust 中间产物在 `target/`。不要把评测构建所必需的源码或依赖藏在这里。
