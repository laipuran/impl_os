# 与 entry.S 由同一个 global_asm! 顺序装配，共享 Rust 提供的 TF_* 布局。
.macro RESTORE_GPR register, index
    ld \register, TF_GPR + \index * REG_BYTES(sp)
.endm

.section .text.trap,"ax"
.balign 4
.globl __enter_user
.type __enter_user, @function
__enter_user:
    # 消费当前内核调用栈，不返回 Rust 调用者。
    mv sp, a0
    j __trap_return
.size __enter_user, .-__enter_user

.balign 4
.type __trap_return, @function
__trap_return:
    ld t0, TF_SEPC(sp)
    csrw sepc, t0
    ld t0, TF_SSTATUS(sp)
    andi t1, t0, 256 # SPP
    bnez t1, .Lreturn_kernel
    # 用户返回状态由内核生成，不信任保存现场中其余特权控制位。
    li t0, {user_status}
    addi t1, sp, TF_SIZE
    csrw sscratch, t1
    j .Lrestore_status
.Lreturn_kernel:
    li t1, {disabled_status}
    not t1, t1
    and t0, t0, t1
    csrw sscratch, zero
.Lrestore_status:
    csrw sstatus, t0
    # 从设置 sscratch 到 sret 是不可故障的切换窗口：现场必须有效，
    # 中断保持关闭，恢复用户 sp/gp/tp 后不再调用 Rust 或访问用户栈。
    RESTORE_GPR ra,   1
    RESTORE_GPR gp,   3
    RESTORE_GPR tp,   4
    RESTORE_GPR t0,   5
    RESTORE_GPR t1,   6
    RESTORE_GPR t2,   7
    RESTORE_GPR s0,   8
    RESTORE_GPR s1,   9
    RESTORE_GPR a0,  10
    RESTORE_GPR a1,  11
    RESTORE_GPR a2,  12
    RESTORE_GPR a3,  13
    RESTORE_GPR a4,  14
    RESTORE_GPR a5,  15
    RESTORE_GPR a6,  16
    RESTORE_GPR a7,  17
    RESTORE_GPR s2,  18
    RESTORE_GPR s3,  19
    RESTORE_GPR s4,  20
    RESTORE_GPR s5,  21
    RESTORE_GPR s6,  22
    RESTORE_GPR s7,  23
    RESTORE_GPR s8,  24
    RESTORE_GPR s9,  25
    RESTORE_GPR s10, 26
    RESTORE_GPR s11, 27
    RESTORE_GPR t3,  28
    RESTORE_GPR t4,  29
    RESTORE_GPR t5,  30
    RESTORE_GPR t6,  31
    # sp 始终作为现场基址，必须最后恢复；x0 不需要恢复。
    RESTORE_GPR sp,   2
    sret
.size __trap_return, .-__trap_return
.purgem RESTORE_GPR
