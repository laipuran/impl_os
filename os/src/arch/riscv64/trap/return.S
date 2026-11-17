.section .text.trap,"ax"
.option push
.option norvc
.option norelax

# 以 `t6` 作为 `TrapContext` 基址恢复全部通用寄存器，最后一步再覆盖 `t6` 自身。
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

.Lrv_return_kernel:
    # 内核态 Trap 返回前强制清理 `sstatus`（关闭中断与扩展，并置位 `SPP = 1` 确保返回 S-mode）。
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

.Lrv_return_user_trap:
    # 用户态寄存器已直接写入调用方传入的 `TrapContext`，此处只需恢复内核被调用者保存寄存器并返回。
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
    # 将切换栈帧指针写入 `sscratch`。注意：从下条指令起到 `sret` 完成前仍处于 S-mode，
    # 但 `sscratch` 已非零（`entry.S` 会将此期间的 Trap 分流至用户分支），因此紧随其后的
    # `RV_RESTORE_GPRS` 必须仅访问持续有效的 `TrapContext` 内存，绝不能在此过渡窗口内触发可恢复的访存异常。
    RV_RESTORE_GPRS
    sret
.size riscv_run_user, .-riscv_run_user
.option pop
