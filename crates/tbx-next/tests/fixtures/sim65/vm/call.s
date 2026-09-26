.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 11
    PUSH 22
    CALL outer
    PUTDEC
    CR
    PUTDEC
    CR
    HALT
outer:
    COPY_BASE 1
    CALL inner
    ; After inner returns the outer call base still refers to 22.
    COPY_BASE 1
    ADD
    RET
inner:
    ; This call base refers to the 11 copied by outer.
    COPY_BASE 1
    RET
VM_END
