.include "vm_fixture.inc"
VM_HEADER entry, 0, 2, array_descriptors
entry:
    PUSH 777
    PUSH 1
    LOAD_ARRAY 0
    PUTDEC
    CR
    PUSH 2
    LOAD_ARRAY 0
    PUTDEC
    CR
    PUSH 1
    PUSH -42
    STORE_ARRAY 1
    PUSH 1
    LOAD_ARRAY 1
    PUTDEC
    CR
    DROP
    HALT
VM_END

.segment "RODATA"
array_descriptors:
    .word array_a, 2
    .word array_b, 3
.segment "DATA"
array_a: .word 1234, $e9d2
array_b: .word 7, 8, 9
