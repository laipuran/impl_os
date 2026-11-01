.section .text.trap_smoke,"ax"
.balign 4
.global __loongarch_smoke_kernel
.type __loongarch_smoke_kernel, @function
__loongarch_smoke_kernel:
    # Preserve every kernel ABI register that the sentinel test overwrites.
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
    # Return the handler-edited a0, proving register and ERA restoration.
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
.size __loongarch_smoke_kernel, .-__loongarch_smoke_kernel

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
