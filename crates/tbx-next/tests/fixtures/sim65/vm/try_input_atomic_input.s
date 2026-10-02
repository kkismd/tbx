.export _tbx_read_byte, input_index
.segment "BSS"
input_index: .res 1
.segment "CODE"
_tbx_read_byte:
    lda input_index
    bne input_failure
    inc input_index
    ldx #'4'
    lda #0
    rts
input_failure:
    lda #2
    rts
