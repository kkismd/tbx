.include "vm_fixture.inc"
VM_HEADER entry, 0
callee:
    RET
entry:
    CALL callee
VM_END
VM_EXPECT 11, entry, 0, 0, 0, 0, $ff, 0, 0, 0
