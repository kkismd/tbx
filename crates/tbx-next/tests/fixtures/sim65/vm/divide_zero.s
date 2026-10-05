.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 7
    PUSH 0
failure:
    DIV
    HALT
VM_END
expected_stack: .word 7, 0
VM_EXPECT 17, failure, 2, 0, expected_stack, 4, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
