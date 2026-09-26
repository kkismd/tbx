.include "vm_fixture.inc"
VM_HEADER entry, 1
entry:
    .byte $02, $34, $12
    .byte $02, $fe, $ff
    .byte $40
    .byte $11, $00
    .byte $10, $00
    .byte $60
    .byte $61
    .byte $01
VM_END
