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
    la sp, boot_stack_top
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
