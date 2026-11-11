.section .text.task_test,"ax"
.balign 4
.global task_test_entry
.type task_test_entry, @function
task_test_entry:
    li.d $a7, 0x123
    syscall 0
    li.d $t0, -38
    bne $a0, $t0, .Ltask_failed
    li.d $a7, 0x10000005d
    syscall 0
    li.d $t0, -38
    bne $a0, $t0, .Ltask_failed
    li.d $a0, 0x12345675a
    li.d $a7, 93
.global task_test_exit
task_test_exit:
    syscall 0
    break 0
.Ltask_failed:
    li.d $a0, 0xee
    li.d $a7, 93
    b task_test_exit

.global task_test_group_entry
task_test_group_entry:
    li.d $a0, -2
    li.d $a7, 94
.global task_test_exit_group
task_test_exit_group:
    syscall 0
    break 0

.global task_test_fault
task_test_fault:
    break 0
.size task_test_entry, .-task_test_entry
