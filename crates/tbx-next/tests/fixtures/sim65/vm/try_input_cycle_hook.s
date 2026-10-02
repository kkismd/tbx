.export _tbx_before_init
.import cycle_index
.segment "CODE"
_tbx_before_init:
    lda #0
    sta cycle_index
    rts
