use super::*;

#[test]
fn primitive_call_runs_without_return_frame_and_advances_to_next_instruction() {
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(push_42);
    let mut words = PublishedWords::new();
    let word = words.add(CompletedWordDefinition::primitive(primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(word));
    let next = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.step(execution(&code, &words, &primitives)),
        Ok(StepOutcome::Continued)
    );

    assert_eq!(vm.instruction_pointer(), location(&code, next));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.peek_data(), Ok(value(42)));
}

#[test]
fn primitive_failure_preserves_control_state_and_atomic_handler_stack() {
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(fail_without_stack_update);
    let mut words = PublishedWords::new();
    let word = words.add(CompletedWordDefinition::primitive(primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(7)));
    let call = code.append(Instruction::Call(word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    let result = vm.step(execution(&code, &words, &primitives));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, call),
            kind: VmErrorKind::PrimitiveFailed {
                primitive,
                source: PrimitiveError::Failed
            }
        })
    );
    assert_eq!(vm.instruction_pointer(), location(&code, call));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(7)));
}

#[test]
fn primitive_output_writes_completed_chunks_in_order() {
    let mut primitives = PrimitiveRegistry::new();
    let alpha = primitives.register(write_alpha);
    let beta = primitives.register(write_beta);
    let empty = primitives.register(write_empty);
    let mut words = PublishedWords::new();
    let alpha_word = words.add(CompletedWordDefinition::primitive(alpha));
    let beta_word = words.add(CompletedWordDefinition::primitive(beta));
    let empty_word = words.add(CompletedWordDefinition::primitive(empty));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(alpha_word));
    code.append(Instruction::Call(beta_word));
    code.append(Instruction::Call(empty_word));
    code.append(Instruction::Halt);
    let mut output = TestOutput::new();
    let mut vm = new_vm(&code, entry);

    let result = vm.run(execution(&code, &words, &primitives).with_output(&mut output));

    assert_eq!(result, Ok(RunOutcome::Halted));
    assert_eq!(output.chunks(), ["alpha", "beta", ""]);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn primitive_output_failure_preserves_vm_state_without_stack_rollback() {
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(write_then_pop_output);
    let mut words = PublishedWords::new();
    let word = words.add(CompletedWordDefinition::primitive(primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(7)));
    let call = code.append(Instruction::Call(word));
    code.append(Instruction::Halt);
    let mut output = TestOutput::new();
    output.fail_next_write(RuntimeOutputError::Failed);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    let before = snapshot(&vm);
    let result = vm.step(execution(&code, &words, &primitives).with_output(&mut output));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, call),
            kind: VmErrorKind::PrimitiveFailed {
                primitive,
                source: PrimitiveError::OutputFailed {
                    source: RuntimeOutputError::Failed,
                }
            }
        })
    );
    assert_vm_state(&vm, before);
    assert!(output.chunks().is_empty());
}

#[test]
fn missing_output_capability_is_a_primitive_failure_without_vm_mutation() {
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(write_then_pop_output);
    let mut words = PublishedWords::new();
    let word = words.add(CompletedWordDefinition::primitive(primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(11)));
    let call = code.append(Instruction::Call(word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    let before = snapshot(&vm);
    let result = vm.step(execution(&code, &words, &primitives));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, call),
            kind: VmErrorKind::PrimitiveFailed {
                primitive,
                source: PrimitiveError::OutputFailed {
                    source: RuntimeOutputError::Unavailable,
                }
            }
        })
    );
    assert_vm_state(&vm, before);
}

#[test]
fn run_keeps_successful_primitive_state_before_failed_primitive_call() {
    let mut primitives = PrimitiveRegistry::new();
    let push = primitives.register(push_42);
    let fail = primitives.register(fail_without_stack_update);
    let mut words = PublishedWords::new();
    let push_word = words.add(CompletedWordDefinition::primitive(push));
    let fail_word = words.add(CompletedWordDefinition::primitive(fail));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(push_word));
    let failing_call = code.append(Instruction::Call(fail_word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    let result = vm.run(execution(&code, &words, &primitives));

    let error = VmError {
        location: location(&code, failing_call),
        kind: VmErrorKind::PrimitiveFailed {
            primitive: fail,
            source: PrimitiveError::Failed,
        },
    };
    assert_eq!(result, Err(error));
    assert_eq!(error.address(), failing_call);
    assert_vm_state(
        &vm,
        expected_state(
            location(&code, failing_call),
            vec![value(42)],
            Vec::new(),
            false,
        ),
    );
}

#[test]
fn primitive_call_rejects_unregistered_primitive_without_mutation() {
    let primitives = PrimitiveRegistry::new();
    let primitive = PrimitiveId::from_slot(0);
    let mut words = PublishedWords::new();
    let word = words.add(CompletedWordDefinition::primitive(primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(word));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    let result = vm.step(execution(&code, &words, &primitives));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidPrimitiveId {
                source: PrimitiveLookupError::InvalidPrimitiveId { id: primitive }
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}
