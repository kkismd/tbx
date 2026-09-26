.include "vm_fixture.inc"
VM_HEADER entry, 256
    .byte $00
entry:
    LOAD 255
    PUTDEC
    CR
    PUSH 99
    STORE 255
    LOAD 255
    PUTDEC
    CR
    HALT
VM_END
