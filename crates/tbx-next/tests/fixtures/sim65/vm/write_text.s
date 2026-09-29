.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 3, descriptors
entry:
    WRITE_TEXT 0
    WRITE_TEXT 1
    WRITE_TEXT 2
    HALT
VM_END
.segment "RODATA"
descriptors:
    .word one, 1
    .word many, 4
    .word empty, 0
one: .byte 'A'
many: .byte 'B', 0, $80, 'C'
empty: .byte 'X'
