.export _tbx_read_byte
.segment "CODE"
; A=2 means the target wrapper cannot provide input.
_tbx_read_byte:
    lda #2
    rts
