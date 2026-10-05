.include "vm_fixture.inc"
VM_HEADER entry, 1, 0, 0
entry:
    PUSH 77
    STORE 0
    PUSH 5
    CONTROL_PUSH
    CALL body
    HALT
body:
input_failure:
    TRY_INPUT
    RET
VM_END
VM_EXPECT 26, input_failure, 0, 0, 0, 0, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH
