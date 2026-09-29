.include "vm_fixture.inc"
VM_HEADER entry, 0, 256, $fff8
entry:
    PUSH 1
failure:
    LOAD_ARRAY 2
    HALT
VM_END
expected_stack: .word 1
VM_EXPECT 20, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, 0, 0, 0
