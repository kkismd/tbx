.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 77
    CALL body
    HALT
body:
    PUSH -123
    STORE_SCRATCH 7
    PUSH 42
failure:
    STORE_SCRATCH 8
    RET
VM_END
expected_stack: .word 77, 42
expected_frame:
    .word body - _tbx_code_start - 1
    .byte 1, 0
    .res 14, 0
    .word $ff85
VM_EXPECT 27, failure, 2, 1, expected_stack, 4, $ff, 0, expected_frame, 20, 0, 0, 0
