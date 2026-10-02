.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .repeat 62
    PUSH 0
    .endrepeat
    TRY_INPUT
    PUTDEC
    CR
    PUTDEC
    CR
    HALT
VM_END
