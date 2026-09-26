.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 7
    CALL callee
    HALT
callee:
failure:
    COPY_BASE 2
    RET
VM_END
expected_stack: .word 7
expected_frame:
    .word callee - _tbx_code_start - 1
    .byte 1, 0
    .repeat 16
        .byte 0
    .endrepeat
VM_EXPECT 19, failure, 1, 1, expected_stack, 2, $ff, 0, expected_frame, 20
