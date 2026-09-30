.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 9
    CONTROL_PUSH
    CALL callee
    CONTROL_COPY
    PUTDEC
    CR
    CONTROL_DROP
    PUSH 4
    PUTDEC
    CR
    HALT
callee:
    PUSH 12
    CONTROL_PUSH
    RET
VM_END
