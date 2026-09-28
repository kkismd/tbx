.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
failure:
    PUTDEC
    HALT
VM_END
expected_stack: .word 0
VM_EXPECT 12, failure, 0, 0, expected_stack, 0, $ff, 0, 0, 0
