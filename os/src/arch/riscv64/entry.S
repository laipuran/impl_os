.section .text.entry
.globl _start
_start:
    csrci sstatus, 2
    # 初始骨架仅使用 hart 0。SMP 支持属于后续里程碑。
    bnez a0, .Lpark
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop
    # 内核不使用 TLS：Rust 入口和 Trap 处理函数使用相同的固定 tp。
    li tp, 0
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
