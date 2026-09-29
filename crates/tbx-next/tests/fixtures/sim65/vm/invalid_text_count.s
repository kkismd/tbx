.include "vm_fixture.inc"
VM_TEXT_HEADER entry, 0, 0, 0, 257, descriptors
entry:
    HALT
VM_END
.segment "RODATA"
descriptors: .word 0, 0
