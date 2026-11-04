# 结构相关偏移均由 Rust 传入；整数寄存器槽位宽度为 XLEN。
.equ RV_CTX_SIZE, {context_size}
.equ RV_GPR, {gpr}
.equ RV_WORD, {word_size}
.equ RV_PC, {pc}
.equ RV_STATUS, {status}
.equ RV_CAUSE, {cause}
.equ RV_BAD_ADDR, {bad_addr}
# 依次存放 ra/gp/tp/s0-s11、上下文指针、一个临时槽位及对齐填充。
.equ RV_SWITCH_SIZE, {switch_size}

.section .text.trap,"ax"
.option push
.option norvc
.option norelax

# 各入口路径分别保存 sp；t0 在用作栈帧基址前先保存。
.macro RV_SAVE_GPRS base
    sd zero, (RV_GPR + RV_WORD * 0)(\base)
    sd ra, (RV_GPR + RV_WORD * 1)(\base)
    sd gp, (RV_GPR + RV_WORD * 3)(\base)
    sd tp, (RV_GPR + RV_WORD * 4)(\base)
    sd t1, (RV_GPR + RV_WORD * 6)(\base)
    sd t2, (RV_GPR + RV_WORD * 7)(\base)
    sd s0, (RV_GPR + RV_WORD * 8)(\base)
    sd s1, (RV_GPR + RV_WORD * 9)(\base)
    sd a0, (RV_GPR + RV_WORD * 10)(\base)
    sd a1, (RV_GPR + RV_WORD * 11)(\base)
    sd a2, (RV_GPR + RV_WORD * 12)(\base)
    sd a3, (RV_GPR + RV_WORD * 13)(\base)
    sd a4, (RV_GPR + RV_WORD * 14)(\base)
    sd a5, (RV_GPR + RV_WORD * 15)(\base)
    sd a6, (RV_GPR + RV_WORD * 16)(\base)
    sd a7, (RV_GPR + RV_WORD * 17)(\base)
    sd s2, (RV_GPR + RV_WORD * 18)(\base)
    sd s3, (RV_GPR + RV_WORD * 19)(\base)
    sd s4, (RV_GPR + RV_WORD * 20)(\base)
    sd s5, (RV_GPR + RV_WORD * 21)(\base)
    sd s6, (RV_GPR + RV_WORD * 22)(\base)
    sd s7, (RV_GPR + RV_WORD * 23)(\base)
    sd s8, (RV_GPR + RV_WORD * 24)(\base)
    sd s9, (RV_GPR + RV_WORD * 25)(\base)
    sd s10, (RV_GPR + RV_WORD * 26)(\base)
    sd s11, (RV_GPR + RV_WORD * 27)(\base)
    sd t3, (RV_GPR + RV_WORD * 28)(\base)
    sd t4, (RV_GPR + RV_WORD * 29)(\base)
    sd t5, (RV_GPR + RV_WORD * 30)(\base)
    sd t6, (RV_GPR + RV_WORD * 31)(\base)
.endm

.macro RV_SAVE_CSRS base, temp
    csrr \temp, sepc
    sd \temp, RV_PC(\base)
    csrr \temp, sstatus
    sd \temp, RV_STATUS(\base)
    csrr \temp, scause
    sd \temp, RV_CAUSE(\base)
    csrr \temp, stval
    sd \temp, RV_BAD_ADDR(\base)
.endm

.balign 4
.globl riscv_trap_entry
.type riscv_trap_entry, @function
riscv_trap_entry:
    # S 模式下 sscratch 为零；U 模式下指向可信的切换栈帧。
    csrrw sp, sscratch, sp
    bnez sp, .Lrv_user_trap
    csrrw sp, sscratch, sp
    addi sp, sp, -RV_CTX_SIZE
    sd t0, (RV_GPR + RV_WORD * 5)(sp)
    RV_SAVE_GPRS sp
    addi t0, sp, RV_CTX_SIZE
    sd t0, (RV_GPR + RV_WORD * 2)(sp)
    RV_SAVE_CSRS sp, t0
    # 启动时将 tp 固定为零（内核不使用线程局部存储）；这两个 ABI 基址
    # 都不能取自被中断的用户上下文或内核测试哨兵值。
    la gp, __global_pointer$
    li tp, 0
    mv a0, sp
    call {kernel_handler}
    j .Lrv_return_kernel

.Lrv_user_trap:
    sd t0, 128(sp)
    ld t0, 120(sp)
    RV_SAVE_GPRS t0
    csrr t1, sscratch
    sd t1, (RV_GPR + RV_WORD * 2)(t0)
    csrw sscratch, zero
    ld t1, 128(sp)
    sd t1, (RV_GPR + RV_WORD * 5)(t0)
    RV_SAVE_CSRS t0, t1
    j .Lrv_return_user_trap
.size riscv_trap_entry, .-riscv_trap_entry
.option pop
