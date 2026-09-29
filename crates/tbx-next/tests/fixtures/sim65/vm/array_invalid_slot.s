.include "vm_fixture.inc"
VM_HEADER entry, 0, 1, descriptors
entry:
    PUSH 0
failure:
    LOAD_ARRAY 1
    HALT
VM_END
.segment "RODATA"
descriptors: .word storage, 1
expected_stack: .word 0
.segment "DATA"
storage: .word 321
expected: .word 321
VM_EXPECT 20, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, storage, expected, 2
