; M32 private bytecode runtime for the sim6502 target.
; The bytecode wrapper supplies four RODATA symbols. The two hooks are private
; M32 test instrumentation; ordinary wrappers implement them as no-ops.
.setcpu "6502"
.macpack longbranch

.import _tbx_code_start, _tbx_code_end, _tbx_entry_offset, _tbx_global_count
.import _tbx_before_init, _tbx_error_probe, _putchar
.export _main
.export tbx_pc, tbx_base, tbx_end, tbx_data_depth, tbx_control_depth
.export tbx_call_depth, tbx_global_count, tbx_last_error
.export tbx_data_stack, tbx_globals, tbx_frames

.segment "ZEROPAGE"
tbx_pc:             .res 2
tbx_base:           .res 2
tbx_end:            .res 2
tbx_data_depth:     .res 1
tbx_control_depth:  .res 1
tbx_call_depth:     .res 1
tbx_global_count:   .res 2
tbx_last_error:     .res 1
cursor:             .res 2
target:             .res 2
ptr:                .res 2
left:               .res 2
right:              .res 2
value:              .res 4
work:               .res 2
sign:               .res 1
count:              .res 1
opcode:             .res 1

.segment "BSS"
tbx_data_stack: .res 128
tbx_frames:     .res 320
tbx_globals:    .res 512
digits:         .res 6

.segment "RODATA"
frame_addresses:
.repeat 16, I
    .word tbx_frames + I * 20
.endrepeat

.segment "CODE"
_main:
    jsr _tbx_before_init
    lda #<_tbx_code_start
    sta tbx_base
    lda #>_tbx_code_start
    sta tbx_base+1
    lda #<_tbx_code_end
    sta tbx_end
    lda #>_tbx_code_end
    sta tbx_end+1
    lda _tbx_global_count
    sta tbx_global_count
    lda _tbx_global_count+1
    sta tbx_global_count+1
    lda #0
    sta tbx_data_depth
    sta tbx_control_depth
    sta tbx_call_depth
    sta tbx_last_error
    ; Count is bounded before touching the globals allocation.
    lda tbx_global_count+1
    beq :+
    cmp #1
    jne fail_global
    lda tbx_global_count
    jne fail_global
:
    lda tbx_base
    cmp tbx_end
    lda tbx_base+1
    sbc tbx_end+1
    jcs fail_bytecode
    ; Initialize exactly the declared cells, independent of BSS startup contents.
    lda #<tbx_globals
    sta ptr
    lda #>tbx_globals
    sta ptr+1
    lda tbx_global_count
    sta work
    lda tbx_global_count+1
    sta work+1
init_globals:
    lda work
    ora work+1
    jeq init_entry
    ldy #0
    tya
    sta (ptr),y
    iny
    sta (ptr),y
    clc
    lda ptr
    adc #2
    sta ptr
    bcc :+
    inc ptr+1
:
    lda work
    bne :+
    dec work+1
:
    dec work
    jmp init_globals
init_entry:
    lda _tbx_entry_offset
    sta left
    lda _tbx_entry_offset+1
    sta left+1
    jsr validate_target
    jcs fail_bytecode
    jsr commit_target

dispatch:
    ; PC always names the current opcode. Cursor is speculative.
    lda tbx_pc+1
    cmp tbx_base+1
    jcc fail_bytecode
    bne :+
    lda tbx_pc
    cmp tbx_base
    jcc fail_bytecode
:
    lda tbx_pc+1
    cmp tbx_end+1
    bcc :+
    jne fail_bytecode
    lda tbx_pc
    cmp tbx_end
    jcs fail_bytecode
:
    ldy #0
    lda (tbx_pc),y
    sta opcode
    cmp #$01
    jeq op_halt
    cmp #$02
    jeq op_push
    cmp #$10
    jeq op_load
    cmp #$11
    jeq op_store
    cmp #$20
    jeq op_call
    cmp #$21
    jeq op_copy_base
    cmp #$22
    jeq op_return
    cmp #$30
    jeq op_jump
    cmp #$31
    jeq op_jump_zero
    cmp #$40
    jeq op_binary
    cmp #$41
    jeq op_binary
    cmp #$42
    jeq op_binary
    cmp #$48
    jeq op_binary
    cmp #$49
    jeq op_binary
    cmp #$4a
    jeq op_binary
    cmp #$4b
    jeq op_binary
    cmp #$50
    jeq op_drop
    cmp #$60
    jeq op_putdec
    cmp #$61
    jeq op_cr
    jmp fail_opcode

op_halt:
    lda #0
    rts

; Input A is the full instruction length. Carry set means truncated/wrapped.
need_bytes:
    clc
    adc tbx_pc
    sta cursor
    lda tbx_pc+1
    adc #0
    sta cursor+1
    jcs need_bad
    lda cursor+1
    cmp tbx_end+1
    jcc need_good
    jne need_bad
    lda cursor
    cmp tbx_end
    jcc need_good
    jeq need_good
need_bad:
    sec
    rts
need_good:
    ; Move cursor to the first operand after validating the whole instruction.
    clc
    lda tbx_pc
    adc #1
    sta cursor
    lda tbx_pc+1
    adc #0
    sta cursor+1
    clc
    rts

read_operand:
    ldy #0
    lda (cursor),y
    inc cursor
    bne :+
    inc cursor+1
:
    rts

read_word:
    jsr read_operand
    sta left
    jsr read_operand
    sta left+1
    rts

; A fallthrough instruction must have a real successor before it changes VM
; state. Jump, Return, and a taken conditional branch do not fall through.
validate_next:
    lda cursor+1
    cmp tbx_end+1
    bcc next_good
    bne next_bad
    lda cursor
    cmp tbx_end
    bcc next_good
next_bad:
    sec
    rts
next_good:
    clc
    rts

commit_cursor:
    lda cursor
    sta tbx_pc
    lda cursor+1
    sta tbx_pc+1
    jmp dispatch

commit_target:
    lda target
    sta tbx_pc
    lda target+1
    sta tbx_pc+1
    rts

; Input left is a bytecode offset. Reject address wrap and end-exclusive targets.
validate_target:
    clc
    lda tbx_base
    adc left
    sta target
    lda tbx_base+1
    adc left+1
    sta target+1
    jcs target_bad
    lda target+1
    cmp tbx_end+1
    jcc target_good
    jne target_bad
    lda target
    cmp tbx_end
    jcc target_good
target_bad:
    sec
    rts
target_good:
    clc
    rts

op_push:
    lda #3
    jsr need_bytes
    jcs fail_bytecode
    jsr read_word
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    cmp #64
    jcs fail_overflow
    ldx tbx_data_depth
    txa
    asl
    tax
    lda left
    sta tbx_data_stack,x
    inx
    lda left+1
    sta tbx_data_stack,x
    inc tbx_data_depth
    jmp commit_cursor

; Global slot operand is u8; index arithmetic is 16-bit for slot 255.
global_pointer:
    jsr read_operand
    sta count
    lda tbx_global_count+1
    jne global_valid
    lda count
    cmp tbx_global_count
    jcs global_bad
global_valid:
    lda count
    asl
    sta ptr
    lda #0
    rol
    sta ptr+1
    clc
    lda ptr
    adc #<tbx_globals
    sta ptr
    lda ptr+1
    adc #>tbx_globals
    sta ptr+1
    clc
    rts
global_bad:
    sec
    rts

op_load:
    lda #2
    jsr need_bytes
    jcs fail_bytecode
    jsr global_pointer
    jcs fail_global
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    cmp #64
    jcs fail_overflow
    ldy #0
    lda (ptr),y
    sta left
    iny
    lda (ptr),y
    sta left+1
    ldx tbx_data_depth
    txa
    asl
    tax
    lda left
    sta tbx_data_stack,x
    inx
    lda left+1
    sta tbx_data_stack,x
    inc tbx_data_depth
    jmp commit_cursor

op_store:
    lda #2
    jsr need_bytes
    jcs fail_bytecode
    jsr global_pointer
    jcs fail_global
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    sec
    sbc #1
    asl
    tax
    ldy #0
    lda tbx_data_stack,x
    sta (ptr),y
    inx
    iny
    lda tbx_data_stack,x
    sta (ptr),y
    dec tbx_data_depth
    jmp commit_cursor

; X = 2 * frame index, ptr = address of that 20-byte frame.
frame_at_x:
    lda frame_addresses,x
    sta ptr
    lda frame_addresses+1,x
    sta ptr+1
    rts

op_call:
    lda #3
    jsr need_bytes
    jcs fail_bytecode
    jsr read_word
    jsr validate_target
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_call_depth
    cmp #16
    jcs fail_call_overflow
    asl
    tax
    jsr frame_at_x
    ; Return position is a byte offset, not a machine address.
    sec
    lda cursor
    sbc tbx_base
    ldy #0
    sta (ptr),y
    lda cursor+1
    sbc tbx_base+1
    iny
    sta (ptr),y
    lda tbx_data_depth
    iny
    sta (ptr),y
    lda tbx_control_depth
    iny
    sta (ptr),y
    lda #0
    ldy #4
:
    sta (ptr),y
    iny
    cpy #20
    bne :-
    inc tbx_call_depth
    jsr commit_target
    jmp dispatch

op_copy_base:
    lda #2
    jsr need_bytes
    jcs fail_bytecode
    jsr read_operand
    sta count
    jsr validate_next
    jcs fail_bytecode
    lda tbx_call_depth
    jeq fail_call_underflow
    sec
    sbc #1
    asl
    tax
    jsr frame_at_x
    ; ReferenceVm's offset is one-based and counts back from call depth.
    lda count
    jeq fail_invariant
    ldy #2
    lda (ptr),y
    cmp count
    jcc fail_invariant
    sec
    sbc count
    cmp tbx_data_depth
    jcs fail_invariant
    sta count
    lda tbx_data_depth
    cmp #64
    jcs fail_overflow
    lda count
    asl
    tax
    lda tbx_data_stack,x
    sta left
    inx
    lda tbx_data_stack,x
    sta left+1
    lda tbx_data_depth
    asl
    tax
    lda left
    sta tbx_data_stack,x
    inx
    lda left+1
    sta tbx_data_stack,x
    inc tbx_data_depth
    jmp commit_cursor

op_return:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    lda tbx_call_depth
    jeq fail_call_underflow
    sec
    sbc #1
    asl
    tax
    jsr frame_at_x
    ldy #0
    lda (ptr),y
    sta left
    iny
    lda (ptr),y
    sta left+1
    jsr validate_target
    jcs fail_bytecode
    ldy #3
    lda tbx_control_depth
    cmp (ptr),y
    jcc fail_invariant
    lda (ptr),y
    sta tbx_control_depth
    dec tbx_call_depth
    jsr commit_target
    jmp dispatch

op_jump:
    lda #3
    jsr need_bytes
    jcs fail_bytecode
    jsr read_word
    jsr validate_target
    jcs fail_bytecode
    jsr commit_target
    jmp dispatch

op_jump_zero:
    lda #3
    jsr need_bytes
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    jsr read_word
    lda tbx_data_depth
    sec
    sbc #1
    asl
    tax
    lda tbx_data_stack,x
    inx
    ora tbx_data_stack,x
    bne jump_zero_untaken
    jsr validate_target
    jcs fail_bytecode
    dec tbx_data_depth
    jsr commit_target
    jmp dispatch
jump_zero_untaken:
    jsr validate_next
    jcs fail_bytecode
    dec tbx_data_depth
    jmp commit_cursor

op_drop:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    dec tbx_data_depth
    jmp commit_cursor

op_binary:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    cmp #2
    jcc fail_underflow
    sec
    sbc #2
    asl
    tax
    lda tbx_data_stack,x
    sta left
    inx
    lda tbx_data_stack,x
    sta left+1
    inx
    lda tbx_data_stack,x
    sta right
    inx
    lda tbx_data_stack,x
    sta right+1
    lda opcode
    cmp #$40
    jeq binary_add
    cmp #$41
    jeq binary_multiply
    cmp #$42
    jeq binary_remainder
    jmp binary_compare

binary_add:
    clc
    lda left
    adc right
    sta value
    lda left+1
    adc right+1
    sta value+1
    jvc binary_commit
    jmp fail_arithmetic

binary_compare:
    lda #0
    sta value+1
    lda left+1
    eor #$80
    sta work
    lda right+1
    eor #$80
    cmp work
    bne compare_order
    lda right
    cmp left
compare_order:
    ; Count is 0 for equality, 1 for left < right, $ff for left > right.
    bcc :+
    beq :++
    lda #1
    bne compare_saved
:
    lda #$ff
    bne compare_saved
:
    lda #0
compare_saved:
    sta count
    lda opcode
    cmp #$48
    jeq compare_equal
    lda opcode
    cmp #$49
    jeq compare_less
    cmp #$4a
    jeq compare_less_equal
    lda count
    beq compare_true
    bmi compare_true
    jmp compare_false
compare_equal:
    lda count
    jeq compare_true
    jmp compare_false
compare_less:
    lda count
    cmp #1
    jeq compare_true
    jmp compare_false
compare_less_equal:
    lda count
    cmp #$ff
    jeq compare_false
    jmp compare_true
compare_true:
    lda #1
    jne compare_commit
compare_false:
    lda #0
compare_commit:
    sta value
    jmp binary_commit

binary_multiply:
    lda left+1
    eor right+1
    and #$80
    sta sign
    jsr absolute_pair
    lda #0
    sta value
    sta value+1
    sta value+2
    sta value+3
    lda #16
    sta count
multiply_loop:
    lda right
    and #1
    beq :+
    clc
    lda value
    adc left
    sta value
    lda value+1
    adc left+1
    sta value+1
    lda value+2
    adc work
    sta value+2
    lda value+3
    adc work+1
    sta value+3
:
    lsr right+1
    ror right
    asl left
    rol left+1
    rol work
    rol work+1
    dec count
    jne multiply_loop
    lda value+2
    ora value+3
    jne fail_arithmetic
    lda sign
    jeq multiply_positive
    lda value+1
    cmp #$80
    jcc multiply_negative
    jne fail_arithmetic
    lda value
    jne fail_arithmetic
multiply_negative:
    sec
    lda #0
    sbc value
    sta value
    lda #0
    sbc value+1
    sta value+1
    jmp binary_commit
multiply_positive:
    lda value+1
    jmi fail_arithmetic
    jmp binary_commit

absolute_pair:
    lda #0
    sta work
    sta work+1
    lda left+1
    bpl :+
    sec
    lda #0
    sbc left
    sta left
    lda #0
    sbc left+1
    sta left+1
:
    lda right+1
    bpl :+
    sec
    lda #0
    sbc right
    sta right
    lda #0
    sbc right+1
    sta right+1
:
    rts

binary_remainder:
    lda right
    ora right+1
    jeq fail_arithmetic
    lda left
    bne :+
    lda left+1
    cmp #$80
    bne :+
    lda right
    cmp #$ff
    bne :+
    lda right+1
    cmp #$ff
    jeq fail_arithmetic
:
    lda left+1
    and #$80
    sta sign
    jsr absolute_pair
    lda #0
    sta value
    sta value+1
    lda #16
    sta count
remainder_loop:
    asl left
    rol left+1
    rol value
    rol value+1
    lda value+1
    cmp right+1
    bcc :+
    jne remainder_subtract
    lda value
    cmp right
    bcc :+
remainder_subtract:
    sec
    lda value
    sbc right
    sta value
    lda value+1
    sbc right+1
    sta value+1
:
    dec count
    jne remainder_loop
    lda sign
    jeq binary_commit
    sec
    lda #0
    sbc value
    sta value
    lda #0
    sbc value+1
    sta value+1

binary_commit:
    lda tbx_data_depth
    sec
    sbc #2
    asl
    tax
    lda value
    sta tbx_data_stack,x
    inx
    lda value+1
    sta tbx_data_stack,x
    dec tbx_data_depth
    jmp commit_cursor

op_putdec:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq commit_cursor
    sec
    sbc #1
    asl
    tax
    lda tbx_data_stack,x
    sta value
    inx
    lda tbx_data_stack,x
    sta value+1
    lda value+1
    bpl :+
    lda #'-'
    jsr emit_char
    jcs fail_output
    sec
    lda #0
    sbc value
    sta value
    lda #0
    sbc value+1
    sta value+1
:
    lda #0
    sta count
decimal_digit:
    lda #0
    sta work
    sta work+1
decimal_divide:
    lda value+1
    cmp #0
    jne decimal_subtract
    lda value
    cmp #10
    jcc decimal_done
decimal_subtract:
    sec
    lda value
    sbc #10
    sta value
    lda value+1
    sbc #0
    sta value+1
    inc work
    jne decimal_divide
    inc work+1
    jmp decimal_divide
decimal_done:
    ldx count
    lda value
    clc
    adc #'0'
    sta digits,x
    inc count
    lda work
    sta value
    lda work+1
    sta value+1
    ora value
    jne decimal_digit
decimal_emit:
    dec count
    ldx count
    lda digits,x
    jsr emit_char
    jcs fail_output
    lda count
    jne decimal_emit
    dec tbx_data_depth
    jmp commit_cursor

op_cr:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda #10
    jsr emit_char
    jcs fail_output
    jmp commit_cursor

emit_char:
    ldx #0
    jsr _putchar
    cpx #$ff
    bne :+
    cmp #$ff
    beq :++
:
    clc
    rts
:
    sec
    rts

fail_opcode:
    lda #10
    jne fail
fail_bytecode:
    lda #11
    jne fail
fail_underflow:
    lda #12
    jne fail
fail_overflow:
    lda #13
    jne fail
fail_call_underflow:
    lda #14
    jne fail
fail_call_overflow:
    lda #15
    jne fail
fail_global:
    lda #16
    jne fail
fail_arithmetic:
    lda #17
    jne fail
fail_output:
    lda #18
    jne fail
fail_invariant:
    lda #19
fail:
    sta tbx_last_error
    jsr _tbx_error_probe
    lda tbx_last_error
    rts
