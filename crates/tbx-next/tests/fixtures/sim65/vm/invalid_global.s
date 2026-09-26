.include "vm_fixture.inc"
VM_HEADER entry, 1
entry:
    PUSH 7
failure:
    STORE 1
    HALT
VM_END
expected_stack: .word 7
VM_EXPECT 16, failure, 1, 0, expected_stack, 2, 0, 0, 0, 0
