.include "vm_fixture.inc"
VM_HEADER entry, 1
entry:
failure:
    CALL failure
    HALT
VM_END
expected_frame:
    .word failure + 3 - _tbx_code_start
    .byte 0, 0
    .repeat 16
        .byte 0
    .endrepeat
VM_EXPECT 15, failure, 0, 16, 0, 0, 0, 0, expected_frame, 20
