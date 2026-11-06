.section .text.syscall_test,"ax"
.option push
.option norvc
.balign 4
.globl syscall_test_entry
.type syscall_test_entry, @function
syscall_test_entry:
    li a7, 0x100000123
    li a0, -1
    li a1, 0x102
    li a2, 0x203
    li a3, 0x304
    li a4, 0x405
    li a5, 0x8000000000000000
    li a6, 0x606
.globl syscall_test_first
syscall_test_first:
    ecall
    mv a5, a0
    li a0, -1
    li a7, 0x456
.globl syscall_test_second
syscall_test_second:
    ecall
    mv a5, a0
    li a0, -1
    li a7, 0x789
.globl syscall_test_third
syscall_test_third:
    ecall
.globl syscall_test_done
syscall_test_done:
    ebreak
.size syscall_test_entry, .-syscall_test_entry
.option pop
