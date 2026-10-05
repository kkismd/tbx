.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 1, $fffe
entry:
failure:
    WRITE_TEXT 0
    HALT
VM_END
VM_EXPECT 22, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
