.section .text.trap,"ax"
.Lreturn_kernel:
    # 防止内核处理函数意外以用户特权级返回。
    ld.d $t0, $sp, {prmd}
    andi $t0, $t0, 8
    st.d $t0, $sp, {prmd}
    b .Lrestore_context

.Lreturn_user_trap:
    # 恢复内核 tp/sp 及被调用者保存寄存器之前，不执行 Rust 代码。
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
    # 异常返回时禁止开启中断；PRMD 中高于 PWE 的位均为保留位。
    # run_user 还会在 Rust 中强制设置 PPLV=3、PWE=0。
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
