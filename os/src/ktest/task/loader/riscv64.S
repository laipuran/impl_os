.pushsection .rodata.loader_probe, "a", @progbits

.option push
.option norelax
.option norvc

.equ LOADER_PROBE_WORD_SIZE, 8
.equ LOADER_PROBE_STACK_ALIGN, 16
.equ LOADER_PROBE_STACK_ARGC, 0
.equ LOADER_PROBE_STACK_ARGV0, LOADER_PROBE_STACK_ARGC + LOADER_PROBE_WORD_SIZE
.equ LOADER_PROBE_STACK_ARGV_NULL, LOADER_PROBE_STACK_ARGV0 + LOADER_PROBE_WORD_SIZE
.equ LOADER_PROBE_STACK_ENVP0, LOADER_PROBE_STACK_ARGV_NULL + LOADER_PROBE_WORD_SIZE
.equ LOADER_PROBE_STACK_ENVP_NULL, LOADER_PROBE_STACK_ENVP0 + LOADER_PROBE_WORD_SIZE
.equ LOADER_PROBE_STACK_AUXV, LOADER_PROBE_STACK_ENVP_NULL + LOADER_PROBE_WORD_SIZE
.equ LOADER_PROBE_AUXV_ENTRY_SIZE, 2 * LOADER_PROBE_WORD_SIZE
.equ LOADER_PROBE_SYS_EXIT, 93
.equ LOADER_PROBE_WRITE_TEST_BYTE, 0x5a

.equ LOADER_PROBE_SEEN_ENTRY, 1 << 0
.equ LOADER_PROBE_SEEN_PAGESZ, 1 << 1
.equ LOADER_PROBE_SEEN_PHDR, 1 << 2
.equ LOADER_PROBE_SEEN_PHENT, 1 << 3
.equ LOADER_PROBE_SEEN_PHNUM, 1 << 4
.equ LOADER_PROBE_SEEN_PHDR_GROUP, LOADER_PROBE_SEEN_PHDR | LOADER_PROBE_SEEN_PHENT | LOADER_PROBE_SEEN_PHNUM

.equ LOADER_PROBE_BAD_STACK_ALIGNMENT, 1
.equ LOADER_PROBE_BAD_ARGC, 2
.equ LOADER_PROBE_BAD_ARGV_ADDRESS, 3
.equ LOADER_PROBE_BAD_ARGV_STRING, 4
.equ LOADER_PROBE_BAD_ARGV_NULL, 5
.equ LOADER_PROBE_BAD_ENVP_ADDRESS, 6
.equ LOADER_PROBE_BAD_ENVP_STRING, 7
.equ LOADER_PROBE_BAD_ENVP_NULL, 8
.equ LOADER_PROBE_UNKNOWN_AUX_KEY, 9
.equ LOADER_PROBE_DUPLICATE_AUX_KEY, 10
.equ LOADER_PROBE_BAD_AT_ENTRY, 11
.equ LOADER_PROBE_BAD_AT_PAGESZ, 12
.equ LOADER_PROBE_BAD_AT_PHDR, 13
.equ LOADER_PROBE_BAD_AT_PHENT, 14
.equ LOADER_PROBE_BAD_AT_PHNUM, 15
.equ LOADER_PROBE_BAD_AT_NULL, 16
.equ LOADER_PROBE_MISSING_AT_ENTRY, 17
.equ LOADER_PROBE_MISSING_AT_PAGESZ, 18
.equ LOADER_PROBE_BAD_PHDR_GROUP, 19
.equ LOADER_PROBE_BAD_PHDR_TYPE, 20
.equ LOADER_PROBE_BAD_PHDR_FLAGS, 21
.equ LOADER_PROBE_BAD_PAGE_HEAD, 22
.equ LOADER_PROBE_BAD_FILE_BYTES, 23
.equ LOADER_PROBE_BAD_BSS, 24
.equ LOADER_PROBE_BAD_PAGE_TAIL, 25
.equ LOADER_PROBE_BAD_STORE, 26

.macro loader_probe_check_byte offset, expected
    lbu t1, \offset(t0)
    li t2, \expected
    bne t1, t2, .Lloader_probe_exit
.endm

.macro loader_probe_check_aux bit, expected, failure
    li a0, LOADER_PROBE_DUPLICATE_AUX_KEY
    andi t3, s3, \bit
    bnez t3, .Lloader_probe_exit
    li a0, \failure
    bne t2, \expected, .Lloader_probe_exit
    ori s3, s3, \bit
    j .Lloader_probe_next_aux
.endm

# 从 t0 扫描到给定的半开区间末址，覆盖区间内的每个字节。
.macro loader_probe_check_zero_until end, failure
    li a0, \failure
    li t1, \end
1:
    lbu t2, 0(t0)
    bnez t2, .Lloader_probe_exit
    addi t0, t0, 1
    bne t0, t1, 1b
.endm

.balign 4
.global loader_probe_with_phdr
loader_probe_with_phdr:
    auipc s1, 0
    li s0, 1
    j .Lloader_probe_stack

.balign 4
.global loader_probe_without_phdr
loader_probe_without_phdr:
    auipc s1, 0
    li s0, 0

.Lloader_probe_stack:
    li a0, LOADER_PROBE_BAD_STACK_ALIGNMENT
    andi t0, sp, LOADER_PROBE_STACK_ALIGN - 1
    bnez t0, .Lloader_probe_exit
    li a0, LOADER_PROBE_BAD_ARGC
    ld t0, LOADER_PROBE_STACK_ARGC(sp)
    li t1, 1
    bne t0, t1, .Lloader_probe_exit

    li a0, LOADER_PROBE_BAD_ARGV_ADDRESS
    ld t0, LOADER_PROBE_STACK_ARGV0(sp)
    li t1, {argv_address}
    bne t0, t1, .Lloader_probe_exit
    # argv[0] 刻意位于奇地址，只用 lbu 读取字符串。
    li a0, LOADER_PROBE_BAD_ARGV_STRING
    loader_probe_check_byte 0, 'p'
    loader_probe_check_byte 1, 'r'
    loader_probe_check_byte 2, 'o'
    loader_probe_check_byte 3, 'g'
    loader_probe_check_byte 4, 0
    li a0, LOADER_PROBE_BAD_ARGV_NULL
    ld t0, LOADER_PROBE_STACK_ARGV_NULL(sp)
    bnez t0, .Lloader_probe_exit

    li a0, LOADER_PROBE_BAD_ENVP_ADDRESS
    ld t0, LOADER_PROBE_STACK_ENVP0(sp)
    li t1, {envp_address}
    bne t0, t1, .Lloader_probe_exit
    li a0, LOADER_PROBE_BAD_ENVP_STRING
    loader_probe_check_byte 0, 'K'
    loader_probe_check_byte 1, '='
    loader_probe_check_byte 2, 'V'
    loader_probe_check_byte 3, 0
    li a0, LOADER_PROBE_BAD_ENVP_NULL
    ld t0, LOADER_PROBE_STACK_ENVP_NULL(sp)
    bnez t0, .Lloader_probe_exit

    addi s2, sp, LOADER_PROBE_STACK_AUXV
    li s3, 0
.Lloader_probe_scan_aux:
    ld t0, 0(s2)
    ld t2, LOADER_PROBE_WORD_SIZE(s2)
    beqz t0, .Lloader_probe_aux_end # AT_NULL
    li t1, {at_entry}
    beq t0, t1, .Lloader_probe_aux_entry
    li t1, {at_pagesz}
    beq t0, t1, .Lloader_probe_aux_pagesz
    li t1, {at_phdr}
    beq t0, t1, .Lloader_probe_aux_phdr
    li t1, {at_phent}
    beq t0, t1, .Lloader_probe_aux_phent
    li t1, {at_phnum}
    beq t0, t1, .Lloader_probe_aux_phnum
    li a0, LOADER_PROBE_UNKNOWN_AUX_KEY
    j .Lloader_probe_exit
.Lloader_probe_next_aux:
    addi s2, s2, LOADER_PROBE_AUXV_ENTRY_SIZE
    j .Lloader_probe_scan_aux

.Lloader_probe_aux_entry:
    loader_probe_check_aux LOADER_PROBE_SEEN_ENTRY, s1, LOADER_PROBE_BAD_AT_ENTRY
.Lloader_probe_aux_pagesz:
    li t1, {page_size}
    loader_probe_check_aux LOADER_PROBE_SEEN_PAGESZ, t1, LOADER_PROBE_BAD_AT_PAGESZ
.Lloader_probe_aux_phdr:
    mv s4, t2
    li t1, {phdr_address}
    loader_probe_check_aux LOADER_PROBE_SEEN_PHDR, t1, LOADER_PROBE_BAD_AT_PHDR
.Lloader_probe_aux_phent:
    li t1, {phent_size}
    loader_probe_check_aux LOADER_PROBE_SEEN_PHENT, t1, LOADER_PROBE_BAD_AT_PHENT
.Lloader_probe_aux_phnum:
    li t1, {phnum}
    loader_probe_check_aux LOADER_PROBE_SEEN_PHNUM, t1, LOADER_PROBE_BAD_AT_PHNUM

.Lloader_probe_aux_end:
    li a0, LOADER_PROBE_BAD_AT_NULL
    bnez t2, .Lloader_probe_exit
    li a0, LOADER_PROBE_MISSING_AT_ENTRY
    andi t0, s3, LOADER_PROBE_SEEN_ENTRY
    beqz t0, .Lloader_probe_exit
    li a0, LOADER_PROBE_MISSING_AT_PAGESZ
    andi t0, s3, LOADER_PROBE_SEEN_PAGESZ
    beqz t0, .Lloader_probe_exit

    li a0, LOADER_PROBE_BAD_PHDR_GROUP
    andi t0, s3, LOADER_PROBE_SEEN_PHDR_GROUP
    beqz s0, .Lloader_probe_no_phdr
    li t1, LOADER_PROBE_SEEN_PHDR_GROUP
    bne t0, t1, .Lloader_probe_exit
    # 只有三项齐全且指针正确时才从用户态读取首条程序头。
    li a0, LOADER_PROBE_BAD_PHDR_TYPE
    lwu t0, 0(s4)
    li t1, {pt_load}
    bne t0, t1, .Lloader_probe_exit
    li a0, LOADER_PROBE_BAD_PHDR_FLAGS
    lwu t0, {ph_flags}(s4)
    li t1, {text_flags}
    bne t0, t1, .Lloader_probe_exit
    j .Lloader_probe_data_head
.Lloader_probe_no_phdr:
    bnez t0, .Lloader_probe_exit

.Lloader_probe_data_head:
    li t0, {data_page}
    loader_probe_check_zero_until {data_address}, LOADER_PROBE_BAD_PAGE_HEAD

    li a0, LOADER_PROBE_BAD_FILE_BYTES
    li t1, {data_file_end}
    li t3, 0
.Lloader_probe_file_bytes:
    lbu t2, 0(t0)
    andi t4, t3, {pattern_mask}
    addi t4, t4, 1
    bne t2, t4, .Lloader_probe_exit
    addi t0, t0, 1
    addi t3, t3, 1
    bne t0, t1, .Lloader_probe_file_bytes

    loader_probe_check_zero_until {data_memory_end}, LOADER_PROBE_BAD_BSS
    loader_probe_check_zero_until {data_pages_end}, LOADER_PROBE_BAD_PAGE_TAIL

    li a0, LOADER_PROBE_BAD_STORE
    li t0, {data_address}
    li t1, LOADER_PROBE_WRITE_TEST_BYTE
    sb t1, 0(t0)
    lbu t2, 0(t0)
    bne t1, t2, .Lloader_probe_exit

    li a0, 0
.Lloader_probe_exit:
    li a7, LOADER_PROBE_SYS_EXIT
    ecall
    unimp

.global loader_probe_end
loader_probe_end:
.option pop
.popsection

.purgem loader_probe_check_byte
.purgem loader_probe_check_aux
.purgem loader_probe_check_zero_until
