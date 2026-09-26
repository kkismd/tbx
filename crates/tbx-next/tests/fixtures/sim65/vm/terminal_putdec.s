.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 42
failure:
    PUTDEC
VM_END
expected_stack: .word 42
VM_EXPECT 11, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0
