; M32 private bytecode contract shared by hand-written sim65 fixtures.
.macro VM_HEADER entry, globals
    .export _tbx_code_start, _tbx_code_end
    .export _tbx_entry_offset, _tbx_global_count
    .segment "RODATA"
_tbx_entry_offset: .word entry - _tbx_code_start
_tbx_global_count: .word globals
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
.macro DROP
    .byte $50
.endmacro
.macro PUTDEC
    .byte $60
.endmacro
.macro CR
    .byte $61
.endmacro

; Error probe record: code, opcode address, depths, stack bytes, global,
; and active frame bytes. $ff disables the global check.
.macro VM_EXPECT code, pc, data_depth, call_depth, stack_ptr, stack_len, global_slot, global_value, frame_ptr, frame_len
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
.endmacro
