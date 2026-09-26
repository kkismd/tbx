.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 7
    .byte $31, $ff, $ff
    PUSH 42
    PUTDEC
    CR
    HALT
VM_END
