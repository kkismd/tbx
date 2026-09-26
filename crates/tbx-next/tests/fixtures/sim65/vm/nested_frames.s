.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 11
    CALL outer
    HALT
outer:
    PUSH 22
    CALL inner
    RET
inner:
failure:
    .byte $00
VM_END
expected_stack: .word 11, 22
expected_frames:
    .word outer - _tbx_code_start - 1
    .byte 1, 0
    .repeat 16
        .byte 0
    .endrepeat
    .word inner - _tbx_code_start - 1
    .byte 2, 0
    .repeat 16
        .byte 0
    .endrepeat
VM_EXPECT 10, failure, 2, 2, expected_stack, 4, $ff, 0, expected_frames, 40
