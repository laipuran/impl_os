.section .text.trap,"ax"
.balign 4096
.global __loongarch_trap_entry
__loongarch_trap_entry:
    # 先把 $t0 和 $sp 暂存到 SAVE0 (0x30) 与 SAVE1 (0x31)，再根据 PRMD.PPLV 判断异常来源特权级。
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
    # 从 SAVE2 指向的内核切换帧首槽取出 `TrapContext` 指针，直接将用户寄存器写入该结构，绝不触碰用户栈。
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
    # 检查 TLBRERA (0x8a) 最低位 IsTLBR：若为 1，说明当前走的是 TLB 重填入口。
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
    # 清除 TLBRERA 低 2 位（含 IsTLBR 标志）得到 4 字节对齐的异常 PC；
    # 将 TLBRPRMD (0x8f) 的 PWE（bit 4）右移对齐到 PRMD.PWE（bit 3），并在保存的 estat 最高位打上软件重填标记。
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
    # 清零 TLBRERA（清除 IsTLBR），使后续若发生内核同步嵌套异常，其 `ertn` 能按普通异常恢复 ERA/PRMD。
    csrwr $zero, 0x8a
.Ldispatch_context:
    # 至此 SAVE0/SAVE1 及异常 CSR 均已落栈，可安全容忍后续内核处理函数中的同步嵌套异常。
    andi $t0, $t0, 3
    bnez $t0, .Lreturn_user_trap
    csrrd $tp, 0x33
    move $a0, $sp
    bl __loongarch_handle_kernel
    b .Lreturn_kernel

# TLBRENTRY 物理入口：触发 TLB 重填时硬件自动置 `CRMD.DA = 1, PG = 0`（直接地址翻译模式）。
# 先跳到带 `0x9000_0000_0000_0000` 高位前缀的虚拟地址（直接地址模式下硬件忽略物理地址位宽之外的高位），
# 随后再写 CRMD 开启映射模式（`DA = 0, PG = 1`），平滑切回内核 DMW0 窗口。
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
