.export _tbx_read_byte, cycle_index
.segment "BSS"
cycle_index: .res 1
.segment "CODE"
_tbx_read_byte:
    ldx cycle_index
    cpx #3
    bcs input_eof
    lda cycle_bytes,x
    inx
    stx cycle_index
    tax
    lda #0
    rts
input_eof:
    lda #1
    rts
.segment "RODATA"
cycle_bytes: .byte "4", "2", 10
