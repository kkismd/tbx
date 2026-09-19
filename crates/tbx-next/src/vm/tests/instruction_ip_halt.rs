use super::*;

#[test]
fn new_rejects_invalid_initial_instruction_pointer() {
    let code = InstructionSequence::new();
    let entry = address(0);

    assert_eq!(
        Vm::new(code.view(), entry).expect_err("empty code should reject entry"),
        VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InstructionFetch {
                source: address_lookup_error(InstructionAddressError::EndAddress {
                    address: entry,
                })
            }
        }
    );
}

#[test]
fn new_at_location_rejects_unregistered_entry_code_space_without_index_fallback() {
    let mut source = InstructionSequence::new();
    let source_entry = source.append(Instruction::Halt);
    let mut target = InstructionSequence::new();
    let target_entry = target.append(Instruction::Push(value(99)));
    let entry = location(&source, source_entry);

    assert_eq!(source_entry.as_index(), target_entry.as_index());
    assert_eq!(
        Vm::new_at_location(target.view(), entry).expect_err("entry owner should mismatch"),
        VmError {
            location: entry,
            kind: VmErrorKind::InstructionFetch {
                source: InstructionLookupError::UnknownCodeSpace {
                    code_space: source.view().code_space(),
                }
            }
        }
    );
}

#[test]
fn new_records_same_owner_entry_location() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Halt);

    let vm = new_vm(&code, entry);

    assert_clean_control(&vm, location(&code, entry), false);
}

#[test]
fn push_step_stores_value_and_advances_to_existing_next_instruction() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(10)));
    let next = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&code, next), false);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(10)));
}

#[test]
fn push_at_end_reports_unexpected_end_without_mutation() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(7)));
    let mut vm = new_vm(&code, entry);

    let result = vm.step(code.view());

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
fn step_rejects_unregistered_current_code_space_without_mutation() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Halt);
    let other = InstructionSequence::new();
    let mut vm = new_vm(&code, entry);

    let result = vm.step(other.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InstructionFetch {
                source: InstructionLookupError::UnknownCodeSpace {
                    code_space: code.view().code_space(),
                }
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn jump_moves_to_valid_target() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Halt);
    let jump = code.append(Instruction::Jump(entry));
    let mut vm = new_vm(&code, jump);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&code, entry), false);
}

#[test]
fn jump_can_move_backward_in_one_step() {
    let mut code = InstructionSequence::new();
    let target = code.append(Instruction::Halt);
    let entry = code.append(Instruction::Jump(target));
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_eq!(vm.instruction_pointer(), location(&code, target));
}

#[test]
fn jump_rejects_invalid_target_without_mutation() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Jump(address(10)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidJumpTarget {
                source: address_lookup_error(InstructionAddressError::InvalidAddress {
                    address: address(10)
                })
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn jump_rejects_end_target_without_mutation() {
    let mut code = InstructionSequence::new();
    let end = address(1);
    let entry = code.append(Instruction::Jump(end));
    let mut vm = new_vm(&code, entry);

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::InvalidJumpTarget {
                source: address_lookup_error(InstructionAddressError::EndAddress { address: end })
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
}

#[test]
fn jump_if_zero_takes_target_and_consumes_condition() {
    let mut code = InstructionSequence::new();
    let push = code.append(Instruction::Push(value(0)));
    let branch = code.append(Instruction::JumpIfZero(address(3)));
    code.append(Instruction::Halt);
    let target = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, push);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&code, target), false);
    assert_eq!(vm.data_stack_depth(), 0);
    assert_eq!(branch.as_index(), 1);
}

#[test]
fn jump_if_zero_falls_through_on_non_zero_and_consumes_condition() {
    let mut code = InstructionSequence::new();
    let push = code.append(Instruction::Push(value(1)));
    let branch = code.append(Instruction::JumpIfZero(address(3)));
    let next = code.append(Instruction::Halt);
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, push);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

    assert_clean_control(&vm, location(&code, next), false);
    assert_eq!(vm.data_stack_depth(), 0);
    assert_eq!(branch.as_index(), 1);
}

#[test]
fn jump_if_zero_underflow_preserves_state() {
    let mut code = InstructionSequence::new();
    let target = code.append(Instruction::Halt);
    let entry = code.append(Instruction::JumpIfZero(target));
    let mut vm = new_vm(&code, entry);

    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, entry),
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow
            }
        })
    );
    assert_clean_control(&vm, location(&code, entry), false);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn jump_if_zero_invalid_target_does_not_consume_condition() {
    let mut code = InstructionSequence::new();
    let push = code.append(Instruction::Push(value(0)));
    let branch = code.append(Instruction::JumpIfZero(address(99)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, push);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, branch),
            kind: VmErrorKind::InvalidJumpTarget {
                source: address_lookup_error(InstructionAddressError::InvalidAddress {
                    address: address(99)
                })
            }
        })
    );
    assert_clean_control(&vm, location(&code, branch), false);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(0)));
}

#[test]
fn jump_if_zero_missing_fallthrough_does_not_consume_condition() {
    let mut code = InstructionSequence::new();
    let push = code.append(Instruction::Push(value(1)));
    let branch = code.append(Instruction::JumpIfZero(push));
    let mut vm = new_vm(&code, push);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
    let result = vm.step(code.view());

    assert_eq!(
        result,
        Err(VmError {
            location: location(&code, branch),
            kind: VmErrorKind::UnexpectedEndOfCode {
                source: address_lookup_error(InstructionAddressError::EndAddress {
                    address: address(2)
                })
            }
        })
    );
    assert_clean_control(&vm, location(&code, branch), false);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(1)));
}

#[test]
fn halt_sets_halted_without_changing_stacks_or_instruction_pointer() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Halted));

    assert_clean_control(&vm, location(&code, entry), true);
    assert_eq!(vm.data_stack_depth(), 0);
}

#[test]
fn halt_at_end_keeps_instruction_pointer_on_halt_instruction() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(8)));
    let halt = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert_vm_state(
        &vm,
        expected_state(location(&code, halt), vec![value(8)], Vec::new(), true),
    );
}

#[test]
fn halted_step_and_run_are_idempotent() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.data_stack.push(value(3));
    vm.push_return_frame(return_frame(&code, entry));

    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Halted));
    let halted = snapshot(&vm);
    assert_eq!(vm.step(code.view()), Ok(StepOutcome::Halted));
    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert_vm_state(&vm, halted);
}

#[test]
fn run_executes_until_halt() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::Push(value(2)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert!(vm.is_halted());
    assert_eq!(vm.data_stack_depth(), 2);
    assert_eq!(vm.pop_data(), Ok(value(2)));
    assert_eq!(vm.pop_data(), Ok(value(1)));
}

#[test]
fn run_uses_jump_semantics() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Jump(address(2)));
    code.append(Instruction::Push(value(99)));
    code.append(Instruction::Push(value(7)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(7)));
}

#[test]
fn run_uses_conditional_branch_semantics() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(0)));
    code.append(Instruction::JumpIfZero(address(3)));
    code.append(Instruction::Halt);
    code.append(Instruction::Push(value(42)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(42)));
}

#[test]
fn run_continues_after_return_until_halt() {
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Return);
    code.append(Instruction::Push(value(99)));
    let target = code.append(Instruction::Push(value(8)));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);
    vm.push_return_frame(return_frame(&code, target));

    assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

    assert!(vm.is_halted());
    assert_eq!(vm.return_stack_depth(), 0);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(8)));
}
