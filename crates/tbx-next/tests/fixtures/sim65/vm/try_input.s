.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .repeat 21
    TRY_INPUT
    PUTDEC
    CR
    PUTDEC
    CR
    .endrepeat
    HALT
VM_END
