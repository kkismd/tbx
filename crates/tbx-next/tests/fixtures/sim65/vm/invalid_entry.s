.export _tbx_code_start, _tbx_code_end, _tbx_entry_offset, _tbx_global_count
.segment "RODATA"
_tbx_entry_offset: .word $ffff
_tbx_global_count: .word 0
_tbx_code_start:
    .byte $01
_tbx_code_end:
