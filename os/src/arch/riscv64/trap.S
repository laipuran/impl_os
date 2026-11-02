.section .text.trap,"ax"
.balign 4
.globl __trap_entry
__trap_entry:
    # S-mode only: hardware does not save GPRs or switch stacks.
    # 36 RV64 slots preserve the ABI's 16-byte stack alignment.
    addi sp, sp, -288
    sd zero,   0*8(sp)
    sd ra,     1*8(sp)
    sd gp,     3*8(sp)
    sd tp,     4*8(sp)
    sd t0,     5*8(sp)
    sd t1,     6*8(sp)
    sd t2,     7*8(sp)
    sd s0,     8*8(sp)
    sd s1,     9*8(sp)
    sd a0,    10*8(sp)
    sd a1,    11*8(sp)
    sd a2,    12*8(sp)
    sd a3,    13*8(sp)
    sd a4,    14*8(sp)
    sd a5,    15*8(sp)
    sd a6,    16*8(sp)
    sd a7,    17*8(sp)
    sd s2,    18*8(sp)
    sd s3,    19*8(sp)
    sd s4,    20*8(sp)
    sd s5,    21*8(sp)
    sd s6,    22*8(sp)
    sd s7,    23*8(sp)
    sd s8,    24*8(sp)
    sd s9,    25*8(sp)
    sd s10,   26*8(sp)
    sd s11,   27*8(sp)
    sd t3,    28*8(sp)
    sd t4,    29*8(sp)
    sd t5,    30*8(sp)
    sd t6,    31*8(sp)
    # t0 is saved, so it can now recover the pre-trap stack pointer.
    addi t0, sp, 288
    sd t0,     2*8(sp)
    csrr t0, sepc
    sd t0,    32*8(sp)
    csrr t0, sstatus
    sd t0,    33*8(sp)
    csrr t0, scause
    sd t0,    34*8(sp)
    csrr t0, stval
    sd t0,    35*8(sp)
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop
    mv a0, sp
    call rust_trap_handler
    # The handler never returns.
.Ltrap_park:
    wfi
    j .Ltrap_park
