; M32 private bytecode runtime for the sim6502 target.
; The bytecode wrapper supplies four RODATA symbols. The two hooks are private
; M32 test instrumentation; ordinary wrappers implement them as no-ops.
.setcpu "6502"
.macpack longbranch

.import _tbx_code_start, _tbx_code_end, _tbx_entry_offset, _tbx_global_count
.import _tbx_before_init, _tbx_error_probe, _putchar
.import _tbx_array_count, _tbx_array_descriptors
.import _tbx_text_count, _tbx_text_descriptors
.import _tbx_read_byte
.export _main
.export tbx_pc, tbx_base, tbx_end, tbx_data_depth, tbx_control_depth
.export tbx_call_depth, tbx_global_count, tbx_last_error
.export tbx_data_stack, tbx_control_stack, tbx_globals, tbx_frames
.export tbx_rng_state
.export vm_zp_end, vm_bss_end
.export vm_code_start, vm_code_end, vm_rodata_start, vm_rodata_end

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
vm_zp_end:

.segment "BSS"
tbx_data_stack: .res 128
tbx_frames:     .res 320
tbx_globals:    .res 512
digits:         .res 6
tbx_control_stack: .res 32
; ADR #1889 explicit seeds require the same xorshift/multiply sequence on all targets.
tbx_rng_state: .res 8
rng_work: .res 8
rng_shift: .res 8
rng_multiplicand: .res 8
rng_product: .res 8
rng_bound: .res 2
rng_remainder: .res 2
rng_shift_count: .res 1
rng_bit_count: .res 1
rng_bit_value: .res 1
input_status: .res 1
input_byte: .res 1
input_state: .res 1
input_sign: .res 1
input_seen: .res 1
input_overflow: .res 1
input_digit_value: .res 1
input_accumulator: .res 2
vm_bss_end:

.segment "RODATA"
vm_rodata_start:
frame_addresses:
.repeat 16, I
    .word tbx_frames + I * 20
.endrepeat
vm_rodata_end:

.segment "CODE"
vm_code_start:
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
    ; The private fixture may provide a seed before VM startup. Seed zero uses
    ; the ADR #1889 normalization value, matching the host RandomState.
    lda tbx_rng_state
    ora tbx_rng_state+1
    ora tbx_rng_state+2
    ora tbx_rng_state+3
    ora tbx_rng_state+4
    ora tbx_rng_state+5
    ora tbx_rng_state+6
    ora tbx_rng_state+7
    bne :+
    lda #$15
    sta tbx_rng_state
    lda #$7c
    sta tbx_rng_state+1
    lda #$4a
    sta tbx_rng_state+2
    lda #$7f
    sta tbx_rng_state+3
    lda #$b9
    sta tbx_rng_state+4
    lda #$79
    sta tbx_rng_state+5
    lda #$37
    sta tbx_rng_state+6
    lda #$9e
    sta tbx_rng_state+7
:
    ; Count is bounded before touching the globals allocation.
    lda tbx_global_count+1
    beq :+
    cmp #1
    jne fail_global
    lda tbx_global_count
    jne fail_global
:
    lda _tbx_array_count+1
    beq :+
    cmp #1
    jne fail_array_metadata
    lda _tbx_array_count
    jne fail_array_metadata
:
    lda _tbx_text_count+1
    cmp #1
    bcc :+
    jne fail_text_metadata
    lda _tbx_text_count
    jne fail_text_metadata
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
    cmp #$12
    jeq op_load_array
    cmp #$13
    jeq op_store_array
    cmp #$20
    jeq op_call
    cmp #$21
    jeq op_copy_base
    cmp #$22
    jeq op_return
    cmp #$70
    jeq op_control_push
    cmp #$71
    jeq op_control_copy
    cmp #$72
    jeq op_control_drop
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
    cmp #$43
    jeq op_binary
    cmp #$44
    jeq op_binary
    cmp #$47
    jeq op_binary
    cmp #$48
    jeq op_binary
    cmp #$49
    jeq op_binary
    cmp #$4a
    jeq op_binary
    cmp #$4b
    jeq op_binary
    cmp #$4c
    jeq op_binary
    cmp #$4d
    jeq op_binary
    cmp #$4f
    jeq op_binary
    cmp #$4e
    jeq op_swap
    cmp #$45
    jeq op_unary
    cmp #$46
    jeq op_unary
    cmp #$50
    jeq op_drop
    cmp #$51
    jeq op_rnd
    cmp #$52
    jeq op_try_input
    cmp #$60
    jeq op_putdec
    cmp #$61
    jeq op_cr
    cmp #$62
    jeq op_putchr
    cmp #$63
    jeq op_write_text
    jmp fail_opcode

op_halt:
    lda #0
    rts

; TRY_INPUT validates both commit prerequisites before calling the external
; hook. It parses and drains incrementally without a line buffer. Parser scratch
; is private VM state; architectural state commits only after the full line.
op_try_input:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    cmp #63
    jcs fail_overflow
    lda #0
    sta input_state
    sta input_sign
    sta input_seen
    sta input_overflow
    sta input_accumulator
    sta input_accumulator+1
input_read:
    ; The hook returns A=0/X=byte, A=1 for EOF, and A=2 for I/O failure.
    jsr _tbx_read_byte
    cmp #2
    jeq fail_input
    cmp #1
    jeq input_eof
    cmp #0
    jne fail_input
    stx input_byte
    lda input_byte
    cmp #10
    jeq input_line_end
    cmp #13
    jeq input_cr
    jsr input_consume
    jmp input_read
input_cr:
    ; CR is only ignored when immediately followed by LF.
    jsr _tbx_read_byte
    cmp #2
    jeq fail_input
    cmp #1
    jeq input_invalid_eof
    cmp #0
    jne fail_input
    cpx #10
    jne input_invalid_cr
    jmp input_line_end
input_invalid_cr:
    lda #1
    sta input_overflow
    stx input_byte
    lda input_byte
    cmp #10
    jeq input_line_end
    jsr input_consume
    jmp input_read
input_invalid_eof:
    lda #1
    sta input_overflow
    jmp input_finish
input_eof:
    lda input_seen
    beq input_empty
    jmp input_finish
input_line_end:
    lda input_seen
    beq input_empty
    jmp input_finish
input_empty:
    lda #0
    sta input_accumulator
    sta input_accumulator+1
    sta input_overflow
    sta input_status
    jmp input_push
input_consume:
    lda #1
    sta input_seen
    lda input_overflow
    jne input_consume_done
    lda input_byte
    cmp #' '
    jeq input_space
    cmp #9
    jeq input_space
    lda input_state
    cmp #3
    jeq input_invalid
    lda input_byte
    cmp #'+'
    jeq input_plus
    cmp #'-'
    jeq input_minus
    cmp #'0'
    jcc input_invalid
    cmp #'9'+1
    jcs input_invalid
    sec
    sbc #'0'
    sta input_digit_value
    lda #2
    sta input_state
input_digit:
    ; magnitude = magnitude * 10 + digit, bounded to 32767/32768.
    lda input_accumulator
    sta left
    lda input_accumulator+1
    sta left+1
    lda left
    asl
    sta right
    lda left+1
    rol
    sta right+1
    lda left
    asl
    sta input_accumulator
    lda left+1
    rol
    sta input_accumulator+1
    asl input_accumulator
    rol input_accumulator+1
    asl input_accumulator
    rol input_accumulator+1
    clc
    lda input_accumulator
    adc right
    sta input_accumulator
    lda input_accumulator+1
    adc right+1
    sta input_accumulator+1
    bcs input_invalid
    clc
    lda input_accumulator
    adc input_digit_value
    sta input_accumulator
    lda input_accumulator+1
    adc #0
    sta input_accumulator+1
    bcs input_invalid
    lda input_accumulator+1
    cmp #$80
    bcc input_consume_done
    bne input_invalid
    lda input_sign
    jeq input_invalid
input_negative_limit:
    lda input_accumulator
    bne input_invalid
input_consume_done:
    rts
input_space:
    lda input_state
    cmp #2
    beq input_trailing_start
    cmp #1
    jeq input_invalid
    rts
input_trailing_start:
    lda #3
    sta input_state
    rts
input_plus:
    jmp input_sign_char
input_minus:
    lda #1
    sta input_sign
input_sign_char:
    lda input_state
    bne input_invalid
    lda #1
    sta input_state
    rts
input_invalid:
    lda #1
    sta input_overflow
    rts
input_finish:
    lda input_state
    cmp #2
    jeq input_finish_valid
    cmp #3
    jne input_empty
input_finish_valid:
    lda input_overflow
    jne input_empty
    lda input_accumulator
    sta value
    lda input_accumulator+1
    sta value+1
    lda input_sign
    beq input_push_success
    sec
    lda #0
    sbc value
    sta value
    lda #0
    sbc value+1
    sta value+1
input_push_success:
    lda #1
    sta input_status
    lda value
    sta left
    lda value+1
    sta left+1
    lda #1
    bne input_store
input_push:
    lda #0
    sta left
    sta left+1
    lda #0
input_store:
    ldx tbx_data_depth
    txa
    asl
    tax
    lda left
    sta tbx_data_stack,x
    inx
    lda left+1
    sta tbx_data_stack,x
    inx
    lda input_status
    sta tbx_data_stack,x
    inx
    lda #0
    sta tbx_data_stack,x
    inc tbx_data_depth
    inc tbx_data_depth
    jmp commit_cursor

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

; Array descriptors are four-byte base/length pairs owned by the wrapper.
; ptr receives the selected descriptor address; target receives element address.
array_descriptor:
    ; The opcode handler already consumed and preserved the slot operand.
    lda _tbx_array_count+1
    beq array_count_u8
    cmp #1
    bne array_slot_bad
    lda _tbx_array_count
    bne array_slot_bad
    jmp array_slot_valid
array_count_u8:
    lda count
    cmp _tbx_array_count
    bcc array_slot_valid
    jmp array_slot_bad
array_slot_valid:
    lda count
    asl
    sta left
    lda #0
    rol
    asl left
    rol
    sta left+1
    clc
    lda _tbx_array_descriptors
    adc left
    sta ptr
    lda _tbx_array_descriptors+1
    adc left+1
    sta ptr+1
    jcs array_metadata_bad
    ldy #0
    lda (ptr),y
    sta target
    iny
    lda (ptr),y
    sta target+1
    iny
    lda (ptr),y
    sta right
    iny
    lda (ptr),y
    sta right+1
    ; Length must be positive and fit the signed positive i16 range.
    lda right+1
    bmi array_metadata_bad
    ora right
    jeq array_metadata_bad
    clc
    rts
array_slot_bad:
    lda #20
    sec
    rts
array_metadata_bad:
    lda #20
    sec
    rts

array_access:
    ; value contains the signed one-based index.
    lda value+1
    and #$80
    bne array_index_bad
    lda value+1
    ora value
    bne array_index_positive
    jmp array_index_bad
array_index_positive:
    lda value+1
    cmp right+1
    bcc array_index_in_range
    bne array_index_bad
    lda value
    cmp right
    bcc array_index_in_range
    beq array_index_in_range
array_index_bad:
    lda #21
    sec
    rts
array_index_in_range:
    sec
    lda value
    sbc #1
    sta value
    lda value+1
    sbc #0
    sta value+1
    asl value
    rol value+1
    bcs array_metadata_bad
    clc
    lda target
    adc value
    sta target
    lda target+1
    adc value+1
    sta target+1
    bcs array_metadata_bad
    clc
    rts

op_load_array:
    lda #2
    jsr need_bytes
    jcs fail_bytecode
    ; Preserve slot while checking the successor and stack before metadata.
    ldy #0
    lda (cursor),y
    sta count
    jsr read_operand
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    jsr array_descriptor
    jcs fail
    ldx tbx_data_depth
    dex
    txa
    asl
    tax
    lda tbx_data_stack,x
    sta value
    inx
    lda tbx_data_stack,x
    sta value+1
    jsr array_access
    jcs fail
    ldy #0
    lda (target),y
    sta value
    iny
    lda (target),y
    sta value+1
    ldx tbx_data_depth
    dex
    txa
    asl
    tax
    lda value
    sta tbx_data_stack,x
    inx
    lda value+1
    sta tbx_data_stack,x
    jmp commit_cursor

op_store_array:
    lda #2
    jsr need_bytes
    jcs fail_bytecode
    ldy #0
    lda (cursor),y
    sta count
    jsr read_operand
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    cmp #2
    jcc fail_underflow
    jsr array_descriptor
    jcs fail
    ; value is the cell to store; the index is the cell beneath it.
    ldx tbx_data_depth
    dex
    txa
    asl
    tax
    lda tbx_data_stack,x
    sta work
    inx
    lda tbx_data_stack,x
    sta work+1
    dex
    dex
    dex
    lda tbx_data_stack,x
    sta value
    inx
    lda tbx_data_stack,x
    sta value+1
    jsr array_access
    jcs fail
    ; No failure is possible after this point: commit storage, stack, and PC.
    ldy #0
    lda work
    sta (target),y
    iny
    lda work+1
    sta (target),y
    dec tbx_data_depth
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

; Control values are signed i16 cells indexed by the current depth.
op_control_push:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    lda tbx_control_depth
    cmp #16
    jcs fail_control_overflow
    ; Copy the data top before committing either depth.
    lda tbx_data_depth
    sec
    sbc #1
    asl
    tax
    lda tbx_data_stack,x
    sta left
    inx
    lda tbx_data_stack,x
    sta left+1
    lda tbx_control_depth
    asl
    tax
    lda left
    sta tbx_control_stack,x
    inx
    lda left+1
    sta tbx_control_stack,x
    dec tbx_data_depth
    inc tbx_control_depth
    jmp commit_cursor

op_control_copy:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_control_depth
    jeq fail_control_underflow
    lda tbx_data_depth
    cmp #64
    jcs fail_overflow
    lda tbx_control_depth
    sec
    sbc #1
    asl
    tax
    lda tbx_control_stack,x
    sta left
    inx
    lda tbx_control_stack,x
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

op_control_drop:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_control_depth
    jeq fail_control_underflow
    dec tbx_control_depth
    jmp commit_cursor

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

op_rnd:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    sec
    sbc #1
    asl
    tax
    lda tbx_data_stack,x
    sta rng_bound
    inx
    lda tbx_data_stack,x
    sta rng_bound+1
    ; The upper bound is a positive signed i16, including 32767.
    bpl rnd_nonnegative_bound
    jmp fail_random_bound
rnd_nonnegative_bound:
    ora rng_bound
    jeq fail_random_bound
    ; Work on a private copy so every subsequent arithmetic step is speculative.
    ldx #7
rnd_copy_state:
    lda tbx_rng_state,x
    sta rng_work,x
    dex
    bpl rnd_copy_state
    ; xorshift64: x ^= x >> 12; x ^= x << 25; x ^= x >> 27.
    lda #12
    sta rng_shift_count
rnd_shift_right_12:
    lsr rng_work+7
    ror rng_work+6
    ror rng_work+5
    ror rng_work+4
    ror rng_work+3
    ror rng_work+2
    ror rng_work+1
    ror rng_work
    dec rng_shift_count
    bne rnd_shift_right_12
    ldx #0
rnd_xor_right_12:
    lda tbx_rng_state,x
    eor rng_work,x
    sta rng_work,x
    inx
    cpx #8
    bne rnd_xor_right_12
    ldx #7
rnd_copy_left_source:
    lda rng_work,x
    sta rng_shift,x
    dex
    bpl rnd_copy_left_source
    lda #25
    sta rng_shift_count
rnd_shift_left_25:
    asl rng_shift
    rol rng_shift+1
    rol rng_shift+2
    rol rng_shift+3
    rol rng_shift+4
    rol rng_shift+5
    rol rng_shift+6
    rol rng_shift+7
    dec rng_shift_count
    bne rnd_shift_left_25
    ldx #0
rnd_xor_left_25:
    lda rng_work,x
    eor rng_shift,x
    sta rng_work,x
    inx
    cpx #8
    bne rnd_xor_left_25
    ldx #7
rnd_copy_right_source:
    lda rng_work,x
    sta rng_shift,x
    dex
    bpl rnd_copy_right_source
    lda #27
    sta rng_shift_count
rnd_shift_right_27:
    lsr rng_shift+7
    ror rng_shift+6
    ror rng_shift+5
    ror rng_shift+4
    ror rng_shift+3
    ror rng_shift+2
    ror rng_shift+1
    ror rng_shift
    dec rng_shift_count
    bne rnd_shift_right_27
    ldx #0
rnd_xor_right_27:
    lda rng_work,x
    eor rng_shift,x
    sta rng_work,x
    inx
    cpx #8
    bne rnd_xor_right_27
    ; Multiply by 0x2545_F491_4F6C_DD1D modulo 2^64.
    ldx #7
rnd_copy_multiplicand:
    lda rng_work,x
    sta rng_multiplicand,x
    dex
    bpl rnd_copy_multiplicand
    ldx #7
rnd_clear_product:
    lda #0
    sta rng_product,x
    dex
    bpl rnd_clear_product
    ldx #0
rnd_multiply_byte:
    lda rnd_multiplier,x
    sta rng_bit_value
    ldy #8
rnd_multiply_bit:
    lsr rng_bit_value
    bcc rnd_multiply_skip_add
    clc
    lda rng_product
    adc rng_multiplicand
    sta rng_product
    lda rng_product+1
    adc rng_multiplicand+1
    sta rng_product+1
    lda rng_product+2
    adc rng_multiplicand+2
    sta rng_product+2
    lda rng_product+3
    adc rng_multiplicand+3
    sta rng_product+3
    lda rng_product+4
    adc rng_multiplicand+4
    sta rng_product+4
    lda rng_product+5
    adc rng_multiplicand+5
    sta rng_product+5
    lda rng_product+6
    adc rng_multiplicand+6
    sta rng_product+6
    lda rng_product+7
    adc rng_multiplicand+7
    sta rng_product+7
rnd_multiply_skip_add:
    cpx #7
    bne rnd_multiply_shift
    cpy #1
    beq rnd_multiply_no_shift
rnd_multiply_shift:
    asl rng_multiplicand
    rol rng_multiplicand+1
    rol rng_multiplicand+2
    rol rng_multiplicand+3
    rol rng_multiplicand+4
    rol rng_multiplicand+5
    rol rng_multiplicand+6
    rol rng_multiplicand+7
rnd_multiply_no_shift:
    dey
    bne rnd_multiply_bit
    inx
    cpx #8
    bne rnd_multiply_byte
    ; Long division yields product % bound. Remainder always remains < bound.
    lda #0
    sta rng_remainder
    sta rng_remainder+1
    ldx #7
rnd_divide_byte:
    lda #8
    sta rng_bit_count
rnd_divide_bit:
    asl rng_product,x
    rol rng_remainder
    rol rng_remainder+1
    lda rng_remainder+1
    cmp rng_bound+1
    bcc rnd_divide_no_subtract
    bne rnd_divide_subtract
    lda rng_remainder
    cmp rng_bound
    bcc rnd_divide_no_subtract
rnd_divide_subtract:
    sec
    lda rng_remainder
    sbc rng_bound
    sta rng_remainder
    lda rng_remainder+1
    sbc rng_bound+1
    sta rng_remainder+1
rnd_divide_no_subtract:
    dec rng_bit_count
    bne rnd_divide_bit
    dex
    bpl rnd_divide_byte
    ; All fallible validation is complete: publish state and replace the bound.
    ldx #7
rnd_commit_state:
    lda rng_work,x
    sta tbx_rng_state,x
    dex
    bpl rnd_commit_state
    clc
    lda rng_remainder
    adc #1
    sta value
    lda rng_remainder+1
    adc #0
    sta value+1
    sec
    lda tbx_data_depth
    sbc #1
    asl
    tax
    lda value
    sta tbx_data_stack,x
    inx
    lda value+1
    sta tbx_data_stack,x
    jmp commit_cursor

.segment "RODATA"
rnd_multiplier: .byte $1d,$dd,$6c,$4f,$91,$f4,$45,$25
.segment "CODE"

op_swap:
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
    sec
    lda tbx_data_depth
    sbc #2
    asl
    tax
    lda right
    sta tbx_data_stack,x
    inx
    lda right+1
    sta tbx_data_stack,x
    inx
    lda left
    sta tbx_data_stack,x
    inx
    lda left+1
    sta tbx_data_stack,x
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
    cmp #$43
    jeq binary_subtract
    cmp #$44
    jeq binary_divide
    cmp #$41
    jeq binary_multiply
    cmp #$42
    jeq binary_remainder
    cmp #$47
    jeq binary_and
    cmp #$4d
    jeq binary_or
    jmp binary_compare

; Logical operands are already evaluated; normalize signed i16 truthiness to 0/1.
binary_and:
    lda left
    ora left+1
    jeq binary_logic_false
    lda right
    ora right+1
    jeq binary_logic_false
    lda #1
    bne binary_logic_commit
binary_or:
    lda left
    ora left+1
    jne binary_logic_true
    lda right
    ora right+1
    jeq binary_logic_false
binary_logic_true:
    lda #1
    bne binary_logic_commit
binary_logic_false:
    lda #0
binary_logic_commit:
    sta value
    lda #0
    sta value+1
    jmp binary_commit

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

binary_subtract:
    sec
    lda left
    sbc right
    sta value
    lda left+1
    sbc right+1
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
    cmp #$4f
    jeq compare_not_equal
    cmp #$4c
    jeq compare_greater
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
compare_not_equal:
    lda count
    jne compare_true
    jmp compare_false
compare_greater:
    lda count
    cmp #$ff
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
    jmp binary_commit

; Divide magnitudes with a restoring 16-step quotient. Signed division truncates
; toward zero; MIN/-1 is rejected, while MIN/1 is representable.
binary_divide:
    lda right
    ora right+1
    jeq fail_arithmetic
    lda left
    jne divide_min_check_done
    lda left+1
    cmp #$80
    jne divide_min_check_done
    lda right
    cmp #$ff
    jne divide_min_check_done
    lda right+1
    cmp #$ff
    jeq fail_arithmetic
divide_min_check_done:
    lda left+1
    eor right+1
    and #$80
    sta sign
    jsr absolute_pair
    lda #0
    sta value
    sta value+1
    lda #16
    sta count
divide_loop:
    asl left
    rol left+1
    rol value
    rol value+1
    lda value+1
    cmp right+1
    bcc divide_skip_subtract
    bne divide_subtract
    lda value
    cmp right
    bcc divide_skip_subtract
divide_subtract:
    sec
    lda value
    sbc right
    sta value
    lda value+1
    sbc right+1
    sta value+1
    inc left
divide_skip_subtract:
    dec count
    jne divide_loop
    lda sign
    jeq divide_positive
    sec
    lda #0
    sbc left
    sta value
    lda #0
    sbc left+1
    sta value+1
    jmp binary_commit
divide_positive:
    lda left+1
    bpl :+
    jmp fail_arithmetic
:
    lda left
    sta value
    lda left+1
    sta value+1
    jmp binary_commit

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

op_unary:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    sec
    sbc #1
    asl
    tax
    lda tbx_data_stack,x
    sta value
    inx
    lda tbx_data_stack,x
    sta value+1
    lda opcode
    cmp #$45
    jeq unary_negate
    lda value
    jne unary_abs_ready
    lda value+1
    cmp #$80
    jeq fail_arithmetic
unary_abs_ready:
    lda value+1
    bpl unary_commit
    jmp unary_negate_value
unary_negate:
    lda value
    jne unary_negate_value
    lda value+1
    cmp #$80
    jeq fail_arithmetic
unary_negate_value:
    sec
    lda #0
    sbc value
    sta value
    lda #0
    sbc value+1
    sta value+1
unary_commit:
    lda tbx_data_depth
    sec
    sbc #1
    asl
    tax
    lda value
    sta tbx_data_stack,x
    inx
    lda value+1
    sta tbx_data_stack,x
    jmp commit_cursor

op_putdec:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
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

op_putchr:
    lda #1
    jsr need_bytes
    jcs fail_bytecode
    jsr validate_next
    jcs fail_bytecode
    lda tbx_data_depth
    jeq fail_underflow
    sec
    sbc #1
    asl
    tax
    lda tbx_data_stack,x
    sta value
    inx
    lda tbx_data_stack,x
    sta value+1
    jne fail_arithmetic
    lda value
    jmi fail_arithmetic
    jsr emit_char
    jcs fail_output
    dec tbx_data_depth
    jmp commit_cursor

; WRITE_TEXT validates all metadata and the fallthrough address before output.
op_write_text:
    lda #2
    jsr need_bytes
    jcs fail_bytecode
    ldy #0
    lda (cursor),y
    sta count
    jsr read_operand
    jsr validate_next
    jcs fail_bytecode
    jsr text_descriptor
    jcs fail_text_metadata
    ; right is the byte length. Empty text does not dereference its base.
    lda right
    ora right+1
    jeq write_text_done
    ; Validate base + (length - 1), allowing the final byte at $ffff.
    sec
    lda right
    sbc #1
    sta work
    lda right+1
    sbc #0
    sta work+1
    clc
    lda target
    adc work
    sta work
    lda target+1
    adc work+1
    jcs fail_text_metadata
    ; Restore length for the output loop; work becomes its remaining count.
    lda right
    sta work
    lda right+1
    sta work+1
write_text_loop:
    ldy #0
    lda (target),y
    jsr emit_char
    jcs fail_output
    inc target
    bne :+
    inc target+1
:
    lda work
    bne :+
    dec work+1
:
    dec work
    lda work
    ora work+1
    jne write_text_loop
write_text_done:
    jmp commit_cursor

; Resolve the saved slot to target=base and right=length.
text_descriptor:
    lda _tbx_text_count+1
    beq text_count_u8
    cmp #1
    bne text_metadata_bad
    lda _tbx_text_count
    bne text_metadata_bad
    jmp text_slot_valid
text_count_u8:
    lda count
    cmp _tbx_text_count
    bcc text_slot_valid
    jmp text_metadata_bad
text_slot_valid:
    lda count
    asl
    sta left
    lda #0
    rol
    asl left
    rol
    sta left+1
    clc
    lda _tbx_text_descriptors
    adc left
    sta ptr
    lda _tbx_text_descriptors+1
    adc left+1
    sta ptr+1
    jcs text_metadata_bad
    clc
    lda ptr
    adc #3
    lda ptr+1
    adc #0
    bcs text_metadata_bad
    ldy #0
    lda (ptr),y
    sta target
    iny
    lda (ptr),y
    sta target+1
    iny
    lda (ptr),y
    sta right
    iny
    lda (ptr),y
    sta right+1
    clc
    rts
text_metadata_bad:
    sec
    rts

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
fail_random_bound:
    lda #25
    jne fail
fail_input:
    lda #26
    jne fail
fail_output:
    lda #18
    jne fail
fail_invariant:
    lda #19
    jne fail
fail_array_slot:
    lda #20
    jne fail
fail_array_metadata:
    lda #20
    jne fail
fail_text_metadata:
    lda #22
    jne fail
fail_array_index:
    lda #21
    jne fail
fail_control_underflow:
    lda #23
    jne fail
fail_control_overflow:
    lda #24
fail:
    sta tbx_last_error
    jsr _tbx_error_probe
    lda tbx_last_error
    rts
vm_code_end:
