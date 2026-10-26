OUTPUT_ARCH(riscv)
ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x80200000;
    .text : ALIGN(4K) {
        KEEP(*(.text.entry))
        *(.text .text.*)
    } :text
    .rodata : ALIGN(4K) {
        *(.rodata .rodata.* .srodata .srodata.*)
    } :rodata
    .data : ALIGN(4K) {
        PROVIDE(__global_pointer$ = . + 0x800);
        *(.data .data.* .sdata .sdata.*)
    } :data
    .bss (NOLOAD) : ALIGN(4K) {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.* .sbss .sbss.*)
        *(COMMON)
        ebss = .;
    } :data
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}
