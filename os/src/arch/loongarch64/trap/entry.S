.section .text.trap,"ax"
.balign 4096
.global __loongarch_trap_entry
__loongarch_trap_entry:
    # Only t0 and sp are touched before their originals are in SAVE0/SAVE1.
    # PRMD.PPLV, never a user-controlled sp, selects the kernel stack.
    csrwr $t0, 0x30
    csrwr $sp, 0x31
    csrrd $t0, 0x1
.Lselect_stack:
    andi $t0, $t0, 3
    bnez $t0, .Lfrom_user
    csrrd $sp, 0x31
    addi.d $sp, $sp, -{frame_size}
    b .Lsave_context
.Lfrom_user:
    csrrd $sp, 0x32
    # Save directly into the trusted context, never onto the user stack.
    ld.d $sp, $sp, 0
.Lsave_context:
    st.d $zero, $sp, {gpr} + {word} * 0
    st.d $ra, $sp, {gpr} + {word} * 1
    st.d $tp, $sp, {gpr} + {word} * 2
    st.d $a0, $sp, {gpr} + {word} * 4
    st.d $a1, $sp, {gpr} + {word} * 5
    st.d $a2, $sp, {gpr} + {word} * 6
    st.d $a3, $sp, {gpr} + {word} * 7
    st.d $a4, $sp, {gpr} + {word} * 8
    st.d $a5, $sp, {gpr} + {word} * 9
    st.d $a6, $sp, {gpr} + {word} * 10
    st.d $a7, $sp, {gpr} + {word} * 11
    st.d $t1, $sp, {gpr} + {word} * 13
    st.d $t2, $sp, {gpr} + {word} * 14
    st.d $t3, $sp, {gpr} + {word} * 15
    st.d $t4, $sp, {gpr} + {word} * 16
    st.d $t5, $sp, {gpr} + {word} * 17
    st.d $t6, $sp, {gpr} + {word} * 18
    st.d $t7, $sp, {gpr} + {word} * 19
    st.d $t8, $sp, {gpr} + {word} * 20
    st.d $r21, $sp, {gpr} + {word} * 21
    st.d $fp, $sp, {gpr} + {word} * 22
    st.d $s0, $sp, {gpr} + {word} * 23
    st.d $s1, $sp, {gpr} + {word} * 24
    st.d $s2, $sp, {gpr} + {word} * 25
    st.d $s3, $sp, {gpr} + {word} * 26
    st.d $s4, $sp, {gpr} + {word} * 27
    st.d $s5, $sp, {gpr} + {word} * 28
    st.d $s6, $sp, {gpr} + {word} * 29
    st.d $s7, $sp, {gpr} + {word} * 30
    st.d $s8, $sp, {gpr} + {word} * 31
    csrrd $t0, 0x30
    st.d $t0, $sp, {gpr} + {word} * 12
    csrrd $t0, 0x31
    st.d $t0, $sp, {gpr} + {word} * 3
    csrrd $t0, 0x8a
    andi $t1, $t0, 1
    bnez $t1, .Lcapture_refill
    csrrd $t0, 0x6
    st.d $t0, $sp, {era}
    csrrd $t0, 0x1
    st.d $t0, $sp, {prmd}
    csrrd $t1, 0x5
    st.d $t1, $sp, {estat}
    csrrd $t1, 0x7
    st.d $t1, $sp, {badv}
    b .Ldispatch_context
.Lcapture_refill:
    # Dedicated refill CSRs do not overwrite ordinary ERA/PRMD.
    bstrins.d $t0, $zero, 1, 0
    st.d $t0, $sp, {era}
    csrrd $t0, 0x8f
    andi $t1, $t0, 0x10
    srli.d $t1, $t1, 1
    andi $t0, $t0, 7
    or $t0, $t0, $t1
    st.d $t0, $sp, {prmd}
    csrrd $t1, 0x5
    li.d $t2, 0x8000000000000000
    or $t1, $t1, $t2
    st.d $t1, $sp, {estat}
    csrrd $t1, 0x89
    st.d $t1, $sp, {badv}
    # Nested ordinary traps must use their own ERA/PRMD on ERTN.
    csrwr $zero, 0x8a
.Ldispatch_context:
    # All scratch originals and exception CSRs are now frame-local. Nested
    # synchronous kernel traps may freely reuse SAVE0/SAVE1 and ERA/PRMD.
    andi $t0, $t0, 3
    bnez $t0, .Lreturn_user_trap
    csrrd $tp, 0x33
    move $a0, $sp
    bl __loongarch_handle_kernel
    # A kernel handler cannot accidentally return at user privilege.
    ld.d $t0, $sp, {prmd}
    andi $t0, $t0, 8
    st.d $t0, $sp, {prmd}
    b .Lrestore_context

.Lreturn_user_trap:
    # No Rust executes until kernel tp/sp and callee-saved state return.
    csrrd $sp, 0x32
    ld.d $t0, $sp, 8
    csrwr $t0, 0x32
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

.global __loongarch_run_user
.type __loongarch_run_user, @function
__loongarch_run_user:
    addi.d $sp, $sp, -128
    st.d $a0, $sp, 0
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
    csrrd $t0, 0x32
    st.d $t0, $sp, 8
    move $t0, $sp
    csrwr $t0, 0x32
    move $sp, $a0
    b .Lrestore_normal
.Lrestore_context:
    ld.d $t0, $sp, {estat}
    bltz $t0, .Lrestore_refill
.Lrestore_normal:
    csrwr $zero, 0x2
    ld.d $t0, $sp, {era}
    csrwr $t0, 0x6
    ld.d $t0, $sp, {prmd}
    # Never enable interrupts on exception return; PRMD bits above PWE are
    # reserved. run_user additionally forces PPLV=3 and PWE=0 in Rust.
    andi $t0, $t0, 0xb
    csrwr $t0, 0x1
    b .Lrestore_registers
.Lrestore_refill:
    csrwr $zero, 0x2
    ld.d $t0, $sp, {era}
    ori $t0, $t0, 1
    csrwr $t0, 0x8a
    ld.d $t0, $sp, {prmd}
    andi $t0, $t0, 8
    slli.d $t0, $t0, 1
    csrwr $t0, 0x8f
.Lrestore_registers:
    ld.d $ra, $sp, {gpr} + {word} * 1
    ld.d $tp, $sp, {gpr} + {word} * 2
    ld.d $a0, $sp, {gpr} + {word} * 4
    ld.d $a1, $sp, {gpr} + {word} * 5
    ld.d $a2, $sp, {gpr} + {word} * 6
    ld.d $a3, $sp, {gpr} + {word} * 7
    ld.d $a4, $sp, {gpr} + {word} * 8
    ld.d $a5, $sp, {gpr} + {word} * 9
    ld.d $a6, $sp, {gpr} + {word} * 10
    ld.d $a7, $sp, {gpr} + {word} * 11
    ld.d $t0, $sp, {gpr} + {word} * 12
    ld.d $t1, $sp, {gpr} + {word} * 13
    ld.d $t2, $sp, {gpr} + {word} * 14
    ld.d $t3, $sp, {gpr} + {word} * 15
    ld.d $t4, $sp, {gpr} + {word} * 16
    ld.d $t5, $sp, {gpr} + {word} * 17
    ld.d $t6, $sp, {gpr} + {word} * 18
    ld.d $t7, $sp, {gpr} + {word} * 19
    ld.d $t8, $sp, {gpr} + {word} * 20
    ld.d $r21, $sp, {gpr} + {word} * 21
    ld.d $fp, $sp, {gpr} + {word} * 22
    ld.d $s0, $sp, {gpr} + {word} * 23
    ld.d $s1, $sp, {gpr} + {word} * 24
    ld.d $s2, $sp, {gpr} + {word} * 25
    ld.d $s3, $sp, {gpr} + {word} * 26
    ld.d $s4, $sp, {gpr} + {word} * 27
    ld.d $s5, $sp, {gpr} + {word} * 28
    ld.d $s6, $sp, {gpr} + {word} * 29
    ld.d $s7, $sp, {gpr} + {word} * 30
    ld.d $s8, $sp, {gpr} + {word} * 31
    ld.d $sp, $sp, {gpr} + {word} * 3
    ertn
.size __loongarch_run_user, .-__loongarch_run_user

# TLBRENTRY is physical, and refill enters with DA=1/PG=0. Jump to the
# physical alias of a high PC first (direct mode ignores bits above PALEN),
# then enable paging while already executing inside the high kernel DMW.
.balign 4096
.global __loongarch_refill_entry
__loongarch_refill_entry:
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
