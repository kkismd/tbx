.export _tbx_read_byte, input_index
.segment "BSS"
input_index: .res 1
.segment "CODE"
_tbx_read_byte:
    ldx input_index
    cpx #input_bytes_end-input_bytes
    bcs input_eof
    lda input_bytes,x
    inx
    stx input_index
    tax
    lda #0
    rts
input_eof:
    lda #1
    rts
.segment "RODATA"
input_bytes:
    .byte "0", 10, "+42", 10, "-42", 10, "32767", 10, "-32768", 10
    .byte 9, " 0 ", 9, 10, 10, "+", 10, "abc", 10, "1 2", 10, "+ 2", 10
    .byte "32768", 10, "-32769", 10, "-327680", 10, "91", 10
    .byte "-65536", 10, "12", 10, "13", 13, 10, "1", 13, "2", 10, "42"
input_bytes_end:
