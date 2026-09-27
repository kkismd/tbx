.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH 128
failure:
    PUTCHR
    HALT
VM_END
expected_stack: .word 128
VM_EXPECT 17, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0
