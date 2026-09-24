use crate::global_array::{ArrayId, GlobalArrayError, GlobalArrayView, GlobalArrayViewMut};
use crate::global_variable::{
    GlobalVarId, GlobalVariableError, GlobalVariableView, GlobalVariableViewMut,
};
use crate::instruction::{
    CodeLocation, CodeSpaceLookup, Instruction, InstructionAddress, InstructionLookup,
    InstructionLookupError, InstructionView,
};
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveLookup, PrimitiveLookupError};
use crate::random::RandomState;
use crate::runtime_input::RuntimeInput;
use crate::runtime_output::RuntimeOutput;
use crate::stack::{ControlValueStack, DataStack, ReturnFrame, ReturnStack, StackError};
use crate::value::Value;
use crate::word::{WordDefinition, WordId, WordLookupError};
use crate::word_lookup::PublishedWordLookup;
use std::fmt;

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
    control_value_stack: ControlValueStack,
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
    ControlValueStackUnderflow {
        source: StackError,
    },
    ControlValueStackDepthBelowCall {
        call_depth: usize,
        current_depth: usize,
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
    FixedTextOutputFailed {
        source: crate::runtime_output::RuntimeOutputError,
    },
    InvalidGlobalVarId {
        source: GlobalVariableError,
    },
    InvalidGlobalArray {
        source: GlobalArrayError,
    },
    InvalidCompiledEntry {
        source: InstructionLookupError,
    },
    CallBaseValueCopy {
        source: CallBaseValueError,
    },
    CallBaseDataStackTruncate {
        source: CallBaseDataStackTruncateError,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallBaseValueError {
    NoCompiledWordInvocation,
    InvalidOffset {
        offset: usize,
        call_data_stack_depth: usize,
    },
    DataStackIndexOutOfBounds {
        index: usize,
        depth: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallBaseDataStackTruncateError {
    NoCompiledWordInvocation,
    CurrentDepthBelowCallBase {
        current_depth: usize,
        call_data_stack_depth: usize,
    },
}

pub(crate) struct ExecutionView<'a> {
    instructions: InstructionLookup<'a>,
    words: PublishedWordLookup<'a>,
    primitives: PrimitiveLookup<'a>,
    globals: Option<GlobalExecutionAccess<'a>>,
    arrays: Option<GlobalArrayExecutionAccess<'a>>,
    output: Option<&'a mut dyn RuntimeOutput>,
    input: Option<&'a mut dyn RuntimeInput>,
    random: Option<&'a mut RandomState>,
}

#[derive(Debug)]
enum GlobalExecutionAccess<'a> {
    Read(GlobalVariableView<'a>),
    Write(GlobalVariableViewMut<'a>),
}

#[derive(Debug)]
enum GlobalArrayExecutionAccess<'a> {
    Read(GlobalArrayView<'a>),
    Write(GlobalArrayViewMut<'a>),
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
            arrays: None,
            output: None,
            input: None,
            random: None,
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

    pub(crate) fn with_arrays(mut self, arrays: GlobalArrayViewMut<'a>) -> Self {
        self.arrays = Some(GlobalArrayExecutionAccess::Write(arrays));
        self
    }

    pub(crate) fn with_array_reader(mut self, arrays: GlobalArrayView<'a>) -> Self {
        self.arrays = Some(GlobalArrayExecutionAccess::Read(arrays));
        self
    }

    pub(crate) fn with_output(mut self, output: &'a mut dyn RuntimeOutput) -> Self {
        self.output = Some(output);
        self
    }

    pub(crate) fn with_input(mut self, input: &'a mut dyn RuntimeInput) -> Self {
        self.input = Some(input);
        self
    }

    pub(crate) fn with_random(mut self, random: &'a mut RandomState) -> Self {
        self.random = Some(random);
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

impl fmt::Debug for ExecutionView<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionView")
            .field("instructions", &self.instructions)
            .field("words", &self.words)
            .field("primitives", &self.primitives)
            .field("globals", &self.globals)
            .field("output", &self.output.is_some())
            .field("input", &self.input.is_some())
            .finish()
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

    fn read_array(&self, id: ArrayId, _index: i16) -> Result<Value, GlobalArrayError> {
        Err(GlobalArrayError::InvalidArrayId { id })
    }

    fn write_array(
        &mut self,
        id: ArrayId,
        _index: i16,
        _value: Value,
    ) -> Result<(), GlobalArrayError> {
        Err(GlobalArrayError::InvalidArrayId { id })
    }

    fn runtime_output(&mut self) -> Option<&mut (dyn RuntimeOutput + '_)> {
        None
    }

    fn runtime_input(&mut self) -> Option<&mut (dyn RuntimeInput + '_)> {
        None
    }

    fn take_runtime_capabilities(&mut self) -> crate::primitive::PrimitiveCapabilities<'a> {
        crate::primitive::PrimitiveCapabilities {
            output: None,
            input: None,
            random: None,
        }
    }

    fn restore_runtime_capabilities(
        &mut self,
        _capabilities: crate::primitive::PrimitiveCapabilities<'a>,
    ) {
    }
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

    fn read_array(&self, id: ArrayId, index: i16) -> Result<Value, GlobalArrayError> {
        match &self.arrays {
            Some(GlobalArrayExecutionAccess::Read(arrays)) => arrays.read_surface(id, index),
            Some(GlobalArrayExecutionAccess::Write(arrays)) => arrays.read_surface(id, index),
            None => Err(GlobalArrayError::InvalidArrayId { id }),
        }
    }

    fn write_array(
        &mut self,
        id: ArrayId,
        index: i16,
        value: Value,
    ) -> Result<(), GlobalArrayError> {
        match &mut self.arrays {
            Some(GlobalArrayExecutionAccess::Write(arrays)) => {
                arrays.write_surface(id, index, value)
            }
            Some(GlobalArrayExecutionAccess::Read(_)) | None => {
                Err(GlobalArrayError::InvalidArrayId { id })
            }
        }
    }

    fn runtime_output(&mut self) -> Option<&mut (dyn RuntimeOutput + '_)> {
        match self.output.as_mut() {
            Some(output) => Some(&mut **output),
            None => None,
        }
    }

    fn runtime_input(&mut self) -> Option<&mut (dyn RuntimeInput + '_)> {
        match self.input.as_mut() {
            Some(input) => Some(&mut **input),
            None => None,
        }
    }

    fn take_runtime_capabilities(&mut self) -> crate::primitive::PrimitiveCapabilities<'a> {
        crate::primitive::PrimitiveCapabilities {
            output: self.output.take(),
            input: self.input.take(),
            random: self.random.take(),
        }
    }

    fn restore_runtime_capabilities(
        &mut self,
        capabilities: crate::primitive::PrimitiveCapabilities<'a>,
    ) {
        self.output = capabilities.output;
        self.input = capabilities.input;
        self.random = capabilities.random;
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

    fn read_array(&self, id: ArrayId, index: i16) -> Result<Value, GlobalArrayError> {
        (**self).read_array(id, index)
    }

    fn write_array(
        &mut self,
        id: ArrayId,
        index: i16,
        value: Value,
    ) -> Result<(), GlobalArrayError> {
        (**self).write_array(id, index, value)
    }

    fn runtime_output(&mut self) -> Option<&mut (dyn RuntimeOutput + '_)> {
        (**self).runtime_output()
    }

    fn runtime_input(&mut self) -> Option<&mut (dyn RuntimeInput + '_)> {
        (**self).runtime_input()
    }

    fn take_runtime_capabilities(&mut self) -> crate::primitive::PrimitiveCapabilities<'a> {
        (**self).take_runtime_capabilities()
    }

    fn restore_runtime_capabilities(
        &mut self,
        capabilities: crate::primitive::PrimitiveCapabilities<'a>,
    ) {
        (**self).restore_runtime_capabilities(capabilities)
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
            control_value_stack: ControlValueStack::new(),
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
        let instruction = instructions
            .get_location(location)
            .cloned()
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InstructionFetch { source },
            })?;

        match instruction {
            Instruction::Push(value) => self.step_push(instructions, location, value),
            Instruction::WriteFixedText(text) => {
                self.step_write_fixed_text(&mut execution, location, &text)
            }
            Instruction::LoadVar(id) => self.step_load_var(&mut execution, location, id),
            Instruction::StoreVar(id) => self.step_store_var(&mut execution, location, id),
            Instruction::LoadArrayElement(id) => self.step_load_array(&mut execution, location, id),
            Instruction::StoreArrayElement(id) => {
                self.step_store_array(&mut execution, location, id)
            }
            Instruction::Call(id) => self.step_call(execution, location, id),
            Instruction::CopyFromCallBase { offset } => {
                self.step_copy_from_call_base(instructions, location, offset)
            }
            Instruction::TruncateDataStackToCallBase => {
                self.step_truncate_data_stack_to_call_base(instructions, location)
            }
            Instruction::PushControlValue => self.step_push_control_value(instructions, location),
            Instruction::CopyControlValue => self.step_copy_control_value(instructions, location),
            Instruction::DropControlValue => self.step_drop_control_value(instructions, location),
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
    fn control_value_stack_depth(&self) -> usize {
        self.control_value_stack.depth()
    }

    /// Returns the call-time data-stack depth of the innermost compiled word.
    ///
    /// The value belongs to VM control state and is intentionally not exposed
    /// through `Value` or `PrimitiveContext`.
    pub(crate) fn call_data_stack_depth(&self) -> Result<usize, StackError> {
        self.return_stack
            .peek()
            .map(ReturnFrame::call_data_stack_depth)
    }

    #[cfg(test)]
    fn call_control_value_stack_depth(&self) -> Result<usize, StackError> {
        self.return_stack
            .peek()
            .map(ReturnFrame::call_control_value_stack_depth)
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

    pub(crate) fn push_data(&mut self, value: Value) {
        self.data_stack.push(value);
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

    fn step_write_fixed_text<'a, E: VmExecutionView<'a>>(
        &mut self,
        execution: &mut E,
        location: CodeLocation,
        text: &str,
    ) -> Result<StepOutcome, VmError> {
        let instructions = execution.instructions();
        let next = self.valid_next_location(instructions, location)?;
        execution
            .runtime_output()
            .ok_or(crate::runtime_output::RuntimeOutputError::Unavailable)
            .and_then(|output| output.write(text))
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::FixedTextOutputFailed { source },
            })?;
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

    fn step_load_array<'a, E: VmExecutionView<'a>>(
        &mut self,
        execution: &mut E,
        location: CodeLocation,
        id: ArrayId,
    ) -> Result<StepOutcome, VmError> {
        self.data_stack.require_depth(1).map_err(|source| VmError {
            location,
            kind: VmErrorKind::DataStackUnderflow { source },
        })?;
        let index = self.data_stack.peek().expect("depth checked").as_integer();
        let instructions = execution.instructions();
        let next = self.valid_next_location(instructions, location)?;
        let value = execution.read_array(id, index).map_err(|source| VmError {
            location,
            kind: VmErrorKind::InvalidGlobalArray { source },
        })?;
        self.data_stack.replace_top(value).expect("depth checked");
        self.instruction_pointer = next;
        Ok(StepOutcome::Continued)
    }

    fn step_store_array<'a, E: VmExecutionView<'a>>(
        &mut self,
        execution: &mut E,
        location: CodeLocation,
        id: ArrayId,
    ) -> Result<StepOutcome, VmError> {
        self.data_stack.require_depth(2).map_err(|source| VmError {
            location,
            kind: VmErrorKind::DataStackUnderflow { source },
        })?;
        let (index, value) = self.data_stack.peek2().expect("depth checked");
        let instructions = execution.instructions();
        let next = self.valid_next_location(instructions, location)?;
        execution
            .write_array(id, index.as_integer(), value)
            .map_err(|source| VmError {
                location,
                kind: VmErrorKind::InvalidGlobalArray { source },
            })?;
        self.data_stack.pop2().expect("depth checked");
        self.instruction_pointer = next;
        Ok(StepOutcome::Continued)
    }

    fn step_copy_from_call_base(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
        offset: usize,
    ) -> Result<StepOutcome, VmError> {
        let next = self.valid_next_location(instructions, location)?;
        let call_data_stack_depth = self.call_data_stack_depth().map_err(|_| VmError {
            location,
            kind: VmErrorKind::CallBaseValueCopy {
                source: CallBaseValueError::NoCompiledWordInvocation,
            },
        })?;
        if offset == 0 || offset > call_data_stack_depth {
            return Err(VmError {
                location,
                kind: VmErrorKind::CallBaseValueCopy {
                    source: CallBaseValueError::InvalidOffset {
                        offset,
                        call_data_stack_depth,
                    },
                },
            });
        }

        let index = call_data_stack_depth - offset;
        let value = self.data_stack.value_at(index).map_err(|source| {
            let StackError::DataStackIndexOutOfBounds { index, depth } = source else {
                unreachable!("value_at only reports indexed data-stack errors")
            };
            VmError {
                location,
                kind: VmErrorKind::CallBaseValueCopy {
                    source: CallBaseValueError::DataStackIndexOutOfBounds { index, depth },
                },
            }
        })?;

        // The source is read from its current stack occupant. Only after all
        // validation succeeds do the copy and instruction-pointer transition
        // commit, preserving instruction failure atomicity.
        self.data_stack.push(value);
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_truncate_data_stack_to_call_base(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
    ) -> Result<StepOutcome, VmError> {
        let next = self.valid_next_location(instructions, location)?;
        let call_data_stack_depth = self.call_data_stack_depth().map_err(|_| VmError {
            location,
            kind: VmErrorKind::CallBaseDataStackTruncate {
                source: CallBaseDataStackTruncateError::NoCompiledWordInvocation,
            },
        })?;

        // This operation only discards values above the call base. It never
        // restores values that were popped after the call began.
        self.data_stack
            .truncate_to_depth(call_data_stack_depth)
            .map_err(|source| {
                let StackError::DataStackDepthBelowTarget { target, depth } = source else {
                    unreachable!("truncate_to_depth only reports a shallow data stack")
                };
                VmError {
                    location,
                    kind: VmErrorKind::CallBaseDataStackTruncate {
                        source: CallBaseDataStackTruncateError::CurrentDepthBelowCallBase {
                            current_depth: depth,
                            call_data_stack_depth: target,
                        },
                    },
                }
            })?;
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_push_control_value(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
    ) -> Result<StepOutcome, VmError> {
        self.data_stack.require_depth(1).map_err(|source| VmError {
            location,
            kind: VmErrorKind::DataStackUnderflow { source },
        })?;
        let next = self.valid_next_location(instructions, location)?;
        let value = self
            .data_stack
            .pop()
            .expect("data-stack depth was checked before PushControlValue");
        self.control_value_stack.push(value);
        self.instruction_pointer = next;
        Ok(StepOutcome::Continued)
    }

    fn step_copy_control_value(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
    ) -> Result<StepOutcome, VmError> {
        let value = self.control_value_stack.peek().map_err(|source| VmError {
            location,
            kind: VmErrorKind::ControlValueStackUnderflow { source },
        })?;
        let next = self.valid_next_location(instructions, location)?;
        self.data_stack.push(value);
        self.instruction_pointer = next;
        Ok(StepOutcome::Continued)
    }

    fn step_drop_control_value(
        &mut self,
        instructions: InstructionLookup<'_>,
        location: CodeLocation,
    ) -> Result<StepOutcome, VmError> {
        self.control_value_stack.peek().map_err(|source| VmError {
            location,
            kind: VmErrorKind::ControlValueStackUnderflow { source },
        })?;
        let next = self.valid_next_location(instructions, location)?;
        self.control_value_stack
            .pop()
            .expect("control-value depth was checked before DropControlValue");
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
        mut execution: E,
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

                let capabilities = execution.take_runtime_capabilities();
                let mut context = PrimitiveContext::with_capabilities(
                    &mut self.data_stack,
                    capabilities.output,
                    capabilities.input,
                    capabilities.random,
                );
                let result = handler(&mut context);
                let capabilities = context.into_capabilities();
                execution.restore_runtime_capabilities(capabilities);
                match result {
                    Ok(()) => {
                        self.instruction_pointer = next;
                        Ok(StepOutcome::Continued)
                    }
                    Err(source) => {
                        // ADR #1720 assigns stack failure atomicity to each
                        // primitive's validate-then-commit implementation.
                        Err(VmError {
                            location,
                            kind: VmErrorKind::PrimitiveFailed { primitive, source },
                        })
                    }
                }
            }
            WordDefinition::Compiled { entry } => {
                let entry = self.valid_compiled_entry(instructions, location, entry)?;

                self.return_stack
                    .push(ReturnFrame::with_control_value_stack_depth(
                        next,
                        self.data_stack.depth(),
                        self.control_value_stack.depth(),
                    ));
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
        let call_depth = frame.call_control_value_stack_depth();
        let current_depth = self.control_value_stack.depth();
        if current_depth < call_depth {
            return Err(VmError {
                location,
                kind: VmErrorKind::ControlValueStackDepthBelowCall {
                    call_depth,
                    current_depth,
                },
            });
        }

        // ADR #1936 makes Return restore only callee-owned control values. Validate
        // the target and saved depth before changing the stack, frame, or IP.
        self.control_value_stack
            .truncate_to_depth(call_depth)
            .expect("control-value depth was checked before Return truncation");
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
mod tests;
