.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH -32768
failure:
    ABS
    HALT
VM_END
expected_stack: .word $8000
VM_EXPECT 17, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
