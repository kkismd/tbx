.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 42
    CONTROL_PUSH
    .repeat 64
        PUSH 1
    .endrepeat
failure:
    CONTROL_COPY
    HALT
VM_END
expected_stack:
    .repeat 64
        .word 1
    .endrepeat
expected_control: .word 42
VM_EXPECT_CONTROL 13, failure, 64, 1, 0, expected_stack, 128, expected_control, 2, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK | VM_CHECK_CONTROL_DEPTH | VM_CHECK_CONTROL_STACK
