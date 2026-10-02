.export _tbx_before_init
.import input_index
.segment "CODE"
_tbx_before_init:
    lda #0
    sta input_index
    rts
