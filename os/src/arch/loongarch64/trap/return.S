# 与 entry.S 顺序装配，共享 Rust 提供的现场布局。
.macro RESTORE_GPR register, index
    ld.d \register, $sp, TF_GPR + \index * REG_BYTES
.endm

.section .text.trap,"ax"
.balign 4
.globl __enter_user
.type __enter_user, @function
__enter_user:
    move $sp, $a0
    b __trap_return
.size __enter_user, .-__enter_user

.balign 4
.type __trap_return, @function
__trap_return:
    csrwr $zero, 0x2
    ld.d $t0, $sp, TF_PRMD
    andi $t1, $t0, 3
    beqz $t1, .Lreturn_kernel
    # 不信任用户返回控制位，固定 PPLV=3、PIE/PWE=0。
    li.w $t0, {user_status}
    addi.d $t1, $sp, TF_SIZE
    csrwr $t1, 0x32
    b .Lselect_return
.Lreturn_kernel:
    # PPLV=0、PIE=0，保留内核 PWE；清除保留位。
    andi $t0, $t0, 8
    csrwr $zero, 0x32
.Lselect_return:
    ld.d $t1, $sp, TF_ESTAT
    bltz $t1, .Lreturn_refill
    csrwr $t0, 0x1
    ld.d $t0, $sp, TF_ERA
    csrwr $t0, 0x6
    b .Lrestore_registers
.Lreturn_refill:
    # 只供未来显式处理 refill 后恢复；默认 handler 仅报告并停机。
    andi $t1, $t0, 3
    andi $t0, $t0, 8
    slli.d $t0, $t0, 1
    or $t0, $t0, $t1
    csrwr $t0, 0x8f
    ld.d $t0, $sp, TF_ERA
    ori $t0, $t0, 1
    csrwr $t0, 0x8a
.Lrestore_registers:
    # 恢复窗口不允许故障；不调用 Rust、不访问用户栈，sp 最后恢复。
    RESTORE_GPR $ra, 1
    RESTORE_GPR $tp, 2
    RESTORE_GPR $a0, 4
    RESTORE_GPR $a1, 5
    RESTORE_GPR $a2, 6
    RESTORE_GPR $a3, 7
    RESTORE_GPR $a4, 8
    RESTORE_GPR $a5, 9
    RESTORE_GPR $a6, 10
    RESTORE_GPR $a7, 11
    RESTORE_GPR $t0, 12
    RESTORE_GPR $t1, 13
    RESTORE_GPR $t2, 14
    RESTORE_GPR $t3, 15
    RESTORE_GPR $t4, 16
    RESTORE_GPR $t5, 17
    RESTORE_GPR $t6, 18
    RESTORE_GPR $t7, 19
    RESTORE_GPR $t8, 20
    RESTORE_GPR $r21, 21
    RESTORE_GPR $fp, 22
    RESTORE_GPR $s0, 23
    RESTORE_GPR $s1, 24
    RESTORE_GPR $s2, 25
    RESTORE_GPR $s3, 26
    RESTORE_GPR $s4, 27
    RESTORE_GPR $s5, 28
    RESTORE_GPR $s6, 29
    RESTORE_GPR $s7, 30
    RESTORE_GPR $s8, 31
    RESTORE_GPR $sp, 3
    ertn
.size __trap_return, .-__trap_return
.purgem RESTORE_GPR
