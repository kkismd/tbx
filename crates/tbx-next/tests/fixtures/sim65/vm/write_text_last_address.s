.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 1, descriptors
entry:
    WRITE_TEXT 0
    HALT
VM_END
.segment "RODATA"
descriptors: .word $ffff, 1
