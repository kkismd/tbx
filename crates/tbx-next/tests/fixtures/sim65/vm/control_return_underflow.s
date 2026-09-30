.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 55
    CONTROL_PUSH
    CALL callee
after_call:
    HALT
callee:
    CONTROL_DROP
failure:
    RET
VM_END
expected_control: .word 55
expected_frame:
    .word after_call - _tbx_code_start
    .byte 0, 1
    .res 16, 0
VM_EXPECT_CONTROL 19, failure, 0, 0, 1, 0, 0, expected_control, 2, expected_frame, 20
