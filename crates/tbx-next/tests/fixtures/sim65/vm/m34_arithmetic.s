.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 10
    PUSH 3
    SUB
    PUTDEC
    CR
    PUSH -7
    PUSH 2
    DIV
    PUTDEC
    CR
    PUSH 7
    PUSH -2
    DIV
    PUTDEC
    CR
    PUSH -7
    PUSH -2
    DIV
    PUTDEC
    CR
    PUSH -32768
    PUSH 1
    DIV
    PUTDEC
    CR
    PUSH 5
    NEG
    PUTDEC
    CR
    PUSH -32767
    ABS
    PUTDEC
    CR
    PUSH -1
    PUSH 0
    GT
    PUTDEC
    CR
    PUSH 0
    PUSH -1
    GT
    PUTDEC
    CR
    PUSH 1
    PUSH 0
    GT
    PUTDEC
    CR
    HALT
VM_END
