.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
failure:
    .byte $63
VM_END
VM_EXPECT 11, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
