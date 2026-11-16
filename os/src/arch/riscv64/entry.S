.section .text.entry
.globl _start
_start:
    csrci sstatus, 2
    # 当前仅使用 hart 0 启动内核，其余从核直接进入 `.Lpark` 休眠等待。
    bnez a0, .Lpark
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop
    # 内核不使用线程局部存储（TLS），将 `tp` 固定清零（Trap 入口也会恢复此约定）。
    li tp, 0
    la sp, boot_stack_top
    # OpenSBI 通过 `a1` 传入 FDT 物理地址；将其移入 `a0` 作为 `kernel_main(boot_arg)` 的唯一参数。
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
