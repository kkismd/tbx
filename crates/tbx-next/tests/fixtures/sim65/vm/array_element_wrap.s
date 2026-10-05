.include "vm_fixture.inc"
VM_HEADER entry, 0, 1, descriptors
entry:
    PUSH 2
failure:
    LOAD_ARRAY 0
    HALT
VM_END
.segment "RODATA"
descriptors: .word $fffe, 2
expected_stack: .word 2
VM_EXPECT 20, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
