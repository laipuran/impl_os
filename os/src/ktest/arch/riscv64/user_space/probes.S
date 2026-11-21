    .section .rodata.user_space_probes, "a"
    .balign 4
    .option push
    .option norvc
    .option norelax
    .global riscv_user_space_start
    .global riscv_user_space_load
    .global riscv_user_space_store
    .global riscv_user_space_end
riscv_user_space_start:
    # 未知系统调用正常返回后，继续在同一地址访问本任务的数据和栈。
    mv t2, a0
    li a7, -1
    ecall
    li t0, -38
    bne a0, t0, .Lfailed
    ld t0, 0(t2)
    addi t0, t0, 1
    sd t0, 0(t2)
    sd t0, -8(sp)
    ld a0, -8(sp)
    li a7, 93
    ecall
.Lfailed:
    unimp
riscv_user_space_load:
    ld a0, 0(a0)
    unimp
riscv_user_space_store:
    sd zero, 0(a0)
    unimp
riscv_user_space_end:
    .option pop
