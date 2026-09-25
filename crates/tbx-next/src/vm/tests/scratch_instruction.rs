use super::*;
use crate::stack::ScratchSlot;

fn slots() -> [ScratchSlot; 8] {
    [
        ScratchSlot::I,
        ScratchSlot::J,
        ScratchSlot::K,
        ScratchSlot::L,
        ScratchSlot::M,
        ScratchSlot::N,
        ScratchSlot::X,
        ScratchSlot::Y,
    ]
}

#[test]
fn every_scratch_slot_stores_and_loads_signed_values() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();

    for (index, slot) in slots().into_iter().enumerate() {
        let mut code = InstructionSequence::new();
        let store = code.append(Instruction::StoreScratch(ScratchSlotOperand::from_slot(
            slot,
        )));
        code.append(Instruction::LoadScratch(ScratchSlotOperand::from_slot(
            slot,
        )));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, store);
        vm.push_data(value(-120 + index as i16));
        vm.push_return_frame(return_frame(&code, address(2)));

        assert_eq!(
            vm.run(execution(&code, &words, &primitives)),
            Ok(RunOutcome::Halted)
        );
        assert_eq!(vm.peek_data(), Ok(value(-120 + index as i16)));
        assert_eq!(vm.scratch(slot), Ok(-120 + index as i16));
    }
}

#[test]
fn scratch_load_starts_at_zero_and_requires_an_invocation() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::I,
    )));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.push_return_frame(return_frame(&code, address(1)));

    assert_eq!(
        vm.step(execution(&code, &words, &primitives)),
        Ok(StepOutcome::Continued)
    );
    assert_eq!(vm.peek_data(), Ok(value(0)));
}

#[test]
fn nested_invocation_scratch_is_independent_for_the_same_slot() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut code = InstructionSequence::new();
    let store = code.append(Instruction::StoreScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::I,
    )));
    code.append(Instruction::Return);
    let load_outer = code.append(Instruction::LoadScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::I,
    )));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, store);
    let mut outer = return_frame(&code, load_outer);
    outer.set_scratch(ScratchSlot::I, 11);
    vm.push_return_frame(outer);
    vm.push_return_frame(return_frame(&code, load_outer));
    vm.push_data(value(22));

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );
    assert_eq!(vm.peek_data(), Ok(value(11)));
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.scratch(ScratchSlot::I), Ok(11));
}

#[test]
fn invalid_slot_and_missing_frame_fail_without_mutation() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::StoreScratch(ScratchSlotOperand::from_raw(8)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.push_data(value(42));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidScratchSlot { slot: 8 },
        })
    );
    assert_vm_state(&vm, before);

    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::StoreScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::I,
    )));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.push_data(value(42));
    let before = snapshot(&vm);
    assert_eq!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::NoScratchInvocation
        })
    );
    assert_vm_state(&vm, before);

    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadScratch(ScratchSlotOperand::from_raw(255)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    let before = snapshot(&vm);
    assert_eq!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidScratchSlot { slot: 255 },
        })
    );
    assert_vm_state(&vm, before);

    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::I,
    )));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    let before = snapshot(&vm);
    assert_eq!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::NoScratchInvocation,
        })
    );
    assert_vm_state(&vm, before);
}

#[test]
fn store_underflow_and_missing_next_location_are_atomic() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::StoreScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::I,
    )));
    let mut vm = new_vm(&code, entry);
    vm.push_return_frame(return_frame(&code, entry));
    let before = snapshot(&vm);
    assert!(matches!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            kind: VmErrorKind::DataStackUnderflow { .. },
            ..
        })
    ));
    assert_vm_state(&vm, before);

    vm.push_data(value(9));
    let before = snapshot(&vm);
    assert!(matches!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            kind: VmErrorKind::UnexpectedEndOfCode { .. },
            ..
        })
    ));
    assert_vm_state(&vm, before);
}

#[test]
fn load_missing_next_location_preserves_stack_pointer_and_scratch() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadScratch(ScratchSlotOperand::from_slot(
        ScratchSlot::X,
    )));
    let mut vm = new_vm(&code, entry);
    let mut frame = return_frame(&code, entry);
    frame.set_scratch(ScratchSlot::X, 77);
    vm.push_return_frame(frame);
    let before = snapshot(&vm);

    assert!(matches!(
        vm.step(execution(&code, &words, &primitives)),
        Err(VmError {
            kind: VmErrorKind::UnexpectedEndOfCode { .. },
            ..
        })
    ));
    assert_vm_state(&vm, before);
    assert_eq!(vm.scratch(ScratchSlot::X), Ok(77));
}
