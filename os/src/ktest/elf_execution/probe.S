.section .rodata
.balign 4
.option push
.option norelax
.global elf_execution_start
.global elf_execution_end
elf_execution_start:
    andi t0, sp, 15
    bnez t0, 1f
    ld t0, 0(sp)
    li t1, 1
    bne t0, t1, 1f
    ld t0, 8(sp)
    lwu t0, 0(t0)
    li t1, 0x676f7270
    bne t0, t1, 1f
    ld t0, 16(sp)
    bnez t0, 1f
    ld t0, 24(sp)
    lbu t0, 2(t0)
    li t1, 'V'
    bne t0, t1, 1f
    ld t0, 40(sp)
    li t1, 9
    bne t0, t1, 1f
    ld t0, 48(sp)
    li t1, 0x10100
    bne t0, t1, 1f
    li t0, 0x12380
    lbu a0, 0(t0)
    li t1, 42
    bne a0, t1, 1f
    lbu t1, -1(t0)
    bnez t1, 1f
    lbu t1, 1(t0)
    bnez t1, 1f
    li t0, 0x13000
    lbu t1, 0(t0)
    bnez t1, 1f
    j 2f
1:
    li a0, 1
2:
    li a7, 93
    ecall
    unimp
elf_execution_end:
.option pop
