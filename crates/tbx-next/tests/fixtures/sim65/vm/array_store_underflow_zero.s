.include "vm_fixture.inc"
VM_HEADER entry, 0, 1, descriptors
entry:
failure:
    STORE_ARRAY 1
    HALT
VM_END
.segment "RODATA"
descriptors: .word storage, 1
.segment "DATA"
storage: .word 321
expected: .word 321
VM_EXPECT 12, failure, 0, 0, 0, 0, $ff, 0, 0, 0, storage, expected, 2, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK | VM_CHECK_BYTES
