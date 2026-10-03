.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 77
    CALL body
    HALT
body:
    PUSH -123
    STORE_SCRATCH 7
failure:
    .byte $14
VM_END
expected_stack: .word 77
expected_frame:
    .word body - _tbx_code_start - 1
    .byte 1, 0
    .res 14, 0
    .word $ff85
VM_EXPECT 11, failure, 1, 1, expected_stack, 2, $ff, 0, expected_frame, 20, 0, 0, 0
