.section .text.syscall_test,"ax"
.balign 4
.global syscall_test_entry
.type syscall_test_entry, @function
syscall_test_entry:
    li.d $a7, 0x100000123
    li.d $a0, -1
    li.d $a1, 0x102
    li.d $a2, 0x203
    li.d $a3, 0x304
    li.d $a4, 0x405
    li.d $a5, 0x8000000000000000
    li.d $a6, 0x606
.global syscall_test_first
syscall_test_first:
    syscall 0
    move $a5, $a0
    li.d $a0, -1
    li.d $a7, 0x456
.global syscall_test_second
syscall_test_second:
    syscall 0
    move $a5, $a0
    li.d $a0, -1
    li.d $a7, 0x789
.global syscall_test_third
syscall_test_third:
    syscall 0
.global syscall_test_done
syscall_test_done:
    break 0
.size syscall_test_entry, .-syscall_test_entry
