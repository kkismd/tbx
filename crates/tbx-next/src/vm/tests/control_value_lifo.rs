use super::*;

#[test]
fn control_value_instructions_move_copy_and_drop_values() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(42)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::CopyControlValue);
    code.append(Instruction::DropControlValue);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert_eq!(vm.data_stack.as_slice(), &[value(42)]);
    assert_eq!(vm.control_value_stack_depth(), 0);
    assert_eq!(vm.return_stack_depth(), 0);
}

#[test]
fn control_value_instructions_preserve_lifo_order_for_nested_values() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::Push(value(2)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::CopyControlValue);
    code.append(Instruction::DropControlValue);
    code.append(Instruction::CopyControlValue);
    code.append(Instruction::DropControlValue);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert_eq!(vm.data_stack.as_slice(), &[value(2), value(1)]);
    assert_eq!(vm.control_value_stack_depth(), 0);
}

#[test]
fn push_control_value_underflow_preserves_all_vm_state() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::PushControlValue);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(code.view()),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow,
            },
        })
    );

    assert_vm_state(&vm, before);
    assert_eq!(vm.control_value_stack_depth(), 0);
}

#[test]
fn copy_control_value_underflow_preserves_data_stack_and_ip() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::CopyControlValue);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.push_data(value(7));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(code.view()),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::ControlValueStackUnderflow {
                source: StackError::ControlValueStackUnderflow,
            },
        })
    );

    assert_vm_state(&vm, before);
    assert_eq!(vm.control_value_stack_depth(), 0);
}

#[test]
fn drop_control_value_underflow_preserves_data_stack_and_ip() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::DropControlValue);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.push_data(value(7));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(code.view()),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::ControlValueStackUnderflow {
                source: StackError::ControlValueStackUnderflow,
            },
        })
    );

    assert_vm_state(&vm, before);
    assert_eq!(vm.control_value_stack_depth(), 0);
}
