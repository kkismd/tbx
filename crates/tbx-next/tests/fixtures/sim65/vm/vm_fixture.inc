; M32 private bytecode contract shared by hand-written sim65 fixtures.
.macro VM_HEADER entry, globals, arrays, descriptors
    .export _tbx_code_start, _tbx_code_end
    .export _tbx_entry_offset, _tbx_global_count
    .export _tbx_array_count, _tbx_array_descriptors
    .export _tbx_text_count, _tbx_text_descriptors
    .segment "RODATA"
_tbx_entry_offset: .word entry - _tbx_code_start
_tbx_global_count: .word globals
_tbx_array_count: .word arrays
_tbx_array_descriptors: .word descriptors
_tbx_text_count: .word 0
_tbx_text_descriptors: .word empty_text_descriptors
empty_text_descriptors: .word 0, 0
_tbx_code_start:
.endmacro

.macro VM_TEXT_HEADER entry, globals, arrays, descriptors, texts, text_descriptors
    .export _tbx_code_start, _tbx_code_end
    .export _tbx_entry_offset, _tbx_global_count
    .export _tbx_array_count, _tbx_array_descriptors
    .export _tbx_text_count, _tbx_text_descriptors
    .segment "RODATA"
_tbx_entry_offset: .word entry - _tbx_code_start
_tbx_global_count: .word globals
_tbx_array_count: .word arrays
_tbx_array_descriptors: .word descriptors
_tbx_text_count: .word texts
_tbx_text_descriptors: .word text_descriptors
_tbx_code_start:
.endmacro

.macro VM_END
_tbx_code_end:
.endmacro

.macro PUSH value
    .byte $02
    .word (value) & $ffff
.endmacro
.macro LOAD slot
    .byte $10, slot
.endmacro
.macro STORE slot
    .byte $11, slot
.endmacro
.macro LOAD_ARRAY slot
    .byte $12, slot
.endmacro
.macro STORE_ARRAY slot
    .byte $13, slot
.endmacro
.macro CALL target
    .byte $20
    .word target - _tbx_code_start
.endmacro
.macro COPY_BASE offset
    .byte $21, offset
.endmacro
.macro RET
    .byte $22
.endmacro
.macro VM_JUMP target
    .byte $30
    .word target - _tbx_code_start
.endmacro
.macro JZ target
    .byte $31
    .word target - _tbx_code_start
.endmacro
.macro HALT
    .byte $01
.endmacro
.macro ADD
    .byte $40
.endmacro
.macro MUL
    .byte $41
.endmacro
.macro REM
    .byte $42
.endmacro
.macro SUB
    .byte $43
.endmacro
.macro DIV
    .byte $44
.endmacro
.macro NEG
    .byte $45
.endmacro
.macro ABS
    .byte $46
.endmacro
.macro EQ
    .byte $48
.endmacro
.macro LT
    .byte $49
.endmacro
.macro LE
    .byte $4a
.endmacro
.macro GE
    .byte $4b
.endmacro
.macro GT
    .byte $4c
.endmacro
.macro DROP
    .byte $50
.endmacro
.macro PUTDEC
    .byte $60
.endmacro
.macro CR
    .byte $61
.endmacro
.macro PUTCHR
    .byte $62
.endmacro
.macro WRITE_TEXT slot
    .byte $63, slot
.endmacro

; Error probe record: code, opcode address, depths, stack, optional global,
; active frame, and optional unchanged array storage bytes.
.macro VM_EXPECT code, pc, data_depth, call_depth, stack_ptr, stack_len, global_slot, global_value, frame_ptr, frame_len, array_ptr, expected_array_ptr, array_len
    .export _tbx_test_expect
    .segment "RODATA"
_tbx_test_expect:
    .byte code
    .word pc
    .byte data_depth, 0, call_depth
    .byte stack_len
    .word stack_ptr
    .byte global_slot
    .word global_value
    .byte frame_len
    .word frame_ptr
    .word array_ptr
    .word expected_array_ptr
    .byte array_len
.endmacro
