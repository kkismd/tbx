.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 0
    PUTCHR
    PUSH 127
    PUTCHR
    HALT
VM_END
