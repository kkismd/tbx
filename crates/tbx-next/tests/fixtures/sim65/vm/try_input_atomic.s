.include "vm_fixture.inc"
VM_HEADER entry, 1, 0, 0
entry:
    PUSH 77
    STORE 0
    PUSH 5
    CONTROL_PUSH
    CALL body
    HALT
body:
input_failure:
    .byte $52
    RET
VM_END
expected_frame:
    .byte 12, 0, 0, 1
    .res 16, 0
expected_control: .word 5
VM_EXPECT_STATE 26, input_failure, 0, 1, 1, 0, 0, 0, 77, expected_frame, 20, expected_control, 2
