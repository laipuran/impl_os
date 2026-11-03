# Rust passes all structure-dependent offsets; x-register slots are XLEN words.
.equ RV_CTX_SIZE, {context_size}
.equ RV_PC, {pc}
.equ RV_STATUS, {status}
.equ RV_CAUSE, {cause}
.equ RV_BAD_ADDR, {bad_addr}
# ra/gp/tp/s0-s11, context pointer, one temporary slot, then alignment padding.
.equ RV_SWITCH_SIZE, 144

.section .text.trap,"ax"
.option push
.option norvc
.option norelax

# sp is captured by each entry path. t0 is saved before it becomes a frame base.
.macro RV_SAVE_GPRS base
    sd zero, 0(\base)
    sd ra, 8(\base)
    sd gp, 24(\base)
    sd tp, 32(\base)
    sd t1, 48(\base)
    sd t2, 56(\base)
    sd s0, 64(\base)
    sd s1, 72(\base)
    sd a0, 80(\base)
    sd a1, 88(\base)
    sd a2, 96(\base)
    sd a3, 104(\base)
    sd a4, 112(\base)
    sd a5, 120(\base)
    sd a6, 128(\base)
    sd a7, 136(\base)
    sd s2, 144(\base)
    sd s3, 152(\base)
    sd s4, 160(\base)
    sd s5, 168(\base)
    sd s6, 176(\base)
    sd s7, 184(\base)
    sd s8, 192(\base)
    sd s9, 200(\base)
    sd s10, 208(\base)
    sd s11, 216(\base)
    sd t3, 224(\base)
    sd t4, 232(\base)
    sd t5, 240(\base)
    sd t6, 248(\base)
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
    ld ra, 8(t6)
    ld sp, 16(t6)
    ld gp, 24(t6)
    ld tp, 32(t6)
    ld t0, 40(t6)
    ld t1, 48(t6)
    ld t2, 56(t6)
    ld s0, 64(t6)
    ld s1, 72(t6)
    ld a0, 80(t6)
    ld a1, 88(t6)
    ld a2, 96(t6)
    ld a3, 104(t6)
    ld a4, 112(t6)
    ld a5, 120(t6)
    ld a6, 128(t6)
    ld a7, 136(t6)
    ld s2, 144(t6)
    ld s3, 152(t6)
    ld s4, 160(t6)
    ld s5, 168(t6)
    ld s6, 176(t6)
    ld s7, 184(t6)
    ld s8, 192(t6)
    ld s9, 200(t6)
    ld s10, 208(t6)
    ld s11, 216(t6)
    ld t3, 224(t6)
    ld t4, 232(t6)
    ld t5, 240(t6)
    ld t6, 248(t6)
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
    sd t0, 40(sp)
    RV_SAVE_GPRS sp
    addi t0, sp, RV_CTX_SIZE
    sd t0, 16(sp)
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
    sd t1, 16(t0)
    csrw sscratch, zero
    ld t1, 128(sp)
    sd t1, 40(t0)
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
