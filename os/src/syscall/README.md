# syscall：Linux ABI 边界（待实现）

负责 syscall 编号分发、参数/结构体转换、用户内存校验以及原始 Linux 返回值/errno 约定。实际业务委托给 `task`、`mm`、`fs` 等模块。

不要把用户测试程序名当成 syscall 名；musl/glibc 和两种架构的封装可能不同。不要用无条件成功绕过未实现操作，应保留可诊断的真实失败。
