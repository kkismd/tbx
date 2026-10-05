.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH -32768
    PUSH 1
failure:
    SUB
    HALT
VM_END
expected_stack: .word $8000, 1
VM_EXPECT 17, failure, 2, 0, expected_stack, 4, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
