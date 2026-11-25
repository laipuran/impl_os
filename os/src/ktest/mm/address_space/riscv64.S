.pushsection .rodata.address_space_probe, "a", @progbits

.option push
.option norvc
.option norelax

.equ ADDRESS_SPACE_SYS_UNKNOWN, -1
.equ ADDRESS_SPACE_SYS_EXIT, 93
.equ ADDRESS_SPACE_NEG_ENOSYS, -38

.balign 4
.global riscv_user_space_start
riscv_user_space_start:
    # 未知系统调用正常返回后，继续在同一地址访问本任务的数据和栈。
    mv t2, a0
    li a7, ADDRESS_SPACE_SYS_UNKNOWN
    ecall
    li t0, ADDRESS_SPACE_NEG_ENOSYS
    bne a0, t0, .Laddress_space_failed
    ld t0, 0(t2)
    addi t0, t0, 1
    sd t0, 0(t2)
    sd t0, -8(sp)
    ld a0, -8(sp)
    li a7, ADDRESS_SPACE_SYS_EXIT
    ecall
.Laddress_space_failed:
    unimp

.balign 4
.global riscv_user_space_load
riscv_user_space_load:
    ld a0, 0(a0)
    unimp

.balign 4
.global riscv_user_space_read_byte
riscv_user_space_read_byte:
    lbu a0, 0(a0)
    li a7, ADDRESS_SPACE_SYS_EXIT
    ecall
    unimp

.balign 4
.global riscv_user_space_store
riscv_user_space_store:
    sd zero, 0(a0)
    unimp

.global riscv_user_space_code_byte
riscv_user_space_code_byte:
    .byte 0x5a
.global riscv_user_space_end
riscv_user_space_end:

.option pop
.popsection
