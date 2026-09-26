.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 7
failure:
    .byte $30, $ff, $ff
    HALT
VM_END
expected_stack: .word 7
VM_EXPECT 11, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0
