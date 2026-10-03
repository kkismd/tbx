.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    CALL body
    HALT
body:
    .repeat 8, I
        LOAD_SCRATCH I
        PUTDEC
        CR
    .endrepeat
    PUSH -32768
    STORE_SCRATCH 0
    PUSH 32767
    STORE_SCRATCH 1
    PUSH -1
    STORE_SCRATCH 2
    PUSH 0
    STORE_SCRATCH 3
    PUSH 1
    STORE_SCRATCH 4
    PUSH -12345
    STORE_SCRATCH 5
    PUSH 12345
    STORE_SCRATCH 6
    PUSH 42
    STORE_SCRATCH 7
    .repeat 8, I
        LOAD_SCRATCH I
        PUTDEC
        CR
    .endrepeat
    RET
VM_END
