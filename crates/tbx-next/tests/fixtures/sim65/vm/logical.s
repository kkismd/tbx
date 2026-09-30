.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 0
    PUSH 0
    LOGICAL_AND
    PUTDEC
    CR
    PUSH 0
    PUSH -1
    LOGICAL_AND
    PUTDEC
    CR
    PUSH -2
    PUSH 0
    LOGICAL_AND
    PUTDEC
    CR
    PUSH -2
    PUSH 3
    LOGICAL_AND
    PUTDEC
    CR
    PUSH 2
    PUSH -3
    LOGICAL_AND
    PUTDEC
    CR
    PUSH 0
    PUSH 0
    LOGICAL_OR
    PUTDEC
    CR
    PUSH 0
    PUSH 1
    LOGICAL_OR
    PUTDEC
    CR
    PUSH -1
    PUSH 0
    LOGICAL_OR
    PUTDEC
    CR
    PUSH 5
    PUSH 0
    LOGICAL_OR
    PUTDEC
    CR
    PUSH 0
    PUSH -7
    LOGICAL_OR
    PUTDEC
    CR
    PUSH 0
    PUSH -32768
    SWAP
    PUTDEC
    CR
    PUTDEC
    CR
    PUSH 123
    PUSH -45
    SWAP
    PUTDEC
    CR
    PUTDEC
    CR
    PUSH 32767
    PUSH -32768
    NOT_EQUAL
    PUTDEC
    CR
    PUSH -32768
    PUSH -32768
    NOT_EQUAL
    PUTDEC
    CR
    PUSH -1
    PUSH 0
    NOT_EQUAL
    PUTDEC
    CR
    PUSH 123
    PUSH 123
    NOT_EQUAL
    PUTDEC
    CR
    HALT
VM_END
