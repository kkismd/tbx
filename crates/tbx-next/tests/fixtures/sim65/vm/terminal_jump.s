.include "vm_fixture.inc"
VM_HEADER entry, 0
done:
    HALT
entry:
    VM_JUMP done
VM_END
