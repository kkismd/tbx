.import tbx_rng_state
.export _tbx_before_init
.segment "CODE"
_tbx_before_init:
    lda #1
    sta tbx_rng_state
    lda #0
    sta tbx_rng_state+1
    sta tbx_rng_state+2
    sta tbx_rng_state+3
    sta tbx_rng_state+4
    sta tbx_rng_state+5
    sta tbx_rng_state+6
    sta tbx_rng_state+7
    rts
