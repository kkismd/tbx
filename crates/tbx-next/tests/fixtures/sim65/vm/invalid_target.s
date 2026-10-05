.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 7
failure:
    .byte $30, $ff, $ff
    HALT
VM_END
VM_EXPECT 11, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
