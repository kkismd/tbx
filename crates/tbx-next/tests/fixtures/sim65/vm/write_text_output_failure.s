.include "vm_fixture.inc"
.export _putchar
VM_TEXT_HEADER entry, 0, 0, 0, 1, descriptors
entry:
    PUSH 4321
failure:
    WRITE_TEXT 0
    HALT
VM_END
.segment "RODATA"
descriptors: .word bytes, 2
bytes: .byte 'x', 'y'
VM_EXPECT 18, failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, 0
.segment "CODE"
_putchar:
    lda #$ff
    tax
    rts
