# 代码布局与职责

## 当前依赖方向

```text
Just 开发任务 → Make 产物构建 → Cargo workspace → os/build.rs 选择链接脚本
比赛 make all ──────┘                           → entry.S → kernel_main
```

目前只建立一个内核 crate，避免在边界尚未稳定时提前拆分多个互相依赖的 crate。硬件差异用 Rust target 的 `cfg` 选择，不用手动复制文件或隐藏 Cargo 配置。

## 内核子目录

| 路径 | 状态 | 责任 |
|---|---|---|
| `main.rs` | 已有 | 启动编排、BSS 清零、panic 入口 |
| `console.rs` | 已有，仅早期启动 | `core::fmt` 文本输出；当前无锁、不支持并发写入 |
| `arch/` | 已有最小实现 | 汇编入口、链接布局、架构寄存器/固件操作、早期 UART |
| `mm/` | 仅职责说明 | 物理页、堆、地址空间、用户映射/缺页/用户访问 |
| `task/` | 仅职责说明 | 进程、线程、调度、阻塞/唤醒、退出/回收 |
| `fs/` | 仅职责说明 | VFS、fd、路径、管道、挂载、EXT4 |
| `drivers/` | 仅职责说明 | VirtIO、块设备、正式串口、中断及未来网卡 |
| `syscall/` | 仅职责说明 | Linux ABI 分发、结构体转换、errno 与参数边界 |

预留目录只有 README，尚未写 `mod` 或构造假的接口。网络栈、ELF loader、时间和同步模块在实际需求明确时再建立，并更新这里。

Make 只有 `all/kernel-rv/kernel-la/clean` 四个目标；开发任务统一由 Just 编排，详细见[任务分工](../development/targets.md)。

## 后续边界约束

- syscall 调用业务模块，不直接访问页表/磁盘/运行队列内部状态。
- fs 依赖块设备能力，不让 VirtIO 驱动解释路径。
- task 持有地址空间和文件资源，不让调度策略耦合具体 EXT4 实现。
- arch 提供机制，不收纳跨架构共享的业务策略。
- 测试必须验证副作用与失败语义，不以打印成功或恒定返回值替代实现。
