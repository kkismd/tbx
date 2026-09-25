use super::*;

#[test]
fn array_load_failures_preserve_vm_and_storage() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let valid = arrays.allocate(2);
    arrays.view_mut().write(valid, 0, value(17)).unwrap();

    for (id, index, source) in [
        (
            valid,
            0,
            GlobalArrayError::SurfaceIndexOutOfBounds {
                id: valid,
                index: 0,
                len: 2,
            },
        ),
        (
            valid,
            -1,
            GlobalArrayError::SurfaceIndexOutOfBounds {
                id: valid,
                index: -1,
                len: 2,
            },
        ),
        (
            valid,
            3,
            GlobalArrayError::SurfaceIndexOutOfBounds {
                id: valid,
                index: 3,
                len: 2,
            },
        ),
        (
            ArrayId::test_invalid(8),
            1,
            GlobalArrayError::InvalidArrayId {
                id: ArrayId::test_invalid(8),
            },
        ),
    ] {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::LoadArrayElement(id));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);
        vm.data_stack.push(value(index));
        let before = snapshot(&vm);

        assert_eq!(
            vm.step(execution_with_arrays(
                &code,
                &words,
                &primitives,
                &mut arrays
            )),
            Err(VmError {
                location: location(&code, entry),
                kind: VmErrorKind::InvalidGlobalArray { source }
            })
        );
        assert_vm_state(&vm, before);
        assert_eq!(arrays.view().read(valid, 0), Ok(value(17)));
    }
}

#[test]
fn array_store_failures_preserve_vm_and_storage() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let id = arrays.allocate(1);
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::StoreArrayElement(id));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.data_stack.push(value(0));
    vm.data_stack.push(value(99));
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(execution_with_arrays(
            &code,
            &words,
            &primitives,
            &mut arrays
        )),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidGlobalArray {
                source: GlobalArrayError::SurfaceIndexOutOfBounds {
                    id,
                    index: 0,
                    len: 1
                }
            }
        })
    );
    assert_vm_state(&vm, before);
    assert_eq!(arrays.view().read(id, 0), Ok(value(0)));

    let invalid = ArrayId::test_invalid(7);
    let mut invalid_code = InstructionSequence::new();
    let invalid_entry = invalid_code.append(Instruction::StoreArrayElement(invalid));
    invalid_code.append(Instruction::Halt);
    let mut invalid_vm = new_vm(&invalid_code, invalid_entry);
    invalid_vm.data_stack.push(value(1));
    invalid_vm.data_stack.push(value(99));
    let invalid_before = snapshot(&invalid_vm);
    assert_eq!(
        invalid_vm.step(execution_with_arrays(
            &invalid_code,
            &words,
            &primitives,
            &mut arrays
        )),
        Err(VmError {
            location: location(&invalid_code, invalid_entry),
            kind: VmErrorKind::InvalidGlobalArray {
                source: GlobalArrayError::InvalidArrayId { id: invalid }
            }
        })
    );
    assert_vm_state(&invalid_vm, invalid_before);
    assert_eq!(arrays.view().read(id, 0), Ok(value(0)));
}

#[test]
fn array_stack_underflow_preserves_vm_and_storage() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let id = arrays.allocate(1);
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::StoreArrayElement(id));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    let before = snapshot(&vm);

    assert_eq!(
        vm.step(execution_with_arrays(
            &code,
            &words,
            &primitives,
            &mut arrays
        )),
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow
            }
        })
    );
    assert_vm_state(&vm, before);
    assert_eq!(arrays.view().read(id, 0), Ok(value(0)));

    let mut one_value_code = InstructionSequence::new();
    let one_value_entry = one_value_code.append(Instruction::StoreArrayElement(id));
    one_value_code.append(Instruction::Halt);
    let mut one_value_vm = new_vm(&one_value_code, one_value_entry);
    one_value_vm.data_stack.push(value(1));
    let one_value_before = snapshot(&one_value_vm);
    assert_eq!(
        one_value_vm.step(execution_with_arrays(
            &one_value_code,
            &words,
            &primitives,
            &mut arrays
        )),
        Err(VmError {
            location: location(&one_value_code, one_value_entry),
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow
            }
        })
    );
    assert_vm_state(&one_value_vm, one_value_before);
    assert_eq!(arrays.view().read(id, 0), Ok(value(0)));

    let mut load_code = InstructionSequence::new();
    let load_entry = load_code.append(Instruction::LoadArrayElement(id));
    load_code.append(Instruction::Halt);
    let mut load_vm = new_vm(&load_code, load_entry);
    let load_before = snapshot(&load_vm);
    assert_eq!(
        load_vm.step(execution_with_arrays(
            &load_code,
            &words,
            &primitives,
            &mut arrays
        )),
        Err(VmError {
            location: location(&load_code, load_entry),
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow
            }
        })
    );
    assert_vm_state(&load_vm, load_before);
    assert_eq!(arrays.view().read(id, 0), Ok(value(0)));
}

#[test]
fn load_var_invalid_id_preserves_vm_and_global_state() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let valid = globals.allocate();
    globals
        .view_mut()
        .write(valid, value(5))
        .expect("allocated global should be valid");
    let invalid = GlobalVarId::test_invalid(99);
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadVar(invalid));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.data_stack.push(value(1));
    let before = snapshot(&vm);

    let result = vm.step(execution_with_globals(
        &code,
        &words,
        &primitives,
        &mut globals,
    ));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidGlobalVarId {
                source: GlobalVariableError::InvalidGlobalVarId { id: invalid }
            }
        })
    );
    assert_vm_state(&vm, before);
    assert_eq!(globals.view().read(valid), Ok(value(5)));
}

#[test]
fn store_var_invalid_id_preserves_vm_and_global_state() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let valid = globals.allocate();
    let invalid = GlobalVarId::test_invalid(99);
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(8)));
    let store = code.append(Instruction::StoreVar(invalid));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    {
        let mut execution = execution_with_globals(&code, &words, &primitives, &mut globals);
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
        let before = snapshot(&vm);

        let result = vm.step(&mut execution);

        assert_eq!(
            result,
            Err(VmError {
                location: location(&code, store),
                kind: VmErrorKind::InvalidGlobalVarId {
                    source: GlobalVariableError::InvalidGlobalVarId { id: invalid }
                }
            })
        );
        assert_vm_state(&vm, before);
    }
    assert_eq!(globals.view().read(valid), Ok(value(0)));
}

#[test]
fn store_var_write_failure_returns_vm_error_without_mutating_vm_state() {
    struct WriteFailingGlobalView<'a> {
        instructions: InstructionLookup<'a>,
    }

    impl<'a> VmExecutionView<'a> for WriteFailingGlobalView<'a> {
        fn instructions(&self) -> InstructionLookup<'a> {
            self.instructions
        }

        fn lookup_word(&self, id: WordId) -> Result<WordDefinition, WordLookupError> {
            Err(WordLookupError::InvalidWordId { id })
        }

        fn lookup_handler(
            &self,
            id: PrimitiveId,
        ) -> Result<crate::primitive::PrimitiveHandler, PrimitiveLookupError> {
            Err(PrimitiveLookupError::InvalidPrimitiveId { id })
        }

        fn read_global(&self, _id: GlobalVarId) -> Result<Value, GlobalVariableError> {
            Ok(value(99))
        }

        fn write_global(
            &mut self,
            id: GlobalVarId,
            _value: Value,
        ) -> Result<(), GlobalVariableError> {
            Err(GlobalVariableError::InvalidGlobalVarId { id })
        }
    }

    let failing = GlobalVarId::test_invalid(7);
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(14)));
    let store = code.append(Instruction::StoreVar(failing));
    code.append(Instruction::Halt);
    let mut execution = WriteFailingGlobalView {
        instructions: code.view().into(),
    };
    let mut vm = Vm::new_at_location_in(&mut execution, location(&code, entry))
        .expect("test entry should be valid");

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    let before = snapshot(&vm);

    let result = vm.step(&mut execution);

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, store),
            kind: VmErrorKind::InvalidGlobalVarId {
                source: GlobalVariableError::InvalidGlobalVarId { id: failing }
            }
        })
    );
    assert_vm_state(&vm, before);
}

#[test]
fn store_var_underflow_preserves_vm_and_global_state() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let id = globals.allocate();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::StoreVar(id));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    let before = snapshot(&vm);

    let result = vm.step(execution_with_globals(
        &code,
        &words,
        &primitives,
        &mut globals,
    ));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow
            }
        })
    );
    assert_vm_state(&vm, before);
    assert_eq!(globals.view().read(id), Ok(value(0)));
}

#[test]
fn load_var_missing_next_location_preserves_stack_and_global_state() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let id = globals.allocate();
    globals
        .view_mut()
        .write(id, value(44))
        .expect("allocated global should be valid");
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadVar(id));
    let mut vm = new_vm(&code, entry);
    let before = snapshot(&vm);

    let result = vm.step(execution_with_globals(
        &code,
        &words,
        &primitives,
        &mut globals,
    ));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::UnexpectedEndOfCode {
                source: address_lookup_error(InstructionAddressError::EndAddress {
                    address: address(1)
                })
            }
        })
    );
    assert_vm_state(&vm, before);
    assert_eq!(globals.view().read(id), Ok(value(44)));
}

#[test]
fn store_var_missing_next_location_preserves_stack_and_global_state() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let id = globals.allocate();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(12)));
    let store = code.append(Instruction::StoreVar(id));
    let mut vm = new_vm(&code, entry);

    {
        let mut execution = execution_with_globals(&code, &words, &primitives, &mut globals);
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
        let before = snapshot(&vm);

        let result = vm.step(&mut execution);

        assert_eq!(
            result,
            Err(VmError {
                location: location(&code, store),
                kind: VmErrorKind::UnexpectedEndOfCode {
                    source: address_lookup_error(InstructionAddressError::EndAddress {
                        address: address(2)
                    })
                }
            })
        );
        assert_vm_state(&vm, before);
    }
    assert_eq!(globals.view().read(id), Ok(value(0)));
}

#[test]
fn return_underflow_reports_error_without_mutation_or_implicit_halt() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Return);
    let mut vm = new_vm(&code, entry);

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::ReturnStackUnderflow {
                source: StackError::ReturnStackUnderflow
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn return_rejects_end_target_without_popping_frame_or_mutation() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Return);
    let end = address(code.len());
    let mut vm = new_vm(&code, entry);
    vm.data_stack.push(value(3));
    vm.push_return_frame(return_frame(&code, end));

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidReturnTarget {
                source: address_lookup_error(InstructionAddressError::EndAddress { address: end })
            }
        })
    );
    assert_eq!(vm.instruction_pointer(), location(&code, entry));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(3)));
}

#[test]
fn return_rejects_out_of_range_target_without_popping_frame_or_mutation() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Return);
    let invalid = address(usize::MAX);
    let mut vm = new_vm(&code, entry);
    vm.control_value_stack.push(value(8));
    let mut frame = return_frame(&code, invalid);
    frame.set_scratch(ScratchSlot::X, 77);
    vm.push_return_frame(frame);

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidReturnTarget {
                source: address_lookup_error(InstructionAddressError::InvalidAddress {
                    address: invalid,
                })
            }
        })
    );
    assert_eq!(vm.instruction_pointer(), location(&code, entry));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.data_stack_depth(), 0);
    assert_eq!(vm.control_value_stack_depth(), 1);
    assert_eq!(vm.scratch(ScratchSlot::X), Ok(77));
}

#[test]
fn return_rejects_control_value_depth_below_call_depth_without_mutation() {
    let mut code = InstructionSequence::new();
    let target = code.append(Instruction::Halt);
    let entry = code.append(Instruction::Return);
    let mut vm = new_vm(&code, entry);
    vm.data_stack.push(value(7));
    let mut frame = ReturnFrame::with_control_value_stack_depth(location(&code, target), 0, 1);
    frame.set_scratch(ScratchSlot::N, -31);
    vm.push_return_frame(frame);

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::ControlValueStackDepthBelowCall {
                call_depth: 1,
                current_depth: 0,
            },
        })
    );
    assert_eq!(vm.instruction_pointer(), location(&code, entry));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(7)));
    assert_eq!(vm.control_value_stack_depth(), 0);
    assert_eq!(vm.scratch(ScratchSlot::N), Ok(-31));
}

#[test]
fn return_rejects_unregistered_target_code_space_without_mutation() {
    let mut full_code = InstructionSequence::new();
    let target = full_code.append(Instruction::Halt);
    let mut shorter_code = InstructionSequence::new();
    let entry = shorter_code.append(Instruction::Return);
    let mut vm = new_vm(&shorter_code, entry);
    vm.push_return_frame(return_frame(&full_code, target));

    let result = vm.step(shorter_code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&shorter_code, entry),
            kind: VmErrorKind::InvalidReturnTarget {
                source: InstructionLookupError::UnknownCodeSpace {
                    code_space: full_code.view().code_space(),
                }
            }
        })
    );
    assert_eq!(vm.instruction_pointer(), location(&shorter_code, entry));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 1);
}

#[test]
fn call_rejects_unpublished_word_id_without_mutation() {
    let mut other_words = PublishedWords::new();
    let unpublished = other_words.add(CompletedWordDefinition::primitive(PrimitiveId::from_slot(
        0,
    )));
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(unpublished));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    let result = vm.step(execution(&code, &words, &primitives));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidWordId {
                source: WordLookupError::InvalidWordId { id: unpublished }
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn call_at_end_rejects_missing_return_address_before_dispatch() {
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(push_42);
    let mut words = PublishedWords::new();
    let word = words.add(CompletedWordDefinition::primitive(primitive));
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Call(word));
    let mut vm = new_vm(&code, entry);

    let result = vm.step(execution(&code, &words, &primitives));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::UnexpectedEndOfCode {
                source: address_lookup_error(InstructionAddressError::EndAddress {
                    address: address(1)
                })
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn cross_space_compiled_call_rejects_invalid_entry_address_atomically() {
    #[derive(Clone, Copy)]
    struct InvalidCompiledEntryView<'a> {
        instructions: InstructionLookup<'a>,
        entry: CodeLocation,
    }

    impl<'a> VmExecutionView<'a> for InvalidCompiledEntryView<'a> {
        fn instructions(&self) -> InstructionLookup<'a> {
            self.instructions
        }

        fn lookup_word(&self, _id: WordId) -> Result<WordDefinition, WordLookupError> {
            Ok(WordDefinition::Compiled { entry: self.entry })
        }

        fn lookup_handler(
            &self,
            id: PrimitiveId,
        ) -> Result<crate::primitive::PrimitiveHandler, PrimitiveLookupError> {
            Err(PrimitiveLookupError::InvalidPrimitiveId { id })
        }

        fn read_global(&self, id: GlobalVarId) -> Result<Value, GlobalVariableError> {
            Err(GlobalVariableError::InvalidGlobalVarId { id })
        }

        fn write_global(
            &mut self,
            id: GlobalVarId,
            _value: Value,
        ) -> Result<(), GlobalVariableError> {
            Err(GlobalVariableError::InvalidGlobalVarId { id })
        }
    }

    let mut callee_code = InstructionSequence::new();
    callee_code.append(Instruction::Return);
    let invalid_entry = CodeLocation::new(callee_code.code_space(), address(99));
    let mut caller_code = InstructionSequence::new();
    let call = caller_code.append(Instruction::Call(WordId::test_invalid(0)));
    caller_code.append(Instruction::Halt);
    let code_spaces = [caller_code.view(), callee_code.view()];
    let mut execution = InvalidCompiledEntryView {
        instructions: CodeSpaceLookup::new(&code_spaces)
            .expect("test code spaces should be distinct")
            .into(),
        entry: invalid_entry,
    };
    let mut vm = Vm::new_at_location_in(&mut execution, location(&caller_code, call))
        .expect("caller entry should be valid");
    vm.data_stack.push(value(4));
    vm.control_value_stack.push(value(5));
    let before = snapshot(&vm);

    let result = vm.step(&mut execution);

    assert_eq!(
        result,
        Err(VmError {
            location: location(&caller_code, call),
            kind: VmErrorKind::InvalidCompiledEntry {
                source: address_lookup_error(InstructionAddressError::InvalidAddress {
                    address: address(99),
                })
            }
        })
    );
    assert_vm_state(&vm, before);
    assert_eq!(vm.control_value_stack_depth(), 1);
}

#[test]
fn run_preserves_nested_call_state_when_inner_compiled_word_fails() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::Push(value(0)));
    let failing_branch = code.append(Instruction::JumpIfZero(address(99)));
    code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let outer_entry = code.append(Instruction::Push(value(11)));
    code.append(Instruction::Call(inner));
    let after_inner = code.append(Instruction::Return);
    let outer = words.add(
        CompletedWordDefinition::compiled(location(&code, outer_entry), code.view())
            .expect("outer entry should be valid"),
    );
    let entry = code.append(Instruction::Call(outer));
    let after_outer = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    let result = vm.run(execution(&code, &words, &primitives));

    let error = VmError {
        location: location(&code, failing_branch),
        kind: VmErrorKind::InvalidJumpTarget {
            source: address_lookup_error(InstructionAddressError::InvalidAddress {
                address: address(99),
            }),
        },
    };
    assert_eq!(result, Err(error));
    assert_eq!(error.address(), failing_branch);
    assert_vm_state(
        &vm,
        expected_state(
            location(&code, failing_branch),
            vec![value(11), value(0)],
            vec![
                return_frame(&code, after_outer),
                ReturnFrame::new(location(&code, after_inner), 1),
            ],
            false,
        ),
    );
}

#[test]
fn runtime_error_keeps_active_invocation_scratch_unchanged() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut code = InstructionSequence::new();
    let inner_entry = code.append(Instruction::Push(value(0)));
    let bad_branch = code.append(Instruction::JumpIfZero(address(99)));
    code.append(Instruction::Return);
    let inner = words.add(
        CompletedWordDefinition::compiled(location(&code, inner_entry), code.view())
            .expect("inner entry should be valid"),
    );
    let call = code.append(Instruction::Call(inner));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, call);
    let mut execution = execution(&code, &words, &primitives);

    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    vm.set_scratch(ScratchSlot::Y, i16::MIN).unwrap();
    assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    assert_eq!(
        vm.step(&mut execution),
        Err(VmError {
            location: location(&code, bad_branch),
            kind: VmErrorKind::InvalidJumpTarget {
                source: address_lookup_error(InstructionAddressError::InvalidAddress {
                    address: address(99),
                }),
            },
        })
    );
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.scratch(ScratchSlot::Y), Ok(i16::MIN));
}

#[test]
fn compiled_call_rejects_unregistered_entry_code_space() {
    let primitives = PrimitiveRegistry::new();
    let mut words = PublishedWords::new();
    let mut full_code = InstructionSequence::new();
    let entry = full_code.append(Instruction::Halt);
    full_code.append(Instruction::Halt);
    let compiled_entry = full_code.append(Instruction::Return);
    let word = words.add(
        CompletedWordDefinition::compiled(location(&full_code, compiled_entry), full_code.view())
            .expect("compiled entry should be valid in full code"),
    );
    let mut short_code = InstructionSequence::new();
    let call = short_code.append(Instruction::Call(word));
    short_code.append(Instruction::Halt);
    let mut vm = new_vm(&short_code, call);

    let result = vm.step(execution(&short_code, &words, &primitives));

    assert_eq!(
        result,
        Err(VmError {
            location: location(&short_code, call),
            kind: VmErrorKind::InvalidCompiledEntry {
                source: InstructionLookupError::UnknownCodeSpace {
                    code_space: full_code.code_space(),
                }
            }
        })
    );
    assert_clean_control(&vm, location(&short_code, call), false);
    assert_eq!(vm.data_stack_depth(), 0);
    assert_eq!(entry.as_index(), 0);
}

#[test]
fn run_reports_return_underflow_as_vm_error() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Return);
    let mut vm = new_vm(&code, entry);

    let result = vm.run(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::ReturnStackUnderflow {
                source: StackError::ReturnStackUnderflow
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
}

#[test]
fn run_preserves_successful_steps_before_invalid_return_target() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(5)));
    let bad_return = code.append(Instruction::Return);
    let invalid = address(100);
    let mut vm = new_vm(&code, entry);
    vm.push_return_frame(return_frame(&code, invalid));

    let result = vm.run(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, bad_return),
            kind: VmErrorKind::InvalidReturnTarget {
                source: address_lookup_error(InstructionAddressError::InvalidAddress {
                    address: invalid,
                })
            }
        })
    );
    assert_eq!(vm.instruction_pointer(), location(&code, bad_return));
    assert!(!vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 1);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(5)));
}

#[test]
fn run_keeps_successful_steps_before_error_and_failed_step_atomic() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(5)));
    let bad_branch = code.append(Instruction::JumpIfZero(address(100)));
    let mut vm = new_vm(&code, entry);

    let result = vm.run(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, bad_branch),
            kind: VmErrorKind::UnexpectedEndOfCode {
                source: address_lookup_error(InstructionAddressError::EndAddress {
                    address: address(2)
                })
            }
        })
    );
    assert_clean_control(&vm, location(&code, bad_branch), false);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(5)));
}

#[test]
fn vm_error_exposes_failed_instruction_address_and_kind() {
    let code = InstructionSequence::new();
    let address = address(3);
    let location = location(&code, address);
    let error = VmError {
        location,
        kind: VmErrorKind::DataStackUnderflow {
            source: StackError::DataStackUnderflow,
        },
    };

    assert_eq!(error.address(), address);
    assert_eq!(error.location(), location);
    assert_eq!(
        error.kind(),
        VmErrorKind::DataStackUnderflow {
            source: StackError::DataStackUnderflow
        }
    );
}
