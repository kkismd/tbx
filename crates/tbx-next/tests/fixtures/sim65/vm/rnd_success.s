.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, empty_arrays
entry:
    PUSH 10
    RND
    PUTDEC
    CR
    PUSH 100
    RND
    PUTDEC
    CR
    PUSH 97
    RND
    PUTDEC
    CR
    PUSH 32767
    RND
    PUTDEC
    CR
    PUSH 10
    RND
    PUTDEC
    CR
    PUSH 1
    RND
    PUTDEC
    CR
    HALT
VM_END
.segment "RODATA"
empty_arrays: .word 0, 0
