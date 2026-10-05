.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
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
VM_EXPECT 10, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
