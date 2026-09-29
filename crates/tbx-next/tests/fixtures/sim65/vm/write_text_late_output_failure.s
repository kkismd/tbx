.include "vm_fixture.inc"
.export _putchar
VM_TEXT_HEADER entry, 0, 0, 0, 1, descriptors
entry:
    PUSH 987
failure:
    WRITE_TEXT 0
    HALT
VM_END
.segment "RODATA"
descriptors: .word bytes, 3
bytes: .byte 'P', 'Q', 'R'
expected_stack: .word 987
expected_prefix: .byte 'P'
VM_EXPECT 18, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, output_prefix, expected_prefix, 1
.segment "BSS"
putchar_calls: .res 1
emitted_byte: .res 1
output_prefix: .res 1
.segment "CODE"
_putchar:
    sta emitted_byte
    inc putchar_calls
    lda putchar_calls
    cmp #1
    bne failed
    lda emitted_byte
    sta output_prefix
    ldx #0
    rts
failed:
    lda #$ff
    ldx #$ff
    rts
