.include "vm_fixture.inc"
VM_HEADER entry, 256, 0, 0
entry:
    LOAD 0
    LOAD 255
    CALL callee
    HALT
callee:
failure:
    .byte $00
VM_END
VM_EXPECT 10, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
