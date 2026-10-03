.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    CALL body
    HALT
body:
    PUSH -123
    STORE_SCRATCH 7
failure:
    STORE_SCRATCH 0
    RET
VM_END
expected_frame:
    .word body - _tbx_code_start - 1
    .byte 0, 0
    .res 14, 0
    .word $ff85
VM_EXPECT 12, failure, 0, 1, 0, 0, $ff, 0, expected_frame, 20, 0, 0, 0
