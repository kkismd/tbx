.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
entry:
    .repeat 63
    PUSH 0
    .endrepeat
failure:
    TRY_INPUT
    HALT
VM_END
expected_stack: .res 126, 0
VM_EXPECT 13, failure, 63, 0, expected_stack, 126, $ff, 0, 0, 0, 0, 0, 0, VM_CHECK_DATA_DEPTH | VM_CHECK_DATA_STACK
