use super::*;

fn decrement_top(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    let value = context.pop()?;
    context.push(Value::integer(value.as_integer() - 1));
    Ok(())
}

fn duplicate_top(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    let value = context.peek()?;
    context.push(value);
    Ok(())
}

fn drop_top(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    context.pop()?;
    Ok(())
}

#[test]
fn multiple_pushes_preserve_order() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::Push(value(2)));
    code.append(Instruction::Push(value(3)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_eq!(vm.pop_data(), Ok(value(3)));
    assert_eq!(vm.pop_data(), Ok(value(2)));
    assert_eq!(vm.pop_data(), Ok(value(1)));
}

#[test]
fn return_moves_to_valid_frame_target_and_preserves_data_and_halted_state() {
    let mut code = InstructionSequence::new();
    let target = code.append(Instruction::Halt);
    let entry = code.append(Instruction::Return);
    let mut vm = new_vm(&code, entry);
    vm.data_stack.push(value(11));
    vm.push_return_frame(return_frame(&code, target));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&code, target), false);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(11)));
}

#[test]
fn return_pops_only_top_frame_and_uses_lifo_order() {
    let mut code = InstructionSequence::new();
    let first_target = code.append(Instruction::Halt);
    let second_target = code.append(Instruction::Halt);
    let entry = code.append(Instruction::Return);
    let mut vm = new_vm(&code, entry);

    vm.push_return_frame(return_frame(&code, first_target));
    vm.push_return_frame(return_frame(&code, second_target));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_eq!(vm.instruction_pointer(), location(&code, second_target));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 1);

    vm.instruction_pointer = location(&code, entry);
    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&code, first_target), false);
}

#[test]
fn compiled_call_pushes_return_frame_and_return_resumes_after_call() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let compiled_entry = code.append(Instruction::Push(value(7)));
    code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&code, compiled_entry), code.view())
            .expect("compiled entry should be valid"),
    );
    let call = code.append(Instruction::Call(word));
    let after_call = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, call);
    let mut execution = execution(&code, &words, &primitives);

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.instruction_pointer(), location(&code, compiled_entry));
    assert_eq!(vm.return_stack_depth(), 1);

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));

    assert_eq!(vm.instruction_pointer(), location(&code, after_call));
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.peek_data(), Ok(value(7)));
}

#[test]
fn nested_compiled_calls_start_zeroed_scratch_and_restore_caller_values() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let outer_entry = code.append(Instruction::Call(inner));
    let outer_return = code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );
    let call = code.append(Instruction::Call(outer));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, call);
    let mut execution = execution(&code, &words, &primitives);

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.scratch(ScratchSlot::I), Ok(0));
    vm.set_scratch(ScratchSlot::I, 1234).unwrap();
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.scratch(ScratchSlot::I), Ok(0));
    vm.set_scratch(ScratchSlot::I, -23).unwrap();
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));

    assert_eq!(vm.instruction_pointer(), location(&code, outer_return));
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.scratch(ScratchSlot::I), Ok(1234));
}

#[test]
fn compiled_call_records_depth_before_entering_callee() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let compiled_entry = code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&code, compiled_entry), code.view())
            .expect("compiled entry should be valid"),
    );
    let call = code.append(Instruction::Call(word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, call);
    let mut execution = execution(&code, &words, &primitives);

    for value_count in [0, 1, 3] {
        for n in 0..value_count {
            vm.data_stack.push(value(n as i16));
        }
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
        assert_eq!(vm.call_data_stack_depth(), Ok(value_count));
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
        assert_eq!(
            vm.call_data_stack_depth(),
            Err(StackError::ReturnStackUnderflow)
        );
        vm.instruction_pointer = location(&code, call);
        while vm.data_stack.pop().is_ok() {}
    }
}

#[test]
fn compiled_return_discards_only_control_values_added_by_the_callee() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let callee_entry = code.append(Instruction::Push(value(30)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::Push(value(99)));
    code.append(Instruction::Return);
    let callee = words.add(
        CompletedWordDefinition::compiled(location(&code, callee_entry), code.view())
            .expect("callee entry should be valid"),
    );
    let caller_entry = code.append(Instruction::Push(value(10)));
    code.append(Instruction::PushControlValue);
    let call = code.append(Instruction::Call(callee));
    let after_call = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, caller_entry);
    let mut execution = execution(&code, &words, &primitives);

    for _ in 0..3 {
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    }
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.call_control_value_stack_depth(), Ok(1));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.instruction_pointer(), location(&code, after_call));
    assert_eq!(vm.control_value_stack_depth(), 1);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(99)));
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(
        vm.instruction_pointer().code_space(),
        location(&code, call).code_space()
    );
}

#[test]
fn nested_compiled_returns_restore_each_invocations_control_depth() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();

    let inner_entry = code.append(Instruction::Push(value(3)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );

    let outer_entry = code.append(Instruction::Push(value(2)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::Call(inner));
    let outer_return = code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );

    let caller_entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::Call(outer));
    let after_outer = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, caller_entry);
    let mut execution = execution(&code, &words, &primitives);

    for _ in 0..6 {
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    }
    assert_eq!(vm.control_value_stack_depth(), 2);
    assert_eq!(vm.call_control_value_stack_depth(), Ok(2));
    for _ in 0..3 {
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    }
    assert_eq!(vm.instruction_pointer(), location(&code, outer_return));
    assert_eq!(vm.control_value_stack_depth(), 2);
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.call_control_value_stack_depth(), Ok(1));

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.instruction_pointer(), location(&code, after_outer));
    assert_eq!(vm.control_value_stack_depth(), 1);
    assert_eq!(vm.return_stack_depth(), 0);
}

#[test]
fn recursive_compiled_returns_restore_each_invocations_control_depth() {
    let mut primitives = PrimitiveRegistry::new();
    let decrement = primitives.register(decrement_top);
    let duplicate = primitives.register(duplicate_top);
    let drop = primitives.register(drop_top);
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let recursive_id = WordId::test_invalid(0);
    let decrement_id = WordId::test_invalid(1);
    let duplicate_id = WordId::test_invalid(2);
    let drop_id = WordId::test_invalid(3);

    let recursive_entry = code.append(Instruction::Push(value(55)));
    code.append(Instruction::PushControlValue);
    code.append(Instruction::Call(decrement_id));
    code.append(Instruction::Call(duplicate_id));
    let base_return = code.append(Instruction::JumpIfZero(address(0)));
    code.append(Instruction::Call(recursive_id));
    code.append(Instruction::Return);
    let base_return_target = code.append(Instruction::Call(drop_id));
    code.append(Instruction::Return);
    code.patch_branch_target(base_return, base_return_target)
        .expect("recursive base target should be patchable");
    let recursive = words.add(
        CompletedWordDefinition::compiled(location(&code, recursive_entry), code.view())
            .expect("recursive entry should be valid"),
    );
    assert_eq!(recursive, recursive_id);
    assert_eq!(
        words.add(CompletedWordDefinition::primitive(decrement)),
        decrement_id
    );
    assert_eq!(
        words.add(CompletedWordDefinition::primitive(duplicate)),
        duplicate_id
    );
    assert_eq!(words.add(CompletedWordDefinition::primitive(drop)), drop_id);

    let caller_entry = code.append(Instruction::Push(value(3)));
    code.append(Instruction::Call(recursive));
    let after_call = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, caller_entry);
    let mut execution = execution(&code, &words, &primitives);

    assert_eq!(vm.run(&mut execution), Ok(RunOutcome::Halted));
    assert_eq!(vm.instruction_pointer(), location(&code, after_call));
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.control_value_stack_depth(), 0);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn copy_from_call_base_copies_one_based_offsets_without_consuming_sources() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let compiled_entry = code.append(Instruction::CopyFromCallBase { offset: 1 });
    code.append(Instruction::CopyFromCallBase { offset: 2 });
    code.append(Instruction::CopyFromCallBase { offset: 3 });
    code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&code, compiled_entry), code.view())
            .expect("compiled entry should be valid"),
    );
    let entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::Push(value(2)));
    code.append(Instruction::Push(value(3)));
    code.append(Instruction::Call(word));
    let after_call = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );
    assert_eq!(vm.instruction_pointer(), location(&code, after_call));
    assert_eq!(
        vm.data_stack.as_slice(),
        &[value(1), value(2), value(3), value(3), value(2), value(1)]
    );
}

#[test]
fn copy_from_call_base_reads_the_current_occupant_after_stack_reuse() {
    let mut code = InstructionSequence::new();
    let copy = code.append(Instruction::CopyFromCallBase { offset: 1 });
    let target = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, copy);
    vm.push_return_frame(ReturnFrame::new(location(&code, target), 1));
    vm.push_data(value(7));
    assert_eq!(vm.pop_data(), Ok(value(7)));
    vm.push_data(value(99));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.instruction_pointer(), location(&code, target));
    assert_eq!(vm.data_stack.as_slice(), &[value(99), value(99)]);
}

#[test]
fn copy_from_call_base_keeps_reference_index_when_values_above_it_change() {
    let mut code = InstructionSequence::new();
    let copy = code.append(Instruction::CopyFromCallBase { offset: 1 });
    let target = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, copy);
    vm.push_return_frame(ReturnFrame::new(location(&code, target), 2));
    vm.push_data(value(10));
    vm.push_data(value(20));
    vm.push_data(value(30));
    assert_eq!(vm.pop_data(), Ok(value(30)));
    vm.push_data(value(40));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(
        vm.data_stack.as_slice(),
        &[value(10), value(20), value(40), value(20)]
    );
}

#[test]
fn copy_from_call_base_rejects_invalid_inputs_without_mutation() {
    let cases = [
        (
            0,
            3,
            vec![value(1), value(2), value(3)],
            CallBaseValueError::InvalidOffset {
                offset: 0,
                call_data_stack_depth: 3,
            },
        ),
        (
            4,
            3,
            vec![value(1), value(2), value(3)],
            CallBaseValueError::InvalidOffset {
                offset: 4,
                call_data_stack_depth: 3,
            },
        ),
        (
            1,
            3,
            vec![value(1)],
            CallBaseValueError::DataStackIndexOutOfBounds { index: 2, depth: 1 },
        ),
    ];

    for (offset, call_depth, values, source) in cases {
        let mut code = InstructionSequence::new();
        let copy = code.append(Instruction::CopyFromCallBase { offset });
        let target = code.append(Instruction::Halt);
        let mut vm = new_vm(&code, copy);
        for value in values {
            vm.push_data(value);
        }
        vm.push_return_frame(ReturnFrame::new(location(&code, target), call_depth));
        let before = snapshot(&vm);

        assert_eq!(
            vm.step(code.view()),
            Err(VmError {
                location: location(&code, copy),
                kind: VmErrorKind::CallBaseValueCopy { source },
            })
        );
        assert_vm_state(&vm, before);
    }
}

#[test]
fn copy_from_call_base_rejects_top_level_execution_without_mutation() {
    let mut code = InstructionSequence::new();
    let copy = code.append(Instruction::CopyFromCallBase { offset: 1 });
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, copy);
    vm.push_data(value(7));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(code.view()),
        Err(VmError {
            location: location(&code, copy),
            kind: VmErrorKind::CallBaseValueCopy {
                source: CallBaseValueError::NoCompiledWordInvocation,
            },
        })
    );
    assert_vm_state(&vm, before);
}

#[test]
fn copy_from_call_base_uses_the_innermost_nested_frame() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::CopyFromCallBase { offset: 1 });
    code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let outer_entry = code.append(Instruction::CopyFromCallBase { offset: 1 });
    code.append(Instruction::Call(inner));
    code.append(Instruction::CopyFromCallBase { offset: 2 });
    code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );
    let entry = code.append(Instruction::Push(value(10)));
    code.append(Instruction::Push(value(20)));
    code.append(Instruction::Call(outer));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );
    assert_eq!(
        vm.data_stack.as_slice(),
        &[value(10), value(20), value(20), value(20), value(10)]
    );
}

#[test]
fn truncate_data_stack_to_call_base_discards_only_callee_values() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let compiled_entry = code.append(Instruction::Push(value(30)));
    code.append(Instruction::Push(value(40)));
    code.append(Instruction::TruncateDataStackToCallBase);
    code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&code, compiled_entry), code.view())
            .expect("compiled entry should be valid"),
    );
    let entry = code.append(Instruction::Push(value(10)));
    code.append(Instruction::Push(value(20)));
    code.append(Instruction::Call(word));
    let after_call = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );
    assert_eq!(vm.instruction_pointer(), location(&code, after_call));
    assert_eq!(vm.data_stack.as_slice(), &[value(10), value(20)]);
}

#[test]
fn truncate_data_stack_to_call_base_is_a_no_op_at_the_call_depth() {
    let mut code = InstructionSequence::new();
    let truncate = code.append(Instruction::TruncateDataStackToCallBase);
    let target = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, truncate);
    vm.push_data(value(7));
    vm.push_return_frame(ReturnFrame::new(location(&code, target), 1));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.instruction_pointer(), location(&code, target));
    assert_eq!(vm.data_stack.as_slice(), &[value(7)]);
}

#[test]
fn truncate_data_stack_to_call_base_rejects_a_shallow_stack_atomically() {
    let mut code = InstructionSequence::new();
    let truncate = code.append(Instruction::TruncateDataStackToCallBase);
    let target = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, truncate);
    vm.push_data(value(7));
    vm.push_return_frame(ReturnFrame::new(location(&code, target), 2));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(code.view()),
        Err(VmError {
            location: location(&code, truncate),
            kind: VmErrorKind::CallBaseDataStackTruncate {
                source: CallBaseDataStackTruncateError::CurrentDepthBelowCallBase {
                    current_depth: 1,
                    call_data_stack_depth: 2,
                },
            },
        })
    );
    assert_vm_state(&vm, before);
}

#[test]
fn truncate_data_stack_to_call_base_does_not_restore_popped_values() {
    let mut code = InstructionSequence::new();
    let truncate = code.append(Instruction::TruncateDataStackToCallBase);
    let target = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, truncate);
    vm.push_data(value(10));
    vm.push_data(value(20));
    vm.push_return_frame(ReturnFrame::new(location(&code, target), 2));
    assert_eq!(vm.pop_data(), Ok(value(20)));
    vm.push_data(value(30));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.data_stack.as_slice(), &[value(10), value(30)]);
}

#[test]
fn truncate_data_stack_to_call_base_rejects_top_level_execution() {
    let mut code = InstructionSequence::new();
    let truncate = code.append(Instruction::TruncateDataStackToCallBase);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, truncate);
    vm.push_data(value(7));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(code.view()),
        Err(VmError {
            location: location(&code, truncate),
            kind: VmErrorKind::CallBaseDataStackTruncate {
                source: CallBaseDataStackTruncateError::NoCompiledWordInvocation,
            },
        })
    );
    assert_vm_state(&vm, before);
}

#[test]
fn truncate_data_stack_to_call_base_uses_innermost_nested_frame() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::Push(value(40)));
    code.append(Instruction::TruncateDataStackToCallBase);
    code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let outer_entry = code.append(Instruction::Push(value(30)));
    code.append(Instruction::Call(inner));
    code.append(Instruction::TruncateDataStackToCallBase);
    code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );
    let entry = code.append(Instruction::Push(value(10)));
    code.append(Instruction::Call(outer));
    let after_call = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );
    assert_eq!(vm.instruction_pointer(), location(&code, after_call));
    assert_eq!(vm.data_stack.as_slice(), &[value(10)]);
}

#[test]
fn nested_compiled_calls_keep_independent_call_depths_and_return_to_outer_frame() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let outer_entry = code.append(Instruction::Push(value(7)));
    code.append(Instruction::Call(inner));
    code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );
    let outer_call = code.append(Instruction::Call(outer));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, outer_call);
    let mut execution = execution(&code, &words, &primitives);

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.call_data_stack_depth(), Ok(0));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.instruction_pointer(), location(&code, inner_entry));
    assert_eq!(vm.call_data_stack_depth(), Ok(1));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.call_data_stack_depth(), Ok(0));

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(
        vm.call_data_stack_depth(),
        Err(StackError::ReturnStackUnderflow)
    );
}

#[test]
fn top_level_execution_has_no_call_data_stack_depth() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Halt);
    let vm = new_vm(&code, entry);

    assert_eq!(
        vm.call_data_stack_depth(),
        Err(StackError::ReturnStackUnderflow)
    );
}

#[test]
fn compiled_call_enters_published_code_space_and_returns_to_caller_space() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut callee_code = InstructionSequence::new();
    let callee_entry = callee_code.append(Instruction::Push(value(17)));
    callee_code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&callee_code, callee_entry), callee_code.view())
            .expect("callee entry should be valid"),
    );
    let mut caller_code = InstructionSequence::new();
    let call = caller_code.append(Instruction::Call(word));
    let after_call = caller_code.append(Instruction::Halt);
    let code_spaces = [caller_code.view(), callee_code.view()];
    let mut execution = multi_execution(&code_spaces, &words, &primitives);
    let mut vm = Vm::new_at_location_in(&mut execution, location(&caller_code, call))
        .expect("caller entry should be valid");

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(
        vm.instruction_pointer(),
        location(&callee_code, callee_entry)
    );
    assert_eq!(
        vm.return_stack.as_slice(),
        &[return_frame(&caller_code, after_call)]
    );

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&caller_code, after_call), false);
    assert_eq!(vm.peek_data(), Ok(value(17)));
}

#[test]
fn three_level_cross_space_compiled_calls_return_through_each_caller_space() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();

    let mut level3_code = InstructionSequence::new();
    let level3_entry = level3_code.append(Instruction::Push(value(3)));
    level3_code.append(Instruction::Return);
    let level3 = words.add(
        CompletedWordDefinition::compiled(location(&level3_code, level3_entry), level3_code.view())
            .expect("level3 entry should be valid"),
    );

    let mut level2_code = InstructionSequence::new();
    let level2_entry = level2_code.append(Instruction::Push(value(2)));
    level2_code.append(Instruction::Call(level3));
    level2_code.append(Instruction::Return);
    let level2 = words.add(
        CompletedWordDefinition::compiled(location(&level2_code, level2_entry), level2_code.view())
            .expect("level2 entry should be valid"),
    );

    let mut level1_code = InstructionSequence::new();
    let level1_entry = level1_code.append(Instruction::Push(value(1)));
    level1_code.append(Instruction::Call(level2));
    level1_code.append(Instruction::Return);
    let level1 = words.add(
        CompletedWordDefinition::compiled(location(&level1_code, level1_entry), level1_code.view())
            .expect("level1 entry should be valid"),
    );

    let mut caller_code = InstructionSequence::new();
    let entry = caller_code.append(Instruction::Call(level1));
    caller_code.append(Instruction::Halt);
    let code_spaces = [
        caller_code.view(),
        level1_code.view(),
        level2_code.view(),
        level3_code.view(),
    ];
    let mut execution = multi_execution(&code_spaces, &words, &primitives);
    let mut vm = Vm::new_at_location_in(&mut execution, location(&caller_code, entry))
        .expect("caller entry should be valid");

    assert_eq!(vm.run(&mut execution), Ok(RunOutcome::Halted));

    assert!(vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.pop_data(), Ok(value(3)));
    assert_eq!(vm.pop_data(), Ok(value(2)));
    assert_eq!(vm.pop_data(), Ok(value(1)));
}

#[test]
fn cross_space_call_does_not_fallback_to_same_local_index_in_caller_space() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();

    let mut callee_code = InstructionSequence::new();
    let callee_entry = callee_code.append(Instruction::Push(value(21)));
    callee_code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&callee_code, callee_entry), callee_code.view())
            .expect("callee entry should be valid"),
    );

    let mut caller_code = InstructionSequence::new();
    let call = caller_code.append(Instruction::Call(word));
    caller_code.append(Instruction::Halt);
    assert_eq!(call.as_index(), callee_entry.as_index());

    let code_spaces = [caller_code.view(), callee_code.view()];
    let mut execution = multi_execution(&code_spaces, &words, &primitives);
    let mut vm = Vm::new_at_location_in(&mut execution, location(&caller_code, call))
        .expect("caller entry should be valid");

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));

    assert_eq!(
        vm.instruction_pointer(),
        location(&callee_code, callee_entry)
    );
    assert_ne!(vm.instruction_pointer(), location(&caller_code, call));
}

#[test]
fn local_branch_inside_cross_space_callee_stays_in_callee_space() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();

    let mut callee_code = InstructionSequence::new();
    let callee_entry = callee_code.append(Instruction::Jump(address(2)));
    callee_code.append(Instruction::Push(value(99)));
    callee_code.append(Instruction::Push(value(5)));
    callee_code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&callee_code, callee_entry), callee_code.view())
            .expect("callee entry should be valid"),
    );

    let mut caller_code = InstructionSequence::new();
    let entry = caller_code.append(Instruction::Call(word));
    caller_code.append(Instruction::Halt);
    let code_spaces = [caller_code.view(), callee_code.view()];
    let mut execution = multi_execution(&code_spaces, &words, &primitives);
    let mut vm = Vm::new_at_location_in(&mut execution, location(&caller_code, entry))
        .expect("caller entry should be valid");

    assert_eq!(vm.run(&mut execution), Ok(RunOutcome::Halted));

    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(5)));
}

#[test]
fn nested_compiled_calls_return_in_lifo_order() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::Push(value(2)));
    code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let outer_entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::Call(inner));
    code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );
    let entry = code.append(Instruction::Call(outer));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );

    assert!(vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.pop_data(), Ok(value(2)));
    assert_eq!(vm.pop_data(), Ok(value(1)));
}

#[test]
fn run_mixes_primitive_and_compiled_word_dispatch() {
    let mut primitives = PrimitiveRegistry::new();
    let add = primitives.register(add_top_two);
    let mut words = PublishedWords::new();
    let add_word = words.add(CompletedWordDefinition::primitive(add));
    let mut code = InstructionSequence::new();
    let compiled_entry = code.append(Instruction::Push(value(30)));
    code.append(Instruction::Push(value(12)));
    code.append(Instruction::Call(add_word));
    code.append(Instruction::Return);
    let compiled_word = words.add(
        CompletedWordDefinition::compiled(location(&code, compiled_entry), code.view())
            .expect("compiled entry should be valid"),
    );
    let entry = code.append(Instruction::Call(compiled_word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );

    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(42)));
}

#[test]
fn old_word_id_call_keeps_old_definition_after_redefinition() {
    let mut primitives = PrimitiveRegistry::new();
    let old_primitive = primitives.register(push_42);
    let new_primitive = primitives.register(|context| {
        context.push(value(99));
        Ok(())
    });
    let mut words = PublishedWords::new();
    let old_word = words.add(CompletedWordDefinition::primitive(old_primitive));
    let new_word = words.add(CompletedWordDefinition::primitive(new_primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(old_word));
    code.append(Instruction::Call(new_word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution(&code, &words, &primitives)),
        Ok(RunOutcome::Halted)
    );

    assert_eq!(vm.pop_data(), Ok(value(99)));
    assert_eq!(vm.pop_data(), Ok(value(42)));
}
