use super::*;

#[test]
fn load_var_pushes_global_value_and_advances() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let id = globals.allocate();
    globals
        .view_mut()
        .write(id, value(37))
        .expect("allocated global should be valid");
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::LoadVar(id));
    let next = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.step(execution_with_globals(
            &code,
            &words,
            &primitives,
            &mut globals
        )),
        Ok(StepOutcome::Continued)
    );

    assert_clean_control(&vm, location(&code, next), false);
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(37)));
}

#[test]
fn store_var_consumes_value_and_updates_global() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let id = globals.allocate();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(82)));
    let store = code.append(Instruction::StoreVar(id));
    let next = code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    {
        let mut execution = execution_with_globals(&code, &words, &primitives, &mut globals);
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(&mut execution), Ok(StepOutcome::Continued));
    }

    assert_clean_control(&vm, location(&code, next), false);
    assert_eq!(vm.data_stack_depth(), 0);
    assert_eq!(globals.view().read(id), Ok(value(82)));
    assert_eq!(store.as_index(), 1);
}

#[test]
fn store_then_load_round_trips_global_value() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let id = globals.allocate();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(-13)));
    code.append(Instruction::StoreVar(id));
    code.append(Instruction::LoadVar(id));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution_with_globals(
            &code,
            &words,
            &primitives,
            &mut globals
        )),
        Ok(RunOutcome::Halted)
    );

    assert!(vm.is_halted());
    assert_eq!(vm.data_stack_depth(), 1);
    assert_eq!(vm.peek_data(), Ok(value(-13)));
    assert_eq!(globals.view().read(id), Ok(value(-13)));
}

#[test]
fn multiple_global_slots_keep_independent_runtime_identity() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let first = globals.allocate();
    let second = globals.allocate();
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(10)));
    code.append(Instruction::StoreVar(first));
    code.append(Instruction::Push(value(20)));
    code.append(Instruction::StoreVar(second));
    code.append(Instruction::LoadVar(first));
    code.append(Instruction::LoadVar(second));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution_with_globals(
            &code,
            &words,
            &primitives,
            &mut globals
        )),
        Ok(RunOutcome::Halted)
    );

    assert_eq!(vm.pop_data(), Ok(value(20)));
    assert_eq!(vm.pop_data(), Ok(value(10)));
    assert_eq!(globals.view().read(first), Ok(value(10)));
    assert_eq!(globals.view().read(second), Ok(value(20)));
}

#[test]
fn array_load_uses_one_origin_indices_and_replaces_the_index() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let id = arrays.allocate(3);
    {
        let mut view = arrays.view_mut();
        view.write(id, 0, value(11)).unwrap();
        view.write(id, 1, value(22)).unwrap();
        view.write(id, 2, value(33)).unwrap();
    }
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(1)));
    code.append(Instruction::LoadArrayElement(id));
    code.append(Instruction::Push(value(3)));
    code.append(Instruction::LoadArrayElement(id));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution_with_arrays(
            &code,
            &words,
            &primitives,
            &mut arrays
        )),
        Ok(RunOutcome::Halted)
    );
    assert_eq!(vm.data_stack.as_slice(), &[value(11), value(33)]);
}

#[test]
fn array_store_updates_only_selected_element_and_consumes_operands() {
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let id = arrays.allocate(3);
    let mut code = InstructionSequence::new();
    let entry = code.append(Instruction::Push(value(2)));
    code.append(Instruction::Push(value(42)));
    code.append(Instruction::StoreArrayElement(id));
    code.append(Instruction::Halt);
    let mut vm = new_vm(&code, entry);

    assert_eq!(
        vm.run(execution_with_arrays(
            &code,
            &words,
            &primitives,
            &mut arrays
        )),
        Ok(RunOutcome::Halted)
    );
    assert!(vm.data_stack.is_empty());
    assert_eq!(arrays.view().read(id, 0), Ok(value(0)));
    assert_eq!(arrays.view().read(id, 1), Ok(value(42)));
    assert_eq!(arrays.view().read(id, 2), Ok(value(0)));
}
