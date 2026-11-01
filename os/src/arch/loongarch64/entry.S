.section .text.entry
.globl _start
_start:
    # PLV0, cached direct-map window for virtual addresses starting with 0x9.
    li.d $t0, 0x9000000000000011
    csrwr $t0, 0x180
    # QEMU enters at the physical address. Temporarily map segment 0 as well,
    # so enabling paging does not fault before we jump into the high window.
    li.w $t0, 0x11
    csrwr $t0, 0x181
    # CRMD: PLV0, interrupts off, paging on, cached data/instruction accesses.
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
    # No TLS is installed; trap initialization retains this kernel ABI tp.
    move $tp, $zero
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
