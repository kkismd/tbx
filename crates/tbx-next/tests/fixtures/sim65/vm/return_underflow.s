.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
failure:
    RET
    HALT
VM_END
VM_EXPECT 14, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_CALL_DEPTH | VM_CHECK_CONTROL_DEPTH
