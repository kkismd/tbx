.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .repeat 16, I
        PUSH I
        CONTROL_PUSH
    .endrepeat
    .repeat 16
        CONTROL_DROP
    .endrepeat
    PUSH 16
    PUTDEC
    CR
    HALT
VM_END
