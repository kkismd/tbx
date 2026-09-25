use super::*;
use crate::global_variable::GlobalVariables;
use crate::instruction::InstructionAddressError;
use crate::instruction::InstructionSequence;
use crate::primitive::{PrimitiveLookupError, PrimitiveRegistry};
use crate::runtime_output::{RuntimeOutputError, TestOutput};
use crate::word::{CompletedWordDefinition, PrimitiveId, PublishedWords, WordLookupError};
use crate::word_lookup::PublishedWordLookup;

fn value(value: i16) -> Value {
    Value::integer(value)
}

fn address(index: usize) -> InstructionAddress {
    InstructionAddress::from_index(index)
}

fn address_lookup_error(source: InstructionAddressError) -> InstructionLookupError {
    InstructionLookupError::Address { source }
}

fn new_vm(code: &InstructionSequence, entry: InstructionAddress) -> Vm {
    Vm::new(code.view(), entry).expect("test entry should be valid")
}

fn location(code: &InstructionSequence, address: InstructionAddress) -> CodeLocation {
    code.view().location(address)
}

fn execution<'a>(
    code: &'a InstructionSequence,
    words: &'a PublishedWords,
    primitives: &'a PrimitiveRegistry,
) -> ExecutionView<'a> {
    ExecutionView::new(
        code.view(),
        PublishedWordLookup::new(words),
        primitives.lookup(),
    )
}

fn multi_execution<'a>(
    code_spaces: &'a [InstructionView<'a>],
    words: &'a PublishedWords,
    primitives: &'a PrimitiveRegistry,
) -> ExecutionView<'a> {
    ExecutionView::with_code_spaces(
        CodeSpaceLookup::new(code_spaces).expect("test code spaces should be distinct"),
        PublishedWordLookup::new(words),
        primitives.lookup(),
    )
}

fn execution_with_globals<'a>(
    code: &'a InstructionSequence,
    words: &'a PublishedWords,
    primitives: &'a PrimitiveRegistry,
    globals: &'a mut GlobalVariables,
) -> ExecutionView<'a> {
    execution(code, words, primitives).with_globals(globals.view_mut())
}

fn execution_with_arrays<'a>(
    code: &'a InstructionSequence,
    words: &'a PublishedWords,
    primitives: &'a PrimitiveRegistry,
    arrays: &'a mut crate::global_array::GlobalArrays,
) -> ExecutionView<'a> {
    execution(code, words, primitives).with_arrays(arrays.view_mut())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VmSnapshot {
    instruction_pointer: CodeLocation,
    data_stack: Vec<Value>,
    return_stack: Vec<ReturnFrame>,
    halted: bool,
}

fn snapshot(vm: &Vm) -> VmSnapshot {
    VmSnapshot {
        instruction_pointer: vm.instruction_pointer,
        data_stack: vm.data_stack.as_slice().to_vec(),
        return_stack: vm.return_stack.as_slice().to_vec(),
        halted: vm.halted,
    }
}

fn assert_vm_state(vm: &Vm, expected: VmSnapshot) {
    assert_eq!(snapshot(vm), expected);
}

fn expected_state(
    instruction_pointer: CodeLocation,
    data_stack: Vec<Value>,
    return_stack: Vec<ReturnFrame>,
    halted: bool,
) -> VmSnapshot {
    VmSnapshot {
        instruction_pointer,
        data_stack,
        return_stack,
        halted,
    }
}

fn assert_clean_control(vm: &Vm, expected_ip: CodeLocation, halted: bool) {
    assert_eq!(vm.instruction_pointer(), expected_ip);
    assert_eq!(vm.is_halted(), halted);
    assert_eq!(vm.return_stack_depth(), 0);
}

fn return_frame(code: &InstructionSequence, return_address: InstructionAddress) -> ReturnFrame {
    ReturnFrame::new(location(code, return_address), 0)
}

fn push_42(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    context.push(value(42));
    Ok(())
}

fn add_top_two(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    let (lhs, rhs) = context.pop2()?;
    context.push(value(lhs.as_integer() + rhs.as_integer()));
    Ok(())
}

fn fail_without_stack_update(
    _context: &mut PrimitiveContext<'_, '_>,
) -> Result<(), PrimitiveError> {
    Err(PrimitiveError::Failed)
}

fn write_alpha(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    context.write_output("alpha")
}

fn write_beta(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    context.write_output("beta")
}

fn write_empty(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    context.write_output("")
}

fn write_then_pop_output(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    context.peek()?;
    context.write_output("after-peek")?;
    context
        .pop()
        .expect("output operand was checked before consumption");
    Ok(())
}

mod control_value_lifo;
mod data_stack_return_stack_call;
mod global_variable_array;
mod instruction_ip_halt;
mod primitive_capability_io_random;
mod runtime_error_failure_state;
mod scratch_instruction;
