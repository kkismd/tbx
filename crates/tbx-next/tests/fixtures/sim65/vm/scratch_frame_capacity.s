.include "vm_fixture.inc"
VM_HEADER entry, 1, 0, 0
entry:
    PUSH 16
    STORE 0
    CALL descend
    HALT
descend:
    LOAD_SCRATCH 7
    DROP
    LOAD 0
    PUSH 1
    SUB
    STORE 0
    LOAD 0
    JZ deepest
    CALL descend
    RET
deepest:
    LOAD_SCRATCH 7
    PUTDEC
    CR
    PUSH -32768
    STORE_SCRATCH 7
    LOAD_SCRATCH 7
    PUTDEC
    CR
    RET
VM_END
