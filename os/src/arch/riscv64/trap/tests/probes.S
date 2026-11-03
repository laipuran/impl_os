.section .text.trap_smoke,"ax"
.option push
.option norvc
.option norelax

.macro RV_SMOKE_CHECK reg, expected
    li t0, \expected
    bne \reg, t0, .Lrv_smoke_failed
.endm

.balign 4
.globl riscv_smoke_kernel
.type riscv_smoke_kernel, @function
riscv_smoke_kernel:
    addi sp, sp, -128
    sd ra, 0(sp)
    sd gp, 8(sp)
    sd tp, 16(sp)
    sd s0, 24(sp)
    sd s1, 32(sp)
    sd s2, 40(sp)
    sd s3, 48(sp)
    sd s4, 56(sp)
    sd s5, 64(sp)
    sd s6, 72(sp)
    sd s7, 80(sp)
    sd s8, 88(sp)
    sd s9, 96(sp)
    sd s10, 104(sp)
    sd s11, 112(sp)
    sd sp, 120(sp)
    li ra, 0x101
    li gp, 0x103
    li tp, 0x104
    li t0, 0x105
    li t1, 0x106
    li t2, 0x107
    li s0, 0x108
    li s1, 0x109
    li a0, 0x10a
    li a1, 0x10b
    li a2, 0x10c
    li a3, 0x10d
    li a4, 0x10e
    li a5, 0x10f
    li a6, 0x110
    li a7, 0x111
    li s2, 0x112
    li s3, 0x113
    li s4, 0x114
    li s5, 0x115
    li s6, 0x116
    li s7, 0x117
    li s8, 0x118
    li s9, 0x119
    li s10, 0x11a
    li s11, 0x11b
    li t3, 0x11c
    li t4, 0x11d
    li t5, 0x11e
    li t6, 0x11f
.globl riscv_smoke_kernel_break
riscv_smoke_kernel_break:
    ebreak
    # Check t0 before borrowing it to check every other restored GPR.
    addi t0, t0, -0x205
    bnez t0, .Lrv_smoke_failed
    RV_SMOKE_CHECK ra, 0x201
    RV_SMOKE_CHECK gp, 0x203
    RV_SMOKE_CHECK tp, 0x204
    RV_SMOKE_CHECK t1, 0x206
    RV_SMOKE_CHECK t2, 0x207
    RV_SMOKE_CHECK s0, 0x208
    RV_SMOKE_CHECK s1, 0x209
    RV_SMOKE_CHECK a0, 0x20a
    RV_SMOKE_CHECK a1, 0x20b
    RV_SMOKE_CHECK a2, 0x20c
    RV_SMOKE_CHECK a3, 0x20d
    RV_SMOKE_CHECK a4, 0x20e
    RV_SMOKE_CHECK a5, 0x20f
    RV_SMOKE_CHECK a6, 0x210
    RV_SMOKE_CHECK a7, 0x211
    RV_SMOKE_CHECK s2, 0x212
    RV_SMOKE_CHECK s3, 0x213
    RV_SMOKE_CHECK s4, 0x214
    RV_SMOKE_CHECK s5, 0x215
    RV_SMOKE_CHECK s6, 0x216
    RV_SMOKE_CHECK s7, 0x217
    RV_SMOKE_CHECK s8, 0x218
    RV_SMOKE_CHECK s9, 0x219
    RV_SMOKE_CHECK s10, 0x21a
    RV_SMOKE_CHECK s11, 0x21b
    RV_SMOKE_CHECK t3, 0x21c
    RV_SMOKE_CHECK t4, 0x21d
    RV_SMOKE_CHECK t5, 0x21e
    RV_SMOKE_CHECK t6, 0x21f
    ld t0, 120(sp)
    bne sp, t0, .Lrv_smoke_failed
    li a0, 1
    j .Lrv_smoke_finish
.Lrv_smoke_failed:
    li a0, 0
.Lrv_smoke_finish:
    ld ra, 0(sp)
    ld gp, 8(sp)
    ld tp, 16(sp)
    ld s0, 24(sp)
    ld s1, 32(sp)
    ld s2, 40(sp)
    ld s3, 48(sp)
    ld s4, 56(sp)
    ld s5, 64(sp)
    ld s6, 72(sp)
    ld s7, 80(sp)
    ld s8, 88(sp)
    ld s9, 96(sp)
    ld s10, 104(sp)
    ld s11, 112(sp)
    addi sp, sp, 128
    ret
.size riscv_smoke_kernel, .-riscv_smoke_kernel

.balign 4
.globl riscv_smoke_nested
.type riscv_smoke_nested, @function
riscv_smoke_nested:
.globl riscv_smoke_nested_break
riscv_smoke_nested_break:
    ebreak
    ret
.size riscv_smoke_nested, .-riscv_smoke_nested

.balign 4
.globl riscv_smoke_user_first
riscv_smoke_user_first:
    ecall
    # A wrong PC update must fail, not accidentally reach the resume probe.
    .word 0
.globl riscv_smoke_user_second
riscv_smoke_user_second:
    ecall
    .word 0
.globl riscv_smoke_user_privileged
riscv_smoke_user_privileged:
    csrw sscratch, zero
    .word 0
.globl riscv_smoke_user_float
riscv_smoke_user_float:
    # fadd.s f0,f0,f0: FS=Off must raise an illegal-instruction exception.
    .word 0x00000053
    .word 0
.option pop
