use crate::global_variable::{
    GlobalVarId, GlobalVariableError, GlobalVariableView, GlobalVariableViewMut,
};
use crate::instruction::{
    CodeLocation, CodeSpaceLookup, Instruction, InstructionAddress, InstructionLookup,
    InstructionLookupError, InstructionView,
};
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveLookup, PrimitiveLookupError};
use crate::stack::{DataStack, ReturnFrame, ReturnStack, StackError};
use crate::value::Value;
use crate::word::{WordDefinition, WordId, WordLookupError};
use crate::word_lookup::PublishedWordLookup;

/// Mutable execution state for the initial TBX Next VM core.
///
/// The VM owns only mutable control/data state. It does not own the shared
/// instruction sequence, word registry, bindings, or any builder/publication
/// surface. Callers pass `InstructionView` to execution methods so the VM can
/// fetch and validate instructions without gaining append or mutation access.
#[derive(Debug)]
pub(crate) struct Vm {
    instruction_pointer: CodeLocation,
    data_stack: DataStack,
    return_stack: ReturnStack,
    halted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepOutcome {
    Continued,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmError {
    location: CodeLocation,
    kind: VmErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmErrorKind {
    InstructionFetch {
        source: InstructionLookupError,
    },
    UnexpectedEndOfCode {
        source: InstructionLookupError,
    },
    DataStackUnderflow {
        source: StackError,
    },
    ReturnStackUnderflow {
        source: StackError,
    },
    InvalidJumpTarget {
        source: InstructionLookupError,
    },
    InvalidReturnTarget {
        source: InstructionLookupError,
    },
    InvalidWordId {
        source: WordLookupError,
    },
    InvalidPrimitiveId {
        source: PrimitiveLookupError,
    },
    PrimitiveFailed {
        primitive: crate::word::PrimitiveId,
        source: PrimitiveError,
    },
    InvalidGlobalVarId {
        source: GlobalVariableError,
    },
    InvalidCompiledEntry {
        source: InstructionLookupError,
    },
}

#[derive(Debug)]
pub(crate) struct ExecutionView<'a> {
    instructions: InstructionLookup<'a>,
    words: PublishedWordLookup<'a>,
    primitives: PrimitiveLookup<'a>,
    globals: Option<GlobalExecutionAccess<'a>>,
}

#[derive(Debug)]
enum GlobalExecutionAccess<'a> {
    Read(GlobalVariableView<'a>),
    Write(GlobalVariableViewMut<'a>),
}

impl<'a> ExecutionView<'a> {
    pub(crate) fn new(
        instructions: InstructionView<'a>,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self::with_instruction_lookup(instructions.into(), words, primitives)
    }

    pub(crate) const fn with_instruction_lookup(
        instructions: InstructionLookup<'a>,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            instructions,
            words,
            primitives,
            globals: None,
        }
    }

    pub(crate) fn with_code_spaces(
        code_spaces: CodeSpaceLookup<'a>,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self::with_instruction_lookup(code_spaces.into(), words, primitives)
    }

    pub(crate) fn with_globals(mut self, globals: GlobalVariableViewMut<'a>) -> Self {
        self.globals = Some(GlobalExecutionAccess::Write(globals));
        self
    }

    pub(crate) fn with_global_reader(mut self, globals: GlobalVariableView<'a>) -> Self {
        self.globals = Some(GlobalExecutionAccess::Read(globals));
        self
    }

    pub(crate) const fn instructions(self) -> InstructionLookup<'a> {
        self.instructions
    }

    pub(crate) const fn words(self) -> PublishedWordLookup<'a> {
        self.words
    }

    pub(crate) const fn primitives(self) -> PrimitiveLookup<'a> {
        self.primitives
    }
}

pub(crate) trait VmExecutionView<'a> {
    fn instructions(&self) -> InstructionLookup<'a>;

    fn lookup_word(&self, id: WordId) -> Result<WordDefinition, WordLookupError>;

    fn lookup_handler(
        &self,
        id: crate::word::PrimitiveId,
    ) -> Result<crate::primitive::PrimitiveHandler, PrimitiveLookupError>;

    fn read_global(&self, id: GlobalVarId) -> Result<Value, GlobalVariableError>;

    fn write_global(&mut self, id: GlobalVarId, value: Value) -> Result<(), GlobalVariableError>;
}

impl<'a> VmExecutionView<'a> for ExecutionView<'a> {
    fn instructions(&self) -> InstructionLookup<'a> {
        self.instructions
    }

    fn lookup_word(&self, id: WordId) -> Result<WordDefinition, WordLookupError> {
        self.words.lookup_word(id).copied()
    }

    fn lookup_handler(
        &self,
        id: crate::word::PrimitiveId,
    ) -> Result<crate::primitive::PrimitiveHandler, PrimitiveLookupError> {
        self.primitives.lookup_handler(id)
    }

    fn read_global(&self, id: GlobalVarId) -> Result<Value, GlobalVariableError> {
        match &self.globals {
            Some(GlobalExecutionAccess::Read(globals)) => globals.read(id),
            Some(GlobalExecutionAccess::Write(globals)) => globals.read(id),
            None => Err(GlobalVariableError::InvalidGlobalVarId { id }),
        }
    }

    fn write_global(&mut self, id: GlobalVarId, value: Value) -> Result<(), GlobalVariableError> {
        match &mut self.globals {
            Some(GlobalExecutionAccess::Write(globals)) => globals.write(id, value),
            Some(GlobalExecutionAccess::Read(_)) | None => {
                Err(GlobalVariableError::InvalidGlobalVarId { id })
            }
        }
    }
}

impl<'a> VmExecutionView<'a> for InstructionView<'a> {
    fn instructions(&self) -> InstructionLookup<'a> {
        (*self).into()
    }

    fn lookup_word(&self, id: WordId) -> Result<WordDefinition, WordLookupError> {
        Err(WordLookupError::InvalidWordId { id })
    }

    fn lookup_handler(
        &self,
        id: crate::word::PrimitiveId,
    ) -> Result<crate::primitive::PrimitiveHandler, PrimitiveLookupError> {
        Err(PrimitiveLookupError::InvalidPrimitiveId { id })
    }

    fn read_global(&self, id: GlobalVarId) -> Result<Value, GlobalVariableError> {
        Err(GlobalVariableError::InvalidGlobalVarId { id })
    }

    fn write_global(&mut self, id: GlobalVarId, _value: Value) -> Result<(), GlobalVariableError> {
        Err(GlobalVariableError::InvalidGlobalVarId { id })
    }
}

impl<'a, T: VmExecutionView<'a> + ?Sized> VmExecutionView<'a> for &mut T {
    fn instructions(&self) -> InstructionLookup<'a> {
        (**self).instructions()
    }

    fn lookup_word(&self, id: WordId) -> Result<WordDefinition, WordLookupError> {
        (**self).lookup_word(id)
    }

    fn lookup_handler(
        &self,
        id: crate::word::PrimitiveId,
    ) -> Result<crate::primitive::PrimitiveHandler, PrimitiveLookupError> {
        (**self).lookup_handler(id)
    }

    fn read_global(&self, id: GlobalVarId) -> Result<Value, GlobalVariableError> {
        (**self).read_global(id)
    }

    fn write_global(&mut self, id: GlobalVarId, value: Value) -> Result<(), GlobalVariableError> {
        (**self).write_global(id, value)
    }
}

impl Vm {
    pub(crate) fn new(
        instructions: InstructionView<'_>,
        entry: InstructionAddress,
    ) -> Result<Self, VmError> {
        let location = instructions.location(entry);
        Self::new_at_location(instructions, location)
    }

    pub(crate) fn new_at_location(
        instructions: InstructionView<'_>,
        entry: CodeLocation,
    ) -> Result<Self, VmError> {
        Self::new_at_location_in(instructions, entry)
    }

    pub(crate) fn new_at_location_in<'a, E: VmExecutionView<'a>>(
        execution: E,
        entry: CodeLocation,
    ) -> Result<Self, VmError> {
        execution
            .instructions()
            .validate_location(entry)
            .map_err(|source| VmError {
                location: entry,
                kind: VmErrorKind::InstructionFetch { source },
            })?;

        Ok(Self {
            instruction_pointer: entry,
            data_stack: DataStack::new(),
            return_stack: ReturnStack::new(),
            halted: false,
        })
    }

    pub(crate) fn step<'a, E: VmExecutionView<'a>>(
        &mut self,
        mut execution: E,
    ) -> Result<StepOutcome, VmError> {
        if self.halted {
            return Ok(StepOutcome::Halted);
        }

        let location = self.instruction_pointer;
        let instructions = execution.instructions();
        let instruction = *instructions
            .get_location(location)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InstructionFetch { source },
            })?;

        match instruction {
            Instruction::Push(value) => self.step_push(instructions, location, value),
            Instruction::LoadVar(id) => self.step_load_var(&mut execution, location, id),
            Instruction::StoreVar(id) => self.step_store_var(&mut execution, location, id),
            Instruction::Call(id) => self.step_call(execution, location, id),
            Instruction::Jump(target) => self.step_jump(instructions, location, target),
            Instruction::JumpIfZero(target) => {
                self.step_jump_if_zero(instructions, location, target)
            }
            Instruction::Return => self.step_return(instructions, location),
            Instruction::Halt => {
                // Halt commits only the state transition. In halted state, the
                // IP records the Halt instruction that stopped execution; it is
                // no longer interpreted as the next instruction to fetch.
                self.halted = true;
                Ok(StepOutcome::Halted)
            }
        }
    }

    pub(crate) fn run<'a, E: VmExecutionView<'a>>(
        &mut self,
        mut execution: E,
    ) -> Result<RunOutcome, VmError> {
        loop {
            match self.step(&mut execution)? {
                StepOutcome::Continued => {}
                StepOutcome::Halted => return Ok(RunOutcome::Halted),
            }
        }
    }

    pub(crate) const fn instruction_pointer(&self) -> CodeLocation {
        self.instruction_pointer
    }

    pub(crate) const fn is_halted(&self) -> bool {
        self.halted
    }

    pub(crate) fn data_stack_depth(&self) -> usize {
        self.data_stack.depth()
    }

    pub(crate) fn return_stack_depth(&self) -> usize {
        self.return_stack.depth()
    }

    #[cfg(test)]
    fn push_return_frame(&mut self, frame: ReturnFrame) {
        self.return_stack.push(frame);
    }

    pub(crate) fn peek_data(&self) -> Result<Value, StackError> {
        self.data_stack.peek()
    }

    pub(crate) fn pop_data(&mut self) -> Result<Value, StackError> {
        self.data_stack.pop()
    }

    fn step_push(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        value: Value,
    ) -> Result<StepOutcome, VmError> {
        let next = self.valid_next_location(instructions, location)?;

        self.data_stack.push(value);
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_load_var<'a, E: VmExecutionView<'a>>(
        &mut self,
        execution: &mut E,
        location: CodeLocation,
        id: GlobalVarId,
    ) -> Result<StepOutcome, VmError> {
        let instructions = execution.instructions();
        let next = self.valid_next_location(instructions, location)?;
        let value = execution.read_global(id).map_err(|source| VmError {
            location,
            kind: VmErrorKind::InvalidGlobalVarId { source },
        })?;

        // ADR #1370 variables are external storage, while ADR #1367 keeps this
        // VM instruction atomic: only after all fallible checks succeed do we
        // publish the stack/IP state transition.
        self.data_stack.push(value);
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_store_var<'a, E: VmExecutionView<'a>>(
        &mut self,
        execution: &mut E,
        location: CodeLocation,
        id: GlobalVarId,
    ) -> Result<StepOutcome, VmError> {
        self.data_stack.require_depth(1).map_err(|source| VmError {
            location,
            kind: VmErrorKind::DataStackUnderflow { source },
        })?;
        let value = self
            .data_stack
            .peek()
            .expect("depth was checked before reading StoreVar value");
        let instructions = execution.instructions();
        let next = self.valid_next_location(instructions, location)?;
        execution.read_global(id).map_err(|source| VmError {
            location,
            kind: VmErrorKind::InvalidGlobalVarId { source },
        })?;

        // Validate before VM mutation so storage, stack, and IP commit through
        // the external storage boundary without converting trait errors into
        // panics.
        execution
            .write_global(id, value)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InvalidGlobalVarId { source },
            })?;
        self.data_stack
            .pop()
            .expect("depth was checked before consuming StoreVar value");
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_jump(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        target: InstructionAddress,
    ) -> Result<StepOutcome, VmError> {
        let target = self.valid_jump_target(instructions, location, target)?;

        self.instruction_pointer = target;

        Ok(StepOutcome::Continued)
    }

    fn step_call<'a, E: VmExecutionView<'a>>(
        &mut self,
        execution: E,
        location: CodeLocation,
        id: WordId,
    ) -> Result<StepOutcome, VmError> {
        let instructions = execution.instructions();
        let next = self.valid_next_location(instructions, location)?;
        let definition = execution.lookup_word(id).map_err(|source| VmError {
            location,
            kind: VmErrorKind::InvalidWordId { source },
        })?;

        match definition {
            WordDefinition::Primitive { primitive } => {
                let handler = execution
                    .lookup_handler(primitive)
                    .map_err(|source| VmError {
                        location,
                        kind: VmErrorKind::InvalidPrimitiveId { source },
                    })?;

                let checkpoint = self.data_stack.clone();
                let mut context = PrimitiveContext::new(&mut self.data_stack);
                match handler(&mut context) {
                    Ok(()) => {
                        self.instruction_pointer = next;
                        Ok(StepOutcome::Continued)
                    }
                    Err(source) => {
                        // Primitive calls are a VM commit boundary: handlers
                        // may perform multiple data-stack operations, so the VM
                        // restores the entry checkpoint on failure instead of
                        // relying on every handler to be internally atomic.
                        self.data_stack.restore(checkpoint);
                        Err(VmError {
                            location,
                            kind: VmErrorKind::PrimitiveFailed { primitive, source },
                        })
                    }
                }
            }
            WordDefinition::Compiled { entry } => {
                let entry = self.valid_compiled_entry(instructions, location, entry)?;

                self.return_stack.push(ReturnFrame::new(next));
                self.instruction_pointer = entry;

                Ok(StepOutcome::Continued)
            }
        }
    }

    fn step_jump_if_zero(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        target: InstructionAddress,
    ) -> Result<StepOutcome, VmError> {
        self.data_stack.require_depth(1).map_err(|source| VmError {
            location,
            kind: VmErrorKind::DataStackUnderflow { source },
        })?;

        let condition = self
            .data_stack
            .peek()
            .expect("depth was checked before reading JumpIfZero condition");
        let next = if condition.is_zero() {
            self.valid_jump_target(instructions, location, target)?
        } else {
            self.valid_next_location(instructions, location)?
        };

        self.data_stack
            .pop()
            .expect("depth was checked before consuming JumpIfZero condition");
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_return(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
    ) -> Result<StepOutcome, VmError> {
        let frame = self.return_stack.peek().map_err(|source| VmError {
            location,
            kind: VmErrorKind::ReturnStackUnderflow { source },
        })?;
        let target = self.valid_return_target(instructions, location, frame.return_location())?;

        self.return_stack
            .pop()
            .expect("return frame was checked before consuming Return frame");
        self.instruction_pointer = target;

        Ok(StepOutcome::Continued)
    }

    fn valid_next_location(
        &self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
    ) -> Result<CodeLocation, VmError> {
        instructions
            .checked_next_location(location)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::UnexpectedEndOfCode { source },
            })
    }

    fn valid_jump_target(
        &self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        target: InstructionAddress,
    ) -> Result<CodeLocation, VmError> {
        let target = CodeLocation::new(location.code_space(), target);
        instructions
            .validate_location(target)
            .map(|_| target)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InvalidJumpTarget { source },
            })
    }

    fn valid_return_target(
        &self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        target: CodeLocation,
    ) -> Result<CodeLocation, VmError> {
        instructions
            .validate_location(target)
            .map(|_| target)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InvalidReturnTarget { source },
            })
    }

    fn valid_compiled_entry(
        &self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        entry: CodeLocation,
    ) -> Result<CodeLocation, VmError> {
        instructions
            .validate_location(entry)
            .map(|_| entry)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InvalidCompiledEntry { source },
            })
    }
}

impl VmError {
    pub(crate) const fn location(self) -> CodeLocation {
        self.location
    }

    pub(crate) const fn address(self) -> InstructionAddress {
        self.location.address()
    }

    pub(crate) const fn kind(self) -> VmErrorKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_variable::GlobalVariables;
    use crate::instruction::InstructionAddressError;
    use crate::instruction::InstructionSequence;
    use crate::primitive::{PrimitiveLookupError, PrimitiveRegistry};
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
        ReturnFrame::new(location(code, return_address))
    }

    fn push_42(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(42));
        Ok(())
    }

    fn add_top_two(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        let (lhs, rhs) = context.pop2()?;
        context.push(value(lhs.as_integer() + rhs.as_integer()));
        Ok(())
    }

    fn fail_after_partial_stack_update(
        context: &mut PrimitiveContext<'_>,
    ) -> Result<(), PrimitiveError> {
        context.pop()?;
        context.push(value(99));
        Err(PrimitiveError::Failed)
    }

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
                    source: address_lookup_error(InstructionAddressError::EndAddress {
                        address: end,
                    })
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
                    source: address_lookup_error(InstructionAddressError::EndAddress {
                        address: end,
                    })
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
        vm.push_return_frame(return_frame(&code, invalid));

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
    fn primitive_failure_restores_data_stack_and_preserves_control_state() {
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail_after_partial_stack_update);
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
    fn run_keeps_successful_primitive_state_before_failed_primitive_call() {
        let mut primitives = PrimitiveRegistry::new();
        let push = primitives.register(push_42);
        let fail = primitives.register(fail_after_partial_stack_update);
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

    #[test]
    fn call_rejects_unpublished_word_id_without_mutation() {
        let mut other_words = PublishedWords::new();
        let unpublished = other_words.add(CompletedWordDefinition::primitive(
            PrimitiveId::from_slot(0),
        ));
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
    fn compiled_call_enters_published_code_space_and_returns_to_caller_space() {
        let primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut callee_code = InstructionSequence::new();
        let callee_entry = callee_code.append(Instruction::Push(value(17)));
        callee_code.append(Instruction::Return);
        let word = words.add(
            CompletedWordDefinition::compiled(
                location(&callee_code, callee_entry),
                callee_code.view(),
            )
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
            CompletedWordDefinition::compiled(
                location(&level3_code, level3_entry),
                level3_code.view(),
            )
            .expect("level3 entry should be valid"),
        );

        let mut level2_code = InstructionSequence::new();
        let level2_entry = level2_code.append(Instruction::Push(value(2)));
        level2_code.append(Instruction::Call(level3));
        level2_code.append(Instruction::Return);
        let level2 = words.add(
            CompletedWordDefinition::compiled(
                location(&level2_code, level2_entry),
                level2_code.view(),
            )
            .expect("level2 entry should be valid"),
        );

        let mut level1_code = InstructionSequence::new();
        let level1_entry = level1_code.append(Instruction::Push(value(1)));
        level1_code.append(Instruction::Call(level2));
        level1_code.append(Instruction::Return);
        let level1 = words.add(
            CompletedWordDefinition::compiled(
                location(&level1_code, level1_entry),
                level1_code.view(),
            )
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
            CompletedWordDefinition::compiled(
                location(&callee_code, callee_entry),
                callee_code.view(),
            )
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
            CompletedWordDefinition::compiled(
                location(&callee_code, callee_entry),
                callee_code.view(),
            )
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
                    return_frame(&code, after_inner),
                ],
                false,
            ),
        );
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
            CompletedWordDefinition::compiled(
                location(&full_code, compiled_entry),
                full_code.view(),
            )
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
}
