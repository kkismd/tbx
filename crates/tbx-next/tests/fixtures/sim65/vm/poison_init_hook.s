.importzp tbx_data_depth, tbx_control_depth, tbx_call_depth, tbx_last_error
.import tbx_data_stack, tbx_globals, tbx_frames
.export _tbx_before_init
.segment "CODE"
_tbx_before_init:
    lda #$a5
    sta tbx_data_depth
    sta tbx_control_depth
    sta tbx_call_depth
    sta tbx_last_error
    ldx #0
globals_loop:
    sta tbx_globals,x
    sta tbx_globals+256,x
    inx
    bne globals_loop
    ldx #0
frame_loop:
    sta tbx_frames,x
    inx
    cpx #20
    bne frame_loop
    ldx #0
stack_loop:
    sta tbx_data_stack,x
    inx
    cpx #128
    bne stack_loop
    rts
