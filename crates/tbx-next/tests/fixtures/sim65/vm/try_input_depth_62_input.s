.export _tbx_read_byte, input_index
.segment "BSS"
input_index: .res 1
.segment "CODE"
_tbx_read_byte:
    ldx input_index
    cpx #2
    bcs input_eof
    inx
    stx input_index
    cpx #1
    beq input_value
    ldx #10
    lda #0
    rts
input_value:
    ldx #'7'
    lda #0
    rts
input_eof:
    lda #1
    rts
