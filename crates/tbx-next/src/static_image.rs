use crate::global_array::{ArrayId, GlobalArrays};
use crate::global_variable::{GlobalVarId, GlobalVariables};
use crate::instruction::{CodeLocation, Instruction, InstructionAddress, InstructionView};
use crate::operator::{OperatorSemantic, OperatorWords};
use crate::word::{PrimitiveId, PublishedWords, WordDefinition, WordId};
use std::collections::{HashMap, HashSet};

mod reference_vm;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CodePosition(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlobalSlot(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ArraySlot(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextSlot(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimitiveOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Negate,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Not,
    Abs,
    Dup,
    Drop,
    Swap,
    PutDec,
    PutChr,
    Cr,
    InputQuestion,
    Rnd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LogicalInstruction {
    PushI16(i16),
    WriteText(TextSlot),
    LoadGlobal(GlobalSlot),
    StoreGlobal(GlobalSlot),
    LoadArray(ArraySlot),
    StoreArray(ArraySlot),
    CallPrimitive(PrimitiveOp),
    CallCode(CodePosition),
    CopyCallBase(usize),
    TruncateCallBase,
    ControlPush,
    ControlCopy,
    ControlDrop,
    Jump(CodePosition),
    JumpIfZero(CodePosition),
    Return,
    Halt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticImage {
    code: Vec<LogicalInstruction>,
    texts: Vec<String>,
    global_count: usize,
    array_lengths: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LowerError {
    InvalidWord(WordId),
    InvalidCodeLocation(CodeLocation),
    UnknownCodeOwner(CodeLocation),
    InvalidGlobal(GlobalVarId),
    InvalidArray(ArrayId),
    ScratchInstructionUnsupported,
    UnknownPrimitive(PrimitiveId),
    ImageTooLarge,
    DuplicateCodeOwner,
}

struct PrimitiveMap(HashMap<PrimitiveId, PrimitiveOp>);

#[derive(Clone, Copy)]
struct PrimitiveWordIds {
    operators: OperatorWords,
    abs: WordId,
    stack: [WordId; 3],
    output: [WordId; 3],
    input: WordId,
    rnd: WordId,
}

impl PrimitiveMap {
    fn from_word_ids(
        words: &PublishedWords,
        ids: [(WordId, PrimitiveOp); 24],
    ) -> Result<Self, LowerError> {
        let mut map = HashMap::with_capacity(ids.len());
        for (word, operation) in ids {
            let primitive = match words.get(word).map_err(|_| LowerError::InvalidWord(word))? {
                WordDefinition::Primitive { primitive } => *primitive,
                WordDefinition::Compiled { .. } => return Err(LowerError::InvalidWord(word)),
            };
            map.insert(primitive, operation);
        }
        Ok(Self(map))
    }
}

fn lower(
    owners: &[InstructionView<'_>],
    words: &PublishedWords,
    primitive_words: PrimitiveWordIds,
    globals: &GlobalVariables,
    arrays: &GlobalArrays,
) -> Result<StaticImage, LowerError> {
    let op_lookup = primitive_words.operators.lookup();
    let primitive_map = PrimitiveMap::from_word_ids(
        words,
        [
            (op_lookup.resolve(OperatorSemantic::Add), PrimitiveOp::Add),
            (
                op_lookup.resolve(OperatorSemantic::Subtract),
                PrimitiveOp::Subtract,
            ),
            (
                op_lookup.resolve(OperatorSemantic::Multiply),
                PrimitiveOp::Multiply,
            ),
            (
                op_lookup.resolve(OperatorSemantic::Divide),
                PrimitiveOp::Divide,
            ),
            (
                op_lookup.resolve(OperatorSemantic::Remainder),
                PrimitiveOp::Remainder,
            ),
            (
                op_lookup.resolve(OperatorSemantic::Negate),
                PrimitiveOp::Negate,
            ),
            (
                op_lookup.resolve(OperatorSemantic::Equal),
                PrimitiveOp::Equal,
            ),
            (
                op_lookup.resolve(OperatorSemantic::NotEqual),
                PrimitiveOp::NotEqual,
            ),
            (op_lookup.resolve(OperatorSemantic::Less), PrimitiveOp::Less),
            (
                op_lookup.resolve(OperatorSemantic::LessEqual),
                PrimitiveOp::LessEqual,
            ),
            (
                op_lookup.resolve(OperatorSemantic::Greater),
                PrimitiveOp::Greater,
            ),
            (
                op_lookup.resolve(OperatorSemantic::GreaterEqual),
                PrimitiveOp::GreaterEqual,
            ),
            (op_lookup.resolve(OperatorSemantic::And), PrimitiveOp::And),
            (op_lookup.resolve(OperatorSemantic::Or), PrimitiveOp::Or),
            (op_lookup.resolve(OperatorSemantic::Not), PrimitiveOp::Not),
            (primitive_words.abs, PrimitiveOp::Abs),
            (primitive_words.stack[0], PrimitiveOp::Dup),
            (primitive_words.stack[1], PrimitiveOp::Drop),
            (primitive_words.stack[2], PrimitiveOp::Swap),
            (primitive_words.output[0], PrimitiveOp::PutDec),
            (primitive_words.output[1], PrimitiveOp::PutChr),
            (primitive_words.output[2], PrimitiveOp::Cr),
            (primitive_words.input, PrimitiveOp::InputQuestion),
            (primitive_words.rnd, PrimitiveOp::Rnd),
        ],
    )?;
    let mut positions = HashMap::new();
    let mut code_spaces = HashSet::with_capacity(owners.len());
    let mut total = 0usize;
    for owner in owners {
        if !code_spaces.insert(owner.code_space()) {
            return Err(LowerError::DuplicateCodeOwner);
        }
        for index in 0..owner.len() {
            positions.insert((owner.code_space(), index), CodePosition(total + index));
        }
        total = total
            .checked_add(owner.len())
            .ok_or(LowerError::ImageTooLarge)?;
    }

    let mut code = Vec::with_capacity(total);
    let mut texts = Vec::new();
    let mut globals_map = HashMap::new();
    let mut arrays_map = HashMap::new();
    let mut array_lengths = Vec::new();
    for owner in owners {
        for index in 0..owner.len() {
            let instruction = owner
                .get(InstructionAddress::from_index(index))
                .map_err(|_| {
                    LowerError::InvalidCodeLocation(
                        owner.location(InstructionAddress::from_index(index)),
                    )
                })?;
            let pos = |address: InstructionAddress| {
                positions
                    .get(&(owner.code_space(), address.as_index()))
                    .copied()
                    .ok_or(LowerError::InvalidCodeLocation(owner.location(address)))
            };
            let logical = match instruction {
                Instruction::Push(value) => LogicalInstruction::PushI16(value.as_integer()),
                Instruction::WriteFixedText(text) => {
                    let slot = TextSlot(texts.len());
                    texts.push(text.to_string());
                    LogicalInstruction::WriteText(slot)
                }
                Instruction::LoadVar(id) | Instruction::StoreVar(id) => {
                    globals
                        .view()
                        .read(*id)
                        .map_err(|_| LowerError::InvalidGlobal(*id))?;
                    let slot = if let Some(slot) = globals_map.get(id) {
                        *slot
                    } else {
                        let slot = GlobalSlot(globals_map.len());
                        globals_map.insert(*id, slot);
                        slot
                    };
                    if matches!(instruction, Instruction::LoadVar(_)) {
                        LogicalInstruction::LoadGlobal(slot)
                    } else {
                        LogicalInstruction::StoreGlobal(slot)
                    }
                }
                Instruction::LoadArrayElement(id) | Instruction::StoreArrayElement(id) => {
                    let len = arrays.len_of(*id).ok_or(LowerError::InvalidArray(*id))?;
                    let slot = *arrays_map.entry(*id).or_insert_with(|| {
                        let slot = ArraySlot(array_lengths.len());
                        array_lengths.push(len);
                        slot
                    });
                    if matches!(instruction, Instruction::LoadArrayElement(_)) {
                        LogicalInstruction::LoadArray(slot)
                    } else {
                        LogicalInstruction::StoreArray(slot)
                    }
                }
                Instruction::LoadScratch(_) | Instruction::StoreScratch(_) => {
                    return Err(LowerError::ScratchInstructionUnsupported);
                }
                Instruction::Call(word) => match words
                    .get(*word)
                    .map_err(|_| LowerError::InvalidWord(*word))?
                {
                    WordDefinition::Primitive { primitive } => LogicalInstruction::CallPrimitive(
                        *primitive_map
                            .0
                            .get(primitive)
                            .ok_or(LowerError::UnknownPrimitive(*primitive))?,
                    ),
                    WordDefinition::Compiled { entry } => {
                        let owner = owners
                            .iter()
                            .find(|owner| owner.code_space() == entry.code_space())
                            .ok_or(LowerError::UnknownCodeOwner(*entry))?;
                        owner
                            .validate_location(*entry)
                            .map_err(|_| LowerError::InvalidCodeLocation(*entry))?;
                        LogicalInstruction::CallCode(
                            *positions
                                .get(&(entry.code_space(), entry.address().as_index()))
                                .ok_or(LowerError::InvalidCodeLocation(*entry))?,
                        )
                    }
                },
                Instruction::CopyFromCallBase { offset } => {
                    LogicalInstruction::CopyCallBase(*offset)
                }
                Instruction::TruncateDataStackToCallBase => LogicalInstruction::TruncateCallBase,
                Instruction::PushControlValue => LogicalInstruction::ControlPush,
                Instruction::CopyControlValue => LogicalInstruction::ControlCopy,
                Instruction::DropControlValue => LogicalInstruction::ControlDrop,
                Instruction::Jump(address) => LogicalInstruction::Jump(pos(*address)?),
                Instruction::JumpIfZero(address) => LogicalInstruction::JumpIfZero(pos(*address)?),
                Instruction::Return => LogicalInstruction::Return,
                Instruction::Halt => LogicalInstruction::Halt,
            };
            code.push(logical);
        }
    }
    Ok(StaticImage {
        code,
        texts,
        global_count: globals_map.len(),
        array_lengths,
    })
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogicalInstructionKind {
    PushI16,
    WriteText,
    LoadGlobal,
    StoreGlobal,
    LoadArray,
    StoreArray,
    CallPrimitive,
    CallCode,
    CopyCallBase,
    TruncateCallBase,
    ControlPush,
    ControlCopy,
    ControlDrop,
    Jump,
    JumpIfZero,
    Return,
    Halt,
}

#[cfg(test)]
impl LogicalInstructionKind {
    const ALL: [Self; 17] = [
        Self::PushI16,
        Self::WriteText,
        Self::LoadGlobal,
        Self::StoreGlobal,
        Self::LoadArray,
        Self::StoreArray,
        Self::CallPrimitive,
        Self::CallCode,
        Self::CopyCallBase,
        Self::TruncateCallBase,
        Self::ControlPush,
        Self::ControlCopy,
        Self::ControlDrop,
        Self::Jump,
        Self::JumpIfZero,
        Self::Return,
        Self::Halt,
    ];
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestImageStatistics {
    pub(crate) instruction_count: usize,
    pub(crate) variant_counts: Vec<(LogicalInstructionKind, usize)>,
    pub(crate) relocation_count: usize,
    pub(crate) fixed_text_bytes: usize,
    pub(crate) fixed_text_count: usize,
    pub(crate) global_count: usize,
    pub(crate) array_lengths: Vec<usize>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestReferenceResult {
    pub(crate) halted: bool,
    pub(crate) data_stack: Vec<i16>,
    pub(crate) statistics: TestImageStatistics,
}

#[cfg(test)]
pub(crate) fn test_lower_and_run<W: std::io::Write>(
    owners: &[InstructionView<'_>],
    words: &PublishedWords,
    primitive_words: (
        OperatorWords,
        WordId,
        [WordId; 3],
        [WordId; 3],
        WordId,
        WordId,
    ),
    globals: &GlobalVariables,
    arrays: &GlobalArrays,
    writer: &mut W,
) -> Result<TestReferenceResult, ()> {
    let image = lower(
        owners,
        words,
        PrimitiveWordIds {
            operators: primitive_words.0,
            abs: primitive_words.1,
            stack: primitive_words.2,
            output: primitive_words.3,
            input: primitive_words.4,
            rnd: primitive_words.5,
        },
        globals,
        arrays,
    )
    .map_err(|_| ())?;
    let statistics = image.statistics();
    let mut vm = reference_vm::ReferenceVm::new(image, CodePosition(0))
        .expect("lowered temporary unit has a valid entry");
    let mut output = crate::runtime_output::WriteRuntimeOutput::new(writer);
    let outcome = vm
        .run(Some(&mut output), None, None)
        .expect("test source executes");
    Ok(TestReferenceResult {
        halted: outcome == reference_vm::RunOutcome::Halted,
        data_stack: vm.data_stack().to_vec(),
        statistics,
    })
}

#[cfg(test)]
impl StaticImage {
    fn statistics(&self) -> TestImageStatistics {
        let mut counts = [0; LogicalInstructionKind::ALL.len()];
        let mut relocation_count = 0;
        for instruction in &self.code {
            let kind = match instruction {
                LogicalInstruction::PushI16(_) => LogicalInstructionKind::PushI16,
                LogicalInstruction::WriteText(_) => LogicalInstructionKind::WriteText,
                LogicalInstruction::LoadGlobal(_) => LogicalInstructionKind::LoadGlobal,
                LogicalInstruction::StoreGlobal(_) => LogicalInstructionKind::StoreGlobal,
                LogicalInstruction::LoadArray(_) => LogicalInstructionKind::LoadArray,
                LogicalInstruction::StoreArray(_) => LogicalInstructionKind::StoreArray,
                LogicalInstruction::CallPrimitive(_) => LogicalInstructionKind::CallPrimitive,
                LogicalInstruction::CallCode(_) => {
                    relocation_count += 1;
                    LogicalInstructionKind::CallCode
                }
                LogicalInstruction::CopyCallBase(_) => LogicalInstructionKind::CopyCallBase,
                LogicalInstruction::TruncateCallBase => LogicalInstructionKind::TruncateCallBase,
                LogicalInstruction::ControlPush => LogicalInstructionKind::ControlPush,
                LogicalInstruction::ControlCopy => LogicalInstructionKind::ControlCopy,
                LogicalInstruction::ControlDrop => LogicalInstructionKind::ControlDrop,
                LogicalInstruction::Jump(_) => {
                    relocation_count += 1;
                    LogicalInstructionKind::Jump
                }
                LogicalInstruction::JumpIfZero(_) => {
                    relocation_count += 1;
                    LogicalInstructionKind::JumpIfZero
                }
                LogicalInstruction::Return => LogicalInstructionKind::Return,
                LogicalInstruction::Halt => LogicalInstructionKind::Halt,
            };
            let index = LogicalInstructionKind::ALL
                .iter()
                .position(|candidate| *candidate == kind)
                .expect("every instruction kind is listed");
            counts[index] += 1;
        }
        TestImageStatistics {
            instruction_count: self.code.len(),
            variant_counts: LogicalInstructionKind::ALL
                .into_iter()
                .zip(counts)
                .collect(),
            relocation_count,
            fixed_text_bytes: self.texts.iter().map(String::len).sum(),
            fixed_text_count: self.texts.len(),
            global_count: self.global_count,
            array_lengths: self.array_lengths.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arithmetic_primitive::register_arithmetic_primitives;
    use crate::binding::{Binding, Bindings};
    use crate::global_array::GlobalArrays;
    use crate::global_variable::GlobalVariables;
    use crate::input_primitive::register_input_primitives;
    use crate::instruction::InstructionSequence;
    use crate::name::NormalizedName;
    use crate::operator::register_named_operator_primitives;
    use crate::output_primitive::register_output_primitives;
    use crate::primitive::PrimitiveRegistry;
    use crate::random_primitive::register_random_primitives;
    use crate::stack_primitive::register_stack_primitives;
    use crate::value::Value;
    use crate::word::{CompletedWordDefinition, PrimitiveId};
    use std::rc::Rc;

    #[test]
    fn static_image_statistics_count_variants_relocations_and_text_bytes() {
        let image = StaticImage {
            code: vec![
                LogicalInstruction::PushI16(1),
                LogicalInstruction::WriteText(TextSlot(0)),
                LogicalInstruction::LoadGlobal(GlobalSlot(0)),
                LogicalInstruction::StoreGlobal(GlobalSlot(0)),
                LogicalInstruction::LoadArray(ArraySlot(0)),
                LogicalInstruction::StoreArray(ArraySlot(0)),
                LogicalInstruction::CallPrimitive(PrimitiveOp::Add),
                LogicalInstruction::CallCode(CodePosition(0)),
                LogicalInstruction::CopyCallBase(1),
                LogicalInstruction::TruncateCallBase,
                LogicalInstruction::ControlPush,
                LogicalInstruction::ControlCopy,
                LogicalInstruction::ControlDrop,
                LogicalInstruction::Jump(CodePosition(0)),
                LogicalInstruction::JumpIfZero(CodePosition(0)),
                LogicalInstruction::Return,
                LogicalInstruction::Halt,
            ],
            texts: vec!["é".to_owned()],
            global_count: 1,
            array_lengths: vec![3],
        };

        let statistics = image.statistics();
        assert_eq!(statistics.instruction_count, 17);
        assert_eq!(
            statistics.variant_counts,
            LogicalInstructionKind::ALL.map(|kind| (kind, 1))
        );
        assert_eq!(statistics.relocation_count, 3);
        assert_eq!(statistics.fixed_text_bytes, 2);
        assert_eq!(statistics.fixed_text_count, 1);
        assert_eq!(statistics.global_count, 1);
        assert_eq!(statistics.array_lengths, [3]);
    }

    struct Fixture {
        bindings: Bindings,
        primitives: PrimitiveRegistry,
        words: PublishedWords,
        operators: OperatorWords,
        abs: WordId,
        stack: [WordId; 3],
        output: [WordId; 3],
        input: WordId,
        rnd: WordId,
        globals: GlobalVariables,
        arrays: GlobalArrays,
    }

    impl Fixture {
        fn new() -> Self {
            let mut bindings = Bindings::new();
            let mut primitives = PrimitiveRegistry::new();
            let mut words = PublishedWords::new();
            let operators =
                register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                    .expect("operators should register");
            let abs = register_arithmetic_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("ABS should register")
                .abs();
            let stack = register_stack_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("stack primitives should register");
            let output = register_output_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("output primitives should register");
            let input = register_input_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("input primitive should register");
            let rnd = register_random_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("random primitive should register");
            Self {
                bindings,
                primitives,
                words,
                operators,
                abs,
                stack: [stack.dup(), stack.drop(), stack.swap()],
                output: [output.putdec(), output.putchr(), output.cr()],
                input: input.input_question(),
                rnd: rnd.rnd(),
                globals: GlobalVariables::new(),
                arrays: GlobalArrays::new(),
            }
        }

        fn lower(&self, owners: &[InstructionView<'_>]) -> Result<StaticImage, LowerError> {
            let primitive_words = PrimitiveWordIds {
                operators: self.operators,
                abs: self.abs,
                stack: self.stack,
                output: self.output,
                input: self.input,
                rnd: self.rnd,
            };
            lower(
                owners,
                &self.words,
                primitive_words,
                &self.globals,
                &self.arrays,
            )
        }
    }

    #[test]
    fn lowers_every_instruction_family_and_resolves_identities() {
        let mut fixture = Fixture::new();
        let global_a = fixture.globals.allocate();
        let global_b = fixture.globals.allocate();
        let array_a = fixture.arrays.allocate(3);
        let array_b = fixture.arrays.allocate(1);
        let mut code = InstructionSequence::new();
        let forward = code.append(Instruction::Jump(InstructionAddress::from_index(0)));
        code.append(Instruction::Push(Value::integer(-12)));
        code.append(Instruction::WriteFixedText(Rc::from("hello")));
        code.append(Instruction::LoadVar(global_a));
        code.append(Instruction::StoreVar(global_b));
        code.append(Instruction::LoadArrayElement(array_a));
        code.append(Instruction::StoreArrayElement(array_b));
        let op = fixture.operators.lookup().resolve(OperatorSemantic::Add);
        code.append(Instruction::Call(op));
        code.append(Instruction::CopyFromCallBase { offset: 2 });
        code.append(Instruction::TruncateDataStackToCallBase);
        code.append(Instruction::PushControlValue);
        code.append(Instruction::CopyControlValue);
        code.append(Instruction::DropControlValue);
        let backward_target = InstructionAddress::from_index(1);
        code.append(Instruction::JumpIfZero(backward_target));
        code.append(Instruction::Return);
        code.append(Instruction::Halt);
        let last = code.len() - 1;
        code.patch_branch_target(forward, InstructionAddress::from_index(last))
            .expect("forward branch should patch");

        let image = fixture
            .lower(&[code.view()])
            .expect("valid graph should lower");
        assert_eq!(image.code.len(), code.len());
        assert_eq!(image.code[0], LogicalInstruction::Jump(CodePosition(last)));
        assert_eq!(image.code[1], LogicalInstruction::PushI16(-12));
        assert_eq!(image.texts, ["hello"]);
        assert_eq!(image.code[2], LogicalInstruction::WriteText(TextSlot(0)));
        assert_eq!(image.code[3], LogicalInstruction::LoadGlobal(GlobalSlot(0)));
        assert_eq!(
            image.code[4],
            LogicalInstruction::StoreGlobal(GlobalSlot(1))
        );
        assert_eq!(image.code[5], LogicalInstruction::LoadArray(ArraySlot(0)));
        assert_eq!(image.code[6], LogicalInstruction::StoreArray(ArraySlot(1)));
        assert_eq!(
            image.code[7],
            LogicalInstruction::CallPrimitive(PrimitiveOp::Add)
        );
        assert_eq!(image.code[8], LogicalInstruction::CopyCallBase(2));
        assert_eq!(image.code[9], LogicalInstruction::TruncateCallBase);
        assert_eq!(image.code[10], LogicalInstruction::ControlPush);
        assert_eq!(image.code[11], LogicalInstruction::ControlCopy);
        assert_eq!(image.code[12], LogicalInstruction::ControlDrop);
        assert_eq!(
            image.code[13],
            LogicalInstruction::JumpIfZero(CodePosition(1))
        );
        assert_eq!(image.code[14], LogicalInstruction::Return);
        assert_eq!(image.code[15], LogicalInstruction::Halt);
        assert_eq!(image.global_count, 2);
        assert_eq!(image.array_lengths, [3, 1]);
    }

    #[test]
    fn lowered_static_image_is_accepted_by_reference_vm() {
        let fixture = Fixture::new();
        let mut code = InstructionSequence::new();
        code.append(Instruction::Push(Value::integer(i16::MIN)));
        code.append(Instruction::Halt);

        let image = fixture.lower(&[code.view()]).expect("program should lower");
        let mut vm = reference_vm::ReferenceVm::new(image, CodePosition(0))
            .expect("lowered image should be executable");

        assert_eq!(
            vm.run(None, None, None),
            Ok(reference_vm::RunOutcome::Halted)
        );
        assert!(vm.is_halted());
    }

    #[test]
    fn compiled_calls_keep_early_bound_entry_and_owner_local_positions() {
        let mut fixture = Fixture::new();
        let mut old_owner = InstructionSequence::new();
        old_owner.append(Instruction::Push(Value::integer(7)));
        let old_entry = old_owner.append(Instruction::Return);
        let old_id = fixture.words.add(
            CompletedWordDefinition::compiled(
                old_owner.view().location(old_entry),
                old_owner.view(),
            )
            .expect("old entry is valid"),
        );
        let mut new_owner = InstructionSequence::new();
        new_owner.append(Instruction::Push(Value::integer(9)));
        let new_entry = new_owner.append(Instruction::Halt);
        let new_id = fixture.words.add(
            CompletedWordDefinition::compiled(
                new_owner.view().location(new_entry),
                new_owner.view(),
            )
            .expect("new entry is valid"),
        );
        let mut caller = InstructionSequence::new();
        caller.append(Instruction::Call(old_id));
        caller.append(Instruction::Call(new_id));

        let image = fixture
            .lower(&[caller.view(), old_owner.view(), new_owner.view()])
            .expect("all owners should be relocated");
        assert_eq!(image.code[0], LogicalInstruction::CallCode(CodePosition(3)));
        assert_eq!(image.code[1], LogicalInstruction::CallCode(CodePosition(5)));
        assert_eq!(image.code[2], LogicalInstruction::PushI16(7));
        assert_eq!(image.code[3], LogicalInstruction::Return);
        assert_eq!(image.code[4], LogicalInstruction::PushI16(9));
        assert_eq!(image.code[5], LogicalInstruction::Halt);
    }

    #[test]
    fn existing_call_keeps_old_entry_after_same_name_is_redefined() {
        let mut fixture = Fixture::new();
        let name = NormalizedName::new("FOO").expect("test name is valid");
        let mut owner = InstructionSequence::new();
        let old_entry = owner.append(Instruction::Push(Value::integer(1)));
        let old_id = fixture.words.add(
            CompletedWordDefinition::compiled(owner.view().location(old_entry), owner.view())
                .expect("old entry is valid"),
        );
        fixture
            .bindings
            .insert_new(name.clone(), Binding::Word(old_id))
            .expect("initial word binding should publish");
        let mut caller = InstructionSequence::new();
        caller.append(Instruction::Call(old_id));

        let new_entry = owner.append(Instruction::Push(Value::integer(2)));
        let replacement =
            CompletedWordDefinition::compiled(owner.view().location(new_entry), owner.view())
                .expect("replacement entry is valid");
        let changed = crate::redefinition::redefine_word(
            &mut fixture.words,
            &mut fixture.bindings,
            &name,
            replacement,
        )
        .expect("same-name word should be redefined");
        assert_eq!(changed.previous(), old_id);
        assert_eq!(fixture.bindings.current_word(&name), Ok(changed.current()));

        let image = fixture
            .lower(&[caller.view(), owner.view()])
            .expect("early-bound call should lower");
        assert_eq!(image.code[0], LogicalInstruction::CallCode(CodePosition(1)));
    }

    #[test]
    fn resolves_all_registered_primitive_words_without_using_slots() {
        let fixture = Fixture::new();
        let op = fixture.operators.lookup();
        let words = [
            (op.resolve(OperatorSemantic::Add), PrimitiveOp::Add),
            (
                op.resolve(OperatorSemantic::Subtract),
                PrimitiveOp::Subtract,
            ),
            (
                op.resolve(OperatorSemantic::Multiply),
                PrimitiveOp::Multiply,
            ),
            (op.resolve(OperatorSemantic::Divide), PrimitiveOp::Divide),
            (
                op.resolve(OperatorSemantic::Remainder),
                PrimitiveOp::Remainder,
            ),
            (op.resolve(OperatorSemantic::Negate), PrimitiveOp::Negate),
            (op.resolve(OperatorSemantic::Equal), PrimitiveOp::Equal),
            (
                op.resolve(OperatorSemantic::NotEqual),
                PrimitiveOp::NotEqual,
            ),
            (op.resolve(OperatorSemantic::Less), PrimitiveOp::Less),
            (
                op.resolve(OperatorSemantic::LessEqual),
                PrimitiveOp::LessEqual,
            ),
            (op.resolve(OperatorSemantic::Greater), PrimitiveOp::Greater),
            (
                op.resolve(OperatorSemantic::GreaterEqual),
                PrimitiveOp::GreaterEqual,
            ),
            (op.resolve(OperatorSemantic::And), PrimitiveOp::And),
            (op.resolve(OperatorSemantic::Or), PrimitiveOp::Or),
            (op.resolve(OperatorSemantic::Not), PrimitiveOp::Not),
            (fixture.abs, PrimitiveOp::Abs),
            (fixture.stack[0], PrimitiveOp::Dup),
            (fixture.stack[1], PrimitiveOp::Drop),
            (fixture.stack[2], PrimitiveOp::Swap),
            (fixture.output[0], PrimitiveOp::PutDec),
            (fixture.output[1], PrimitiveOp::PutChr),
            (fixture.output[2], PrimitiveOp::Cr),
            (fixture.input, PrimitiveOp::InputQuestion),
            (fixture.rnd, PrimitiveOp::Rnd),
        ];
        let mut code = InstructionSequence::new();
        for (id, _) in words {
            code.append(Instruction::Call(id));
        }
        let image = fixture
            .lower(&[code.view()])
            .expect("all bootstrap primitives lower");
        let expected: Vec<_> = words
            .into_iter()
            .map(|(_, operation)| LogicalInstruction::CallPrimitive(operation))
            .collect();
        assert_eq!(image.code, expected);
    }

    #[test]
    fn invalid_and_unknown_references_fail_without_an_image() {
        let fixture = Fixture::new();
        let mut duplicate_owner = InstructionSequence::new();
        duplicate_owner.append(Instruction::Halt);
        assert!(matches!(
            fixture.lower(&[duplicate_owner.view(), duplicate_owner.view()]),
            Err(LowerError::DuplicateCodeOwner)
        ));

        let mut bad_word = InstructionSequence::new();
        bad_word.append(Instruction::Call(WordId::test_invalid(usize::MAX)));
        assert!(matches!(
            fixture.lower(&[bad_word.view()]),
            Err(LowerError::InvalidWord(_))
        ));

        let mut bad_branch = InstructionSequence::new();
        bad_branch.append(Instruction::Jump(InstructionAddress::from_index(1)));
        let mut following_owner = InstructionSequence::new();
        following_owner.append(Instruction::Halt);
        assert!(matches!(
            fixture.lower(&[bad_branch.view(), following_owner.view()]),
            Err(LowerError::InvalidCodeLocation(_))
        ));

        let mut unknown_primitive = Fixture::new();
        let id = unknown_primitive
            .words
            .add(CompletedWordDefinition::primitive(PrimitiveId::from_slot(
                999,
            )));
        let mut code = InstructionSequence::new();
        code.append(Instruction::Call(id));
        assert!(matches!(
            unknown_primitive.lower(&[code.view()]),
            Err(LowerError::UnknownPrimitive(_))
        ));

        let mut bad_global = InstructionSequence::new();
        bad_global.append(Instruction::LoadVar(GlobalVarId::test_invalid(4)));
        assert!(matches!(
            fixture.lower(&[bad_global.view()]),
            Err(LowerError::InvalidGlobal(_))
        ));

        let mut bad_array = InstructionSequence::new();
        bad_array.append(Instruction::StoreArrayElement(ArrayId::test_invalid(4)));
        assert!(matches!(
            fixture.lower(&[bad_array.view()]),
            Err(LowerError::InvalidArray(_))
        ));
    }
}
