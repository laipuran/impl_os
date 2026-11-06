.section .text.entry
.globl _start
_start:
    csrci sstatus, 2
    # Initial scaffold uses hart 0 only. SMP support is a later milestone.
    bnez a0, .Lpark
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop
    # 内核 ABI 不使用 TLS；异常入口恢复此固定的内核 tp。
    li tp, 0
    la sp, boot_stack_top
    # 共享入口只接收一个架构启动参数：RV 将 FDT 物理地址放入 a0。
    mv a0, a1
    tail kernel_main
.Lpark:
    wfi
    j .Lpark

.section .bss.stack,"aw",@nobits
.align 12
boot_stack:
    .space 65536
.globl boot_stack_top
boot_stack_top:
