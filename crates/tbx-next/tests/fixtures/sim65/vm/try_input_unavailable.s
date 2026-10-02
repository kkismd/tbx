.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .byte $52
    HALT
VM_END
VM_EXPECT 26, entry, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0
