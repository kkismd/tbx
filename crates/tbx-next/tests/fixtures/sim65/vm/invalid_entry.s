.export _tbx_code_start, _tbx_code_end, _tbx_entry_offset, _tbx_global_count
.export _tbx_array_count, _tbx_array_descriptors
.export _tbx_text_count, _tbx_text_descriptors
.segment "RODATA"
_tbx_entry_offset: .word $ffff
_tbx_global_count: .word 0
_tbx_array_count: .word 0
_tbx_array_descriptors: .word 0
_tbx_text_count: .word 0
_tbx_text_descriptors: .word 0
_tbx_code_start:
    .byte $01
_tbx_code_end:
