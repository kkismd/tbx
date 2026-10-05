.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    CALL callee
failure:
    CONTROL_DROP
    HALT
callee:
    PUSH 12
    CONTROL_PUSH
    RET
VM_END
VM_EXPECT_CONTROL 23, failure, 0, 0, 0, 0, 0, 0, 0, 0, 0, VM_CHECK_CONTROL_DEPTH | VM_CHECK_CONTROL_STACK
