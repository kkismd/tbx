.include "vm_fixture.inc"
VM_HEADER entry, 2
entry:
    PUSH 0
    PUTDEC
    CR
    PUSH 32767
    PUSH -32768
    ADD
    PUTDEC
    CR
    PUSH -32768
    PUSH 1
    MUL
    PUTDEC
    CR
    PUSH -7
    PUSH 3
    REM
    PUTDEC
    CR
    PUSH 5
    PUSH 5
    EQ
    PUTDEC
    CR
    PUSH 5
    PUSH 4
    EQ
    PUTDEC
    CR
    PUSH -2
    PUSH 1
    LT
    PUTDEC
    CR
    PUSH 2
    PUSH 1
    LT
    PUTDEC
    CR
    PUSH 2
    PUSH 2
    LE
    PUTDEC
    CR
    PUSH 3
    PUSH 2
    LE
    PUTDEC
    CR
    PUSH 3
    PUSH 2
    GE
    PUTDEC
    CR
    PUSH -3
    PUSH 2
    GE
    PUTDEC
    CR
    PUSH -32768
    PUTDEC
    CR
    PUSH 123
    DROP
    HALT
VM_END
