.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 77
    CALL body
    HALT
body:
    PUSH -123
    STORE_SCRATCH 7
    .repeat 63
        PUSH 0
    .endrepeat
failure:
    LOAD_SCRATCH 7
    RET
VM_END
expected_stack:
    .word 77
    .repeat 63
        .word 0
    .endrepeat
expected_frame:
    .word body - _tbx_code_start - 1
    .byte 1, 0
    .res 14, 0
    .word $ff85
VM_EXPECT 13, failure, 64, 1, expected_stack, 128, $ff, 0, expected_frame, 20, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
