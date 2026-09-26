.include "vm_fixture.inc"
VM_HEADER entry, 0
entry:
    PUSH -32768
    PUSH -1
failure:
    REM
    HALT
VM_END
expected_stack: .word $8000, $ffff
VM_EXPECT 17, failure, 2, 0, expected_stack, 4, $ff, 0, 0, 0
