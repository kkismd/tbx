.macpack longbranch
.import _tbx_test_expect
.importzp tbx_pc, tbx_data_depth, tbx_control_depth, tbx_call_depth, tbx_last_error
.import tbx_data_stack, tbx_globals, tbx_frames
.export _tbx_error_probe

.segment "ZEROPAGE"
probe_ptr: .res 2
probe_slot: .res 2
probe_byte: .res 1

.segment "CODE"
_tbx_error_probe:
    lda tbx_last_error
    cmp _tbx_test_expect
    beq :+
    jmp bad
:
    lda tbx_pc
    cmp _tbx_test_expect+1
    beq :+
    jmp bad
:
    lda tbx_pc+1
    cmp _tbx_test_expect+2
    beq :+
    jmp bad
:
    lda tbx_data_depth
    cmp _tbx_test_expect+3
    jne bad
    lda tbx_control_depth
    cmp _tbx_test_expect+4
    jne bad
    lda tbx_call_depth
    cmp _tbx_test_expect+5
    jne bad

    lda _tbx_test_expect+7
    sta probe_ptr
    lda _tbx_test_expect+8
    sta probe_ptr+1
    ldx #0
stack_loop:
    cpx _tbx_test_expect+6
    beq global_check
    txa
    tay
    lda (probe_ptr),y
    cmp tbx_data_stack,x
    jne bad
    inx
    jmp stack_loop

global_check:
    lda _tbx_test_expect+9
    cmp #$ff
    beq frames_check
    asl a
    sta probe_slot
    lda #0
    rol a
    sta probe_slot+1
    clc
    lda #<tbx_globals
    adc probe_slot
    sta probe_ptr
    lda #>tbx_globals
    adc probe_slot+1
    sta probe_ptr+1
    ldy #0
    lda (probe_ptr),y
    cmp _tbx_test_expect+10
    jne bad
    iny
    lda (probe_ptr),y
    cmp _tbx_test_expect+11
    jne bad

frames_check:
    lda _tbx_test_expect+13
    sta probe_ptr
    lda _tbx_test_expect+14
    sta probe_ptr+1
    ldx #0
frame_loop:
    cpx _tbx_test_expect+12
    beq array_check
    txa
    tay
    lda (probe_ptr),y
    cmp tbx_frames,x
    jne bad
    inx
    jmp frame_loop
array_check:
    lda _tbx_test_expect+15
    sta probe_ptr
    lda _tbx_test_expect+16
    sta probe_ptr+1
    lda _tbx_test_expect+17
    sta probe_slot
    lda _tbx_test_expect+18
    sta probe_slot+1
    ldx #0
array_loop:
    cpx _tbx_test_expect+19
    beq done
    txa
    tay
    lda (probe_ptr),y
    sta probe_byte
    lda (probe_slot),y
    cmp probe_byte
    jne bad
    inx
    jmp array_loop
done:
    rts
bad:
    ; A fixture expecting runtime error 19 needs a distinct probe failure.
    lda _tbx_test_expect
    cmp #19
    bne :+
    lda #9
    sta tbx_last_error
    rts
:
    lda #19
    sta tbx_last_error
    rts
