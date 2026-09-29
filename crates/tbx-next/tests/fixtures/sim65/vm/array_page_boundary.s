.include "vm_fixture.inc"
VM_HEADER entry, 0, 1, descriptors
    ; Place the array opcode at $xxff so its slot operand is at $xx00.
    .res $fc, $01
entry:
    PUSH 1
    LOAD_ARRAY 0
    PUTDEC
    CR
    HALT
VM_END
.segment "RODATA"
descriptors: .word storage, 1
.segment "DATA"
storage: .word 321
