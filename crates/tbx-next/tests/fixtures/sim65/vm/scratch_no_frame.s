.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 77
failure:
    LOAD_SCRATCH 0
    HALT
VM_END
expected_stack: .word 77
VM_EXPECT 14, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
