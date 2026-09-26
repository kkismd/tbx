.include "vm_fixture.inc"
VM_HEADER entry, 2
entry:
    ; Both global cells must be explicitly initialized to zero.
    LOAD 0
    PUTDEC
    CR
    PUSH 5
    STORE 1
loop:
    LOAD 1
    PUTDEC
    CR
    LOAD 1
    PUSH 1
    PUSH -1
    ADD
    LT
    JZ not_taken
    PUSH 99
    PUTDEC
not_taken:
    LOAD 1
    PUSH -1
    ADD
    STORE 1
    LOAD 1
    JZ finished
    VM_JUMP loop
finished:
    LOAD 1
    PUTDEC
    CR
    HALT
VM_END
