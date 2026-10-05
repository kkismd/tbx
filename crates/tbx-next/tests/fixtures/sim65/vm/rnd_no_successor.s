.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    PUSH 10
failure:
    RND
VM_END
expected_stack: .word 10
VM_EXPECT 11, failure, 1, 0, expected_stack, 2, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK | VM_CHECK_RNG
