.import tbx_frames
.export _tbx_before_init
.segment "CODE"
_tbx_before_init:
    lda #$a5
    ldx #19
:
    sta tbx_frames,x
    dex
    bpl :-
    rts
