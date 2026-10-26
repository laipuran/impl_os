OUTPUT_ARCH(loongarch)
ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x9000000090000000;
    .text : AT(ADDR(.text) - 0x9000000000000000) {
        KEEP(*(.text.entry))
        *(.text .text.*)
    } :text
    . = ALIGN(4K);
    .rodata : AT(ADDR(.rodata) - 0x9000000000000000) {
        *(.rodata .rodata.* .srodata .srodata.*)
    } :rodata
    . = ALIGN(4K);
    .data : AT(ADDR(.data) - 0x9000000000000000) {
        *(.data .data.* .sdata .sdata.* .got .got.*)
    } :data
    . = ALIGN(4K);
    .bss (NOLOAD) : AT(ADDR(.bss) - 0x9000000000000000) {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.* .sbss .sbss.*)
        *(COMMON)
        ebss = .;
    } :data
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}
