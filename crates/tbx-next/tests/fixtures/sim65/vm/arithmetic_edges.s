.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 181
    PUSH 181
    MUL
    PUTDEC
    CR
    PUSH -32768
    PUSH 3
    REM
    PUTDEC
    CR
    PUSH 7
    PUSH -3
    REM
    PUTDEC
    CR
    PUSH -32768
    PUSH 32767
    LT
    PUTDEC
    CR
    PUSH -1
    PUSH -1
    LE
    PUTDEC
    CR
    PUSH -1
    PUSH 0
    GE
    PUTDEC
    CR
    PUSH 0
    PUSH -1
    GE
    PUTDEC
    CR
    PUSH -32768
    PUSH -32768
    EQ
    PUTDEC
    CR
    HALT
VM_END
