.equ TF_SIZE, {frame_size}
.equ REG_BYTES, {register_size}
.equ TF_GPR, {gpr}
.equ TF_SEPC, {sepc}
.equ TF_SSTATUS, {sstatus}
.equ TF_SCAUSE, {scause}
.equ TF_STVAL, {stval}

# Rust 提供布局；索引对应架构定义的 x 寄存器编号。
.macro SAVE_GPR register, index
    sd \register, TF_GPR + \index * REG_BYTES(sp)
.endm

.section .text.trap,"ax"
.balign 4
.globl __trap_entry
.type __trap_entry, @function
__trap_entry:
    # 内核执行时 sscratch=0；用户执行时它是可信内核栈顶。
    # 用户 sp 即使为零也不影响判断：这里判断的是交换后的内核栈顶。
    csrrw sp, sscratch, sp
    bnez sp, .Ltrap_stack_ready
    csrrw sp, sscratch, sp
.Ltrap_stack_ready:
    addi sp, sp, -TF_SIZE
    SAVE_GPR zero, 0
    SAVE_GPR ra,   1
    SAVE_GPR gp,   3
    SAVE_GPR tp,   4
    SAVE_GPR t0,   5
    SAVE_GPR t1,   6
    SAVE_GPR t2,   7
    SAVE_GPR s0,   8
    SAVE_GPR s1,   9
    SAVE_GPR a0,  10
    SAVE_GPR a1,  11
    SAVE_GPR a2,  12
    SAVE_GPR a3,  13
    SAVE_GPR a4,  14
    SAVE_GPR a5,  15
    SAVE_GPR a6,  16
    SAVE_GPR a7,  17
    SAVE_GPR s2,  18
    SAVE_GPR s3,  19
    SAVE_GPR s4,  20
    SAVE_GPR s5,  21
    SAVE_GPR s6,  22
    SAVE_GPR s7,  23
    SAVE_GPR s8,  24
    SAVE_GPR s9,  25
    SAVE_GPR s10, 26
    SAVE_GPR s11, 27
    SAVE_GPR t3,  28
    SAVE_GPR t4,  29
    SAVE_GPR t5,  30
    SAVE_GPR t6,  31
    # 必须根据 SPP 区分来源，不能根据用户 sp 是否为零判断。
    csrr t0, sstatus
    sd t0, TF_SSTATUS(sp)
    andi t0, t0, 256
    beqz t0, .Lsave_user_sp
    addi t0, sp, TF_SIZE
    j .Lsave_original_sp
.Lsave_user_sp:
    csrr t0, sscratch
.Lsave_original_sp:
    SAVE_GPR t0, 2
    # 此后允许 handler 内的可处理同步异常嵌套。
    csrw sscratch, zero
    csrr t0, sepc
    sd t0, TF_SEPC(sp)
    csrr t0, scause
    sd t0, TF_SCAUSE(sp)
    csrr t0, stval
    sd t0, TF_STVAL(sp)
    # 只有保存被打断时的寄存器值后，才重新建立内核 ABI。
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop
    li tp, 0
    mv a0, sp
    call {handler}
    j __trap_return
.size __trap_entry, .-__trap_entry
.purgem SAVE_GPR
