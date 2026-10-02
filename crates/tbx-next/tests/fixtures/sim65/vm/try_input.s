.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .repeat 17
    .byte $52
    PUTDEC
    CR
    PUTDEC
    CR
    .endrepeat
    HALT
VM_END
