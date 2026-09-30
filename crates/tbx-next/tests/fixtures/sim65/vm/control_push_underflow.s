.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
failure:
    CONTROL_PUSH
    HALT
VM_END
VM_EXPECT_CONTROL 12, failure, 0, 0, 0, 0, 0, 0, 0, 0, 0
