.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 7
    CONTROL_PUSH
    PUSH -3
    CONTROL_PUSH
    CONTROL_COPY
    PUTDEC
    CR
    CONTROL_DROP
    CONTROL_COPY
    PUTDEC
    CR
    HALT
VM_END
