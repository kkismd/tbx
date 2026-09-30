.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .repeat 16
        PUSH 1
        CONTROL_PUSH
    .endrepeat
    PUSH 99
failure:
    CONTROL_PUSH
    HALT
VM_END
expected_control:
    .repeat 16
        .word 1
    .endrepeat
expected_data: .word 99
VM_EXPECT_CONTROL 24, failure, 1, 16, 0, expected_data, 2, expected_control, 32, 0, 0
