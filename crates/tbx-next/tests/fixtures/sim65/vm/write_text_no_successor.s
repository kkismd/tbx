.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 1, descriptors
entry:
failure:
    WRITE_TEXT 0
VM_END
.segment "RODATA"
descriptors: .word bytes, 1
bytes: .byte 'x'
VM_EXPECT 11, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0
