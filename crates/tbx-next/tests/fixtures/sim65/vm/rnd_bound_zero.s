.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 0
failure:
    RND
    HALT
VM_END
expected_stack: .word 0
VM_EXPECT 25, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK | VM_CHECK_RNG
