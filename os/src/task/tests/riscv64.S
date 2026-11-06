.section .text.task_test,"ax"
.option push
.option norvc
.balign 4
.globl task_test_entry
.type task_test_entry, @function
task_test_entry:
    li a7, 0x123
    ecall
    li t0, -38
    bne a0, t0, .Ltask_failed
    li a7, 0x10000005d
    ecall
    li t0, -38
    bne a0, t0, .Ltask_failed
    li a0, 0x12345675a
    li a7, 93
.globl task_test_exit
task_test_exit:
    ecall
    ebreak
.Ltask_failed:
    li a0, 0xee
    li a7, 93
    j task_test_exit

.globl task_test_group_entry
task_test_group_entry:
    li a0, -2
    li a7, 94
.globl task_test_exit_group
task_test_exit_group:
    ecall
    ebreak

.globl task_test_fault
task_test_fault:
    ebreak
.size task_test_entry, .-task_test_entry
.option pop
