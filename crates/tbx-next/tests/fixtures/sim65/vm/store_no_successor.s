.include "vm_fixture.inc"
VM_HEADER entry, 1, 0, 0
entry:
    PUSH 42
failure:
    STORE 0
VM_END
expected_stack: .word 42
; Failed STORE must retain the operand and leave its valid target unchanged.
VM_EXPECT 11, failure, 1, 0, expected_stack, 2, 0, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK | VM_CHECK_GLOBAL
