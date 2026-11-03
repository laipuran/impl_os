.equ TF_SIZE, {frame_size}
.equ REG_BYTES, {register_size}
.equ TF_GPR, {gpr}
.equ TF_ERA, {era}
.equ TF_PRMD, {prmd}
.equ TF_ESTAT, {estat}
.equ TF_BADV, {badv}

.macro SAVE_GPR register, index
    st.d \register, $sp, TF_GPR + \index * REG_BYTES
.endm

.section .text.trap,"ax"
.balign 4096
.globl __trap_entry
.type __trap_entry, @function
__trap_entry:
    # csrwr 将原 CSR 值写回寄存器，原 t0/sp 已分别留在 SAVE0/1。
    csrwr $t0, 0x30
    csrwr $sp, 0x31
    csrrd $t0, 0x1
.Lselect_stack:
    andi $t0, $t0, 3
    bnez $t0, .Lfrom_user
    csrrd $sp, 0x31
    b .Lstack_ready
.Lfrom_user:
    csrrd $sp, 0x32
.Lstack_ready:
    addi.d $sp, $sp, -TF_SIZE
    SAVE_GPR $zero, 0
    SAVE_GPR $ra, 1
    SAVE_GPR $tp, 2
    SAVE_GPR $a0, 4
    SAVE_GPR $a1, 5
    SAVE_GPR $a2, 6
    SAVE_GPR $a3, 7
    SAVE_GPR $a4, 8
    SAVE_GPR $a5, 9
    SAVE_GPR $a6, 10
    SAVE_GPR $a7, 11
    SAVE_GPR $t1, 13
    SAVE_GPR $t2, 14
    SAVE_GPR $t3, 15
    SAVE_GPR $t4, 16
    SAVE_GPR $t5, 17
    SAVE_GPR $t6, 18
    SAVE_GPR $t7, 19
    SAVE_GPR $t8, 20
    SAVE_GPR $r21, 21
    SAVE_GPR $fp, 22
    SAVE_GPR $s0, 23
    SAVE_GPR $s1, 24
    SAVE_GPR $s2, 25
    SAVE_GPR $s3, 26
    SAVE_GPR $s4, 27
    SAVE_GPR $s5, 28
    SAVE_GPR $s6, 29
    SAVE_GPR $s7, 30
    SAVE_GPR $s8, 31
    csrrd $t0, 0x30
    SAVE_GPR $t0, 12
    csrrd $t0, 0x31
    SAVE_GPR $t0, 3
    # refill 使用独立 CSR，不能把普通 ERA/PRMD 当成它的现场。
    csrrd $t0, 0x8a
    andi $t1, $t0, 1
    bnez $t1, .Lcapture_refill
    csrrd $t0, 0x6
    st.d $t0, $sp, TF_ERA
    csrrd $t0, 0x1
    st.d $t0, $sp, TF_PRMD
    csrrd $t0, 0x5
    st.d $t0, $sp, TF_ESTAT
    csrrd $t0, 0x7
    st.d $t0, $sp, TF_BADV
    b .Ldispatch
.Lcapture_refill:
    bstrins.d $t0, $zero, 1, 0
    st.d $t0, $sp, TF_ERA
    csrrd $t0, 0x8f
    andi $t1, $t0, 0x10
    srli.d $t1, $t1, 1
    andi $t0, $t0, 7
    or $t0, $t0, $t1
    st.d $t0, $sp, TF_PRMD
    csrrd $t0, 0x5
    li.d $t1, {refill}
    or $t0, $t0, $t1
    st.d $t0, $sp, TF_ESTAT
    csrrd $t0, 0x89
    st.d $t0, $sp, TF_BADV
    # 清 IsTLBR 后，handler 中的普通嵌套异常可独立使用 ERA/PRMD。
    csrwr $zero, 0x8a
.Ldispatch:
    # SAVE0/1 的原值及全部异常状态已进入本层现场，此后允许同步嵌套。
    move $tp, $zero
    move $a0, $sp
    bl {handler}
    b __trap_return
.size __trap_entry, .-__trap_entry

# TLBRENTRY 是物理地址；refill 进入 DA=1/PG=0。先跳到高地址别名，
# 再恢复内核分页/DMW 状态，避免在未映射的低地址开启分页。
.balign 4096
.globl __tlb_refill_entry
.type __tlb_refill_entry, @function
__tlb_refill_entry:
    csrwr $t0, 0x30
    csrwr $sp, 0x31
    la.local $t0, .Lrefill_high
    li.d $sp, 0x9000000000000000
    or $t0, $t0, $sp
    jr $t0
.Lrefill_high:
    li.w $t0, 0xb0
    csrwr $t0, 0x0
    csrrd $t0, 0x8f
    b .Lselect_stack
.size __tlb_refill_entry, .-__tlb_refill_entry
.purgem SAVE_GPR
