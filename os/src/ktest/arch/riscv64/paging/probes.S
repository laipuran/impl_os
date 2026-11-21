    .section .rodata.paging_probes, "a"
    .balign 4
    .option push
    .option norvc
    .option norelax

    # 整段复制到独占代码页；不引用链接地址或内核全局状态。
    # 各探针末尾的 `unimp` 作为不可达哨兵，防止意外越过 `ecall` 继续执行。
    .global riscv_paging_user_start
    .global riscv_paging_user_load
    .global riscv_paging_user_store
    .global riscv_paging_user_end
    # 入口约定：a0 = 用户数据页虚拟地址，sp = 对齐的用户栈顶。
    # 将 *a0 加 1 后写回数据页并写入栈顶下方的槽位（sp - 8，不调整 sp），再读回作为退出码通过 exit(93) 返回。
riscv_paging_user_start:
    ld t0, 0(a0)
    addi t0, t0, 1
    sd t0, 0(a0)
    sd t0, -8(sp)
    ld a0, -8(sp)
    li a7, 93
    ecall
    unimp
    # 入口约定：a0 = 待读取的目标虚拟地址。
    # 成功时将读取结果留在 a0 并触发 ecall；缺失映射或权限不足时在 ld 处触发缺页异常。
riscv_paging_user_load:
    ld a0, 0(a0)
    ecall
    unimp
    # 入口约定：a0 = 待写入的目标虚拟地址，a1 = 写入值。
    # 成功时触发 ecall；缺失写权限或未映射时在 sd 处触发缺页异常。
riscv_paging_user_store:
    sd a1, 0(a0)
    ecall
    unimp
riscv_paging_user_end:
    .option pop
