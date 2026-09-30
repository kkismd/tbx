.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
failure:
    CONTROL_COPY
    HALT
VM_END
VM_EXPECT_CONTROL 23, failure, 0, 0, 0, 0, 0, 0, 0, 0, 0
