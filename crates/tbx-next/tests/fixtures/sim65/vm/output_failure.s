.include "vm_fixture.inc"
.export _putchar
VM_HEADER entry, 0
entry:
    PUSH 42
failure:
    PUTDEC
    HALT
VM_END
expected_stack: .word 42
VM_EXPECT 18, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0
.segment "CODE"
_putchar:
    lda #$ff
    tax
    rts
