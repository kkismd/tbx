.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    CALL outer
    HALT
outer:
    PUSH 10
    STORE_SCRATCH 0
    CALL inner
    LOAD_SCRATCH 0
    PUTDEC
    CR
    RET
inner:
    LOAD_SCRATCH 0
    PUTDEC
    CR
    PUSH 20
    STORE_SCRATCH 0
    CALL leaf
    LOAD_SCRATCH 0
    PUTDEC
    CR
    RET
leaf:
    LOAD_SCRATCH 0
    PUTDEC
    CR
    PUSH 30
    STORE_SCRATCH 0
    LOAD_SCRATCH 0
    PUTDEC
    CR
    RET
VM_END
