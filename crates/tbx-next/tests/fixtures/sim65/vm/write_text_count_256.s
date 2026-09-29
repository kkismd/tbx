.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 256, descriptors
entry:
    WRITE_TEXT 255
    HALT
VM_END
.segment "RODATA"
descriptors:
.repeat 255
    .word 0, 0
.endrepeat
    .word final_byte, 1
final_byte: .byte 'Z'
