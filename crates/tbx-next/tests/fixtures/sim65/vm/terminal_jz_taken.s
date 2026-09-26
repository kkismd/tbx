.include "vm_fixture.inc"
VM_HEADER entry, 0
done:
    HALT
entry:
    PUSH 0
    JZ done
VM_END
