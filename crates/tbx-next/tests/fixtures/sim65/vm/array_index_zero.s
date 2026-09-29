.include "vm_fixture.inc"
VM_HEADER entry, 0, 1, descriptors
entry:
    PUSH 0
failure:
    LOAD_ARRAY 0
    HALT
VM_END
.segment "RODATA"
descriptors: .word storage, 2
expected_stack: .word 0
.segment "DATA"
storage: .word 321, 654
expected: .word 321, 654
VM_EXPECT 21, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, storage, expected, 4
