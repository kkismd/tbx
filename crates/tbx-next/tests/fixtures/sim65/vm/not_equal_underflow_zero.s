.include "vm_fixture.inc"
VM_HEADER entry, 1, 0, 0
entry:
    PUSH 123
    STORE 0
    PUSH -7
    CONTROL_PUSH
    CALL failure
after_call:
    HALT
failure:
    NOT_EQUAL
    HALT
VM_END
expected_stack: .word 0
expected_frame:
    .word after_call - _tbx_code_start
    .byte 0, 1
expected_control: .word (-7) & $ffff
VM_EXPECT_STATE 12, failure, 0, 1, 1, expected_stack, 0, 0, 123, expected_frame, 4, expected_control, 2, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
