.include "vm_fixture.inc"
VM_HEADER entry, 1, 0, 0
entry:
    PUSH 123
    STORE 0
    PUSH 77
    CONTROL_PUSH
    CALL random_failure
return_after_call:
    HALT
random_failure:
    PUSH 0
failure:
    RND
    HALT
VM_END
expected_stack: .word 0
expected_control: .word 77
expected_frame:
    .byte return_after_call - _tbx_code_start, 0, 0, 1
    .res 16, 0
VM_EXPECT_STATE 25, failure, 1, 1, 1, expected_stack, 2, 0, 123, expected_frame, 20, expected_control, 2
