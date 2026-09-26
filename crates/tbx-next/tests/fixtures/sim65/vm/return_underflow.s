.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
failure:
    RET
    HALT
VM_END
VM_EXPECT 14, failure, 0, 0, 0, 0, $ff, 0, 0, 0
