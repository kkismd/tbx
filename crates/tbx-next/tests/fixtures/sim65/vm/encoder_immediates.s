.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    ; Matches bytecode_6502::encodes_signed_immediates_as_little_endian.
    .byte $02, $34, $12, $02, $fe, $ff, $02, $ff, $7f, $01
VM_END
