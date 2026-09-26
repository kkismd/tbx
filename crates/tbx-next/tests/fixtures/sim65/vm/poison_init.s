.include "vm_fixture.inc"
VM_HEADER entry, 256
entry:
    LOAD 0
    LOAD 255
    CALL callee
    HALT
callee:
failure:
    .byte $00
VM_END
expected_stack: .word 0, 0
expected_frame:
    .word callee - _tbx_code_start - 1
    .byte 2, 0
    .repeat 16
        .byte 0
    .endrepeat
VM_EXPECT 10, failure, 2, 1, expected_stack, 4, 255, 0, expected_frame, 20
