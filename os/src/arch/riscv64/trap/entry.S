# Rust passes all structure-dependent offsets; x-register slots are XLEN words.
.equ RV_CTX_SIZE, {context_size}
.equ RV_GPR, {gpr}
.equ RV_WORD, {word_size}
.equ RV_PC, {pc}
.equ RV_STATUS, {status}
.equ RV_CAUSE, {cause}
.equ RV_BAD_ADDR, {bad_addr}
# ra/gp/tp/s0-s11, context pointer, one temporary slot, then alignment padding.
.equ RV_SWITCH_SIZE, {switch_size}

.section .text.trap,"ax"
.option push
.option norvc
.option norelax

# sp is captured by each entry path. t0 is saved before it becomes a frame base.
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

# t6 remains the frame base until the last load, independent of the restored sp.
.macro RV_RESTORE_GPRS
    ld ra, (RV_GPR + RV_WORD * 1)(t6)
    ld sp, (RV_GPR + RV_WORD * 2)(t6)
    ld gp, (RV_GPR + RV_WORD * 3)(t6)
    ld tp, (RV_GPR + RV_WORD * 4)(t6)
    ld t0, (RV_GPR + RV_WORD * 5)(t6)
    ld t1, (RV_GPR + RV_WORD * 6)(t6)
    ld t2, (RV_GPR + RV_WORD * 7)(t6)
    ld s0, (RV_GPR + RV_WORD * 8)(t6)
    ld s1, (RV_GPR + RV_WORD * 9)(t6)
    ld a0, (RV_GPR + RV_WORD * 10)(t6)
    ld a1, (RV_GPR + RV_WORD * 11)(t6)
    ld a2, (RV_GPR + RV_WORD * 12)(t6)
    ld a3, (RV_GPR + RV_WORD * 13)(t6)
    ld a4, (RV_GPR + RV_WORD * 14)(t6)
    ld a5, (RV_GPR + RV_WORD * 15)(t6)
    ld a6, (RV_GPR + RV_WORD * 16)(t6)
    ld a7, (RV_GPR + RV_WORD * 17)(t6)
    ld s2, (RV_GPR + RV_WORD * 18)(t6)
    ld s3, (RV_GPR + RV_WORD * 19)(t6)
    ld s4, (RV_GPR + RV_WORD * 20)(t6)
    ld s5, (RV_GPR + RV_WORD * 21)(t6)
    ld s6, (RV_GPR + RV_WORD * 22)(t6)
    ld s7, (RV_GPR + RV_WORD * 23)(t6)
    ld s8, (RV_GPR + RV_WORD * 24)(t6)
    ld s9, (RV_GPR + RV_WORD * 25)(t6)
    ld s10, (RV_GPR + RV_WORD * 26)(t6)
    ld s11, (RV_GPR + RV_WORD * 27)(t6)
    ld t3, (RV_GPR + RV_WORD * 28)(t6)
    ld t4, (RV_GPR + RV_WORD * 29)(t6)
    ld t5, (RV_GPR + RV_WORD * 30)(t6)
    ld t6, (RV_GPR + RV_WORD * 31)(t6)
.endm

.balign 4
.globl riscv_trap_entry
.type riscv_trap_entry, @function
riscv_trap_entry:
    # In S mode scratch is zero. In U mode it is the trusted switch frame.
    csrrw sp, sscratch, sp
    bnez sp, .Lrv_user_trap
    csrrw sp, sscratch, sp
    addi sp, sp, -RV_CTX_SIZE
    sd t0, (RV_GPR + RV_WORD * 5)(sp)
    RV_SAVE_GPRS sp
    addi t0, sp, RV_CTX_SIZE
    sd t0, (RV_GPR + RV_WORD * 2)(sp)
    RV_SAVE_CSRS sp, t0
    # tp is fixed at zero by boot (there is no kernel TLS). Neither ABI base
    # may be taken from an interrupted user context or smoke-test sentinels.
    la gp, __global_pointer$
    li tp, 0
    mv a0, sp
    call {kernel_handler}
    # A nested synchronous trap allocates below this frame: scratch stays zero.
    ld t0, RV_PC(sp)
    csrw sepc, t0
    ld t0, RV_STATUS(sp)
    li t1, {disabled_status}
    not t1, t1
    and t0, t0, t1
    ori t0, t0, 256
    csrw sstatus, t0
    mv t6, sp
    RV_RESTORE_GPRS
    sret

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
    # All user state is now in the caller's context, without a frame copy.
    ld gp, 8(sp)
    ld tp, 16(sp)
    ld ra, 0(sp)
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
    addi sp, sp, RV_SWITCH_SIZE
    ret
.size riscv_trap_entry, .-riscv_trap_entry

.balign 4
.globl riscv_run_user
.type riscv_run_user, @function
riscv_run_user:
    addi sp, sp, -RV_SWITCH_SIZE
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
    sd a0, 120(sp)
    mv t6, a0
    ld t0, RV_PC(t6)
    csrw sepc, t0
    ld t0, RV_STATUS(t6)
    csrw sstatus, t0
    csrw sscratch, sp
    # From here through sret only valid context loads occur. The caller must
    # keep the context mapped; a fault in this short switch window is fatal.
    RV_RESTORE_GPRS
    sret
.size riscv_run_user, .-riscv_run_user
.option pop
