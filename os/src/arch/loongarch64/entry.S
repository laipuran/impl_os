.section .text.entry
.globl _start
_start:
    # 配置 DMW0（CSR 0x180）：虚拟地址以 `0x9` 开头时，在 PLV0 下进行可缓存（CC=1）直接地址映射。
    li.d $t0, 0x9000000000000011
    csrwr $t0, 0x180
    # QEMU 从物理地址进入 `_start`；先用 DMW1（CSR 0x181）临时恒等映射 `0x0` 段，
    # 确保下一条开启分页（PG=1）的指令到跳转入高地址窗口前仍能正常取指。
    li.w $t0, 0x11
    csrwr $t0, 0x181
    # 配置 CRMD（CSR 0x0）：PLV0，关闭中断（IE=0），开启映射地址翻译模式（DA=0, PG=1），指令/数据访问可缓存。
    li.w $t0, 0xb0
    csrwr $t0, 0x0
    la.local $t0, .Lmapped
    li.d $t1, 0x9000000000000000
    or $t0, $t0, $t1
    jirl $zero, $t0, 0
.Lmapped:
    # 已进入 DMW0 高地址窗口，卸载临时的低地址 DMW1 映射。
    csrwr $zero, 0x181
    csrrd $t0, 0x20
    bnez $t0, .Lpark
    la.local $sp, boot_stack_top
    # 内核不使用 TLS，将 `$tp` 清零（后续 `trap::init` 会将其备份到 `SAVE3`）。
    move $tp, $zero
    # QEMU direct boot 通过 `$a2` 传入 EFI 系统表物理地址；将其移入 `$a0` 传给 `kernel_main(boot_arg)`。
    move $a0, $a2
    b kernel_main
.Lpark:
    idle 0
    b .Lpark

.section .bss.stack,"aw",@nobits
.align 12
boot_stack:
    .space 65536
.globl boot_stack_top
boot_stack_top:
