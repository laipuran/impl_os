.section .text.trap_smoke,"ax"
.balign 4

# Only feature-only single-CPU wrappers use these trusted recovery pointers.
# A damaged restored sp must not prevent restoring the wrapper caller's ABI.
.macro save_probe_abi slot
    addi.d $sp, $sp, -128
    st.d $ra, $sp, 16
    st.d $tp, $sp, 24
    st.d $r21, $sp, 32
    st.d $fp, $sp, 40
    st.d $s0, $sp, 48
    st.d $s1, $sp, 56
    st.d $s2, $sp, 64
    st.d $s3, $sp, 72
    st.d $s4, $sp, 80
    st.d $s5, $sp, 88
    st.d $s6, $sp, 96
    st.d $s7, $sp, 104
    st.d $s8, $sp, 112
    la.local $t0, \slot
    st.d $sp, $t0, 0
.endm

.macro restore_probe_abi slot
    la.local $t0, \slot
    ld.d $sp, $t0, 0
    ld.d $ra, $sp, 16
    ld.d $tp, $sp, 24
    ld.d $r21, $sp, 32
    ld.d $fp, $sp, 40
    ld.d $s0, $sp, 48
    ld.d $s1, $sp, 56
    ld.d $s2, $sp, 64
    ld.d $s3, $sp, 72
    ld.d $s4, $sp, 80
    ld.d $s5, $sp, 88
    ld.d $s6, $sp, 96
    ld.d $s7, $sp, 104
    ld.d $s8, $sp, 112
    addi.d $sp, $sp, 128
    jr $ra
.endm

.macro check_kernel_gpr register, number
    li.w $t0, 0x700 + \number
    bne \register, $t0, .Lkernel_fail
.endm

.macro check_call_gpr register, number
    li.w $t0, 0x800 + \number
    bne \register, $t0, .Lcall_fail
.endm

.global __loongarch_smoke_kernel
.type __loongarch_smoke_kernel, @function
__loongarch_smoke_kernel:
    # Preserve every kernel ABI register that the sentinel test overwrites.
    save_probe_abi .Lkernel_saved_sp
    li.w $r1, 0x601
    li.w $r2, 0x602
    li.w $r4, 0x604
    li.w $r5, 0x605
    li.w $r6, 0x606
    li.w $r7, 0x607
    li.w $r8, 0x608
    li.w $r9, 0x609
    li.w $r10, 0x60a
    li.w $r11, 0x60b
    move $r12, $sp
    li.w $r13, 0x60d
    li.w $r14, 0x60e
    li.w $r15, 0x60f
    li.w $r16, 0x610
    li.w $r17, 0x611
    li.w $r18, 0x612
    li.w $r19, 0x613
    li.w $r20, 0x614
    li.w $r21, 0x615
    li.w $r22, 0x616
    li.w $r23, 0x617
    li.w $r24, 0x618
    li.w $r25, 0x619
    li.w $r26, 0x61a
    li.w $r27, 0x61b
    li.w $r28, 0x61c
    li.w $r29, 0x61d
    li.w $r30, 0x61e
    li.w $r31, 0x61f
.global __loongarch_smoke_kernel_break
__loongarch_smoke_kernel_break:
    break 0
    # Self-check restored t0 before using it as the comparison scratch.
    addi.d $t0, $t0, -0x70c
    bnez $t0, .Lkernel_fail
    check_kernel_gpr $r1, 1
    check_kernel_gpr $r2, 2
    check_kernel_gpr $r4, 4
    check_kernel_gpr $r5, 5
    check_kernel_gpr $r6, 6
    check_kernel_gpr $r7, 7
    check_kernel_gpr $r8, 8
    check_kernel_gpr $r9, 9
    check_kernel_gpr $r10, 10
    check_kernel_gpr $r11, 11
    check_kernel_gpr $r13, 13
    check_kernel_gpr $r14, 14
    check_kernel_gpr $r15, 15
    check_kernel_gpr $r16, 16
    check_kernel_gpr $r17, 17
    check_kernel_gpr $r18, 18
    check_kernel_gpr $r19, 19
    check_kernel_gpr $r20, 20
    check_kernel_gpr $r21, 21
    check_kernel_gpr $r22, 22
    check_kernel_gpr $r23, 23
    check_kernel_gpr $r24, 24
    check_kernel_gpr $r25, 25
    check_kernel_gpr $r26, 26
    check_kernel_gpr $r27, 27
    check_kernel_gpr $r28, 28
    check_kernel_gpr $r29, 29
    check_kernel_gpr $r30, 30
    check_kernel_gpr $r31, 31
    la.local $t0, .Lkernel_saved_sp
    ld.d $t0, $t0, 0
    bne $sp, $t0, .Lkernel_fail
    li.w $a0, 1
    b .Lkernel_restore
.Lkernel_fail:
    move $a0, $zero
.Lkernel_restore:
    # No caller restoration above can mask an incorrectly restored GPR.
    restore_probe_abi .Lkernel_saved_sp
.size __loongarch_smoke_kernel, .-__loongarch_smoke_kernel

.global __loongarch_smoke_call_abi
.type __loongarch_smoke_call_abi, @function
__loongarch_smoke_call_abi:
    save_probe_abi .Lcall_saved_sp
    # tp and reserved r21 must retain their valid platform values across Rust.
    # Only ordinary callee-saved registers receive synthetic sentinels.
    li.w $fp, 0x816
    li.w $s0, 0x817
    li.w $s1, 0x818
    li.w $s2, 0x819
    li.w $s3, 0x81a
    li.w $s4, 0x81b
    li.w $s5, 0x81c
    li.w $s6, 0x81d
    li.w $s7, 0x81e
    li.w $s8, 0x81f
    bl {run_user}
    # Validate before restoring our caller, including the real platform state.
    la.local $t0, .Lcall_saved_sp
    ld.d $t0, $t0, 0
    bne $sp, $t0, .Lcall_fail
    ld.d $t0, $sp, 24
    bne $tp, $t0, .Lcall_fail
    ld.d $t0, $sp, 32
    bne $r21, $t0, .Lcall_fail
    check_call_gpr $fp, 22
    check_call_gpr $s0, 23
    check_call_gpr $s1, 24
    check_call_gpr $s2, 25
    check_call_gpr $s3, 26
    check_call_gpr $s4, 27
    check_call_gpr $s5, 28
    check_call_gpr $s6, 29
    check_call_gpr $s7, 30
    check_call_gpr $s8, 31
    li.w $a0, 1
    b .Lcall_restore
.Lcall_fail:
    move $a0, $zero
.Lcall_restore:
    restore_probe_abi .Lcall_saved_sp
.size __loongarch_smoke_call_abi, .-__loongarch_smoke_call_abi


.global __loongarch_smoke_nested
.global __loongarch_smoke_nested_break
.type __loongarch_smoke_nested, @function
__loongarch_smoke_nested:
__loongarch_smoke_nested_break:
    break 0
    jr $ra
.size __loongarch_smoke_nested, .-__loongarch_smoke_nested

.global __loongarch_smoke_refill
.global __loongarch_smoke_refill_load
.type __loongarch_smoke_refill, @function
__loongarch_smoke_refill:
__loongarch_smoke_refill_load:
    ld.d $zero, $a0, 0
    jr $ra
.size __loongarch_smoke_refill, .-__loongarch_smoke_refill

.global __loongarch_smoke_user_first
__loongarch_smoke_user_first:
    syscall 0
    # The kernel must explicitly replace ERA, not blindly advance into this.
    break 0
.global __loongarch_smoke_user_second
__loongarch_smoke_user_second:
    syscall 0
    break 0
.global __loongarch_smoke_user_privileged
__loongarch_smoke_user_privileged:
    csrrd $zero, 0x0
    break 0
.global __loongarch_smoke_user_float
__loongarch_smoke_user_float:
    fadd.d $fa0, $fa0, $fa0
    break 0

.section .bss.trap_smoke,"aw",@nobits
.balign 8
.Lkernel_saved_sp:
    .space 8
.Lcall_saved_sp:
    .space 8
