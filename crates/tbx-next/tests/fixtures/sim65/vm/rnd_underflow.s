.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
failure:
    RND
    HALT
VM_END
VM_EXPECT 12, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0
