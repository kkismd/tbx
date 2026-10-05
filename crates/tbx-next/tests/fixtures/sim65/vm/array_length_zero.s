.include "vm_fixture.inc"
VM_HEADER entry, 0, 1, descriptors
entry:
    PUSH 1
failure:
    LOAD_ARRAY 0
    HALT
VM_END
.segment "RODATA"
descriptors: .word storage, 0
expected_stack: .word 1
.segment "DATA"
storage: .word 321
expected: .word 321
VM_EXPECT 20, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, storage, expected, 2, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
