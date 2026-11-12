    .section .rodata.paging_probes, "a"
    .balign 4
    .option push
    .option norvc
    .option norelax

    # 整段复制到独占代码页；不引用链接地址或内核全局状态。
    .global riscv_paging_user_start
    .global riscv_paging_user_load
    .global riscv_paging_user_store
    .global riscv_paging_user_end
riscv_paging_user_start:
    ld t0, 0(a0)
    addi t0, t0, 1
    sd t0, 0(a0)
    sd t0, -8(sp)
    ld a0, -8(sp)
    li a7, 93
    ecall
    unimp
riscv_paging_user_load:
    ld a0, 0(a0)
    ecall
    unimp
riscv_paging_user_store:
    sd a1, 0(a0)
    ecall
    unimp
riscv_paging_user_end:
    .option pop
