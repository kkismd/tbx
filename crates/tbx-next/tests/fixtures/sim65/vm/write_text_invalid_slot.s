.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 1, descriptors
entry:
    PUSH 123
failure:
    WRITE_TEXT 1
    HALT
VM_END
.segment "RODATA"
descriptors: .word bytes, 1
bytes: .byte 'x'
VM_EXPECT 22, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
