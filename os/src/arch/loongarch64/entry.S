.section .text.entry
.globl _start
_start:
    # PLV0，为以 0x9 开头的虚拟地址建立带缓存的直接映射窗口。
    li.d $t0, 0x9000000000000011
    csrwr $t0, 0x180
    # QEMU 从物理地址进入。同时临时映射第 0 段，
    # 避免开启分页后、跳入高地址窗口前发生异常。
    li.w $t0, 0x11
    csrwr $t0, 0x181
    # CRMD：PLV0、关闭中断、开启分页，数据和指令访问使用缓存。
    li.w $t0, 0xb0
    csrwr $t0, 0x0
    la.local $t0, .Lmapped
    li.d $t1, 0x9000000000000000
    or $t0, $t0, $t1
    jirl $zero, $t0, 0
.Lmapped:
    csrwr $zero, 0x181
    csrrd $t0, 0x20
    bnez $t0, .Lpark
    la.local $sp, boot_stack_top
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
