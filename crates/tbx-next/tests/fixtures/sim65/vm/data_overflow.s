.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    .repeat 64
        PUSH 1
    .endrepeat
failure:
    PUSH 2
    HALT
VM_END
expected_stack:
    .repeat 64
        .word 1
    .endrepeat
VM_EXPECT 13, failure, 64, 0, expected_stack, 128, $ff, 0, 0, 0
