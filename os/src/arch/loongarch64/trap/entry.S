.section .text.trap,"ax"
.balign 4096
.global __loongarch_trap_entry
__loongarch_trap_entry:
    # 原值保存到 SAVE0/SAVE1 前，仅修改 t0 和 sp。
    # 根据 PRMD.PPLV 选择内核栈，绝不信任用户控制的 sp。
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
    # 直接保存到可信上下文，绝不写入用户栈。
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
    # 重填专用 CSR 不会覆盖普通 ERA/PRMD。
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
    # 嵌套的普通陷阱必须使用自身的 ERA/PRMD 执行 ERTN。
    csrwr $zero, 0x8a
.Ldispatch_context:
    # 所有暂存寄存器原值和异常 CSR 均已保存到当前栈帧。嵌套的
    # 内核同步陷阱可安全复用 SAVE0/SAVE1 和 ERA/PRMD。
    andi $t0, $t0, 3
    bnez $t0, .Lreturn_user_trap
    csrrd $tp, 0x33
    move $a0, $sp
    bl __loongarch_handle_kernel
    b .Lreturn_kernel

# TLBRENTRY 使用物理地址，重填入口处 DA=1、PG=0。先跳转到高位 PC
# 的物理别名（直接地址模式忽略 PALEN 以上的位），再在高位内核
# 直接映射窗口内执行时开启分页。
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
