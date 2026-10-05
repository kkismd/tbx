.include "vm_fixture.inc"
VM_HEADER entry, 0, 0, 0
callee:
    RET
entry:
    CALL callee
VM_END
expected_frame: .res 20, $a5
VM_EXPECT 11, entry, 0, 0, 0, 0, $ff, 0, expected_frame, 20, 0, 0, 0, VM_CHECK_CALL_DEPTH | VM_CHECK_FRAME
