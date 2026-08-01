use crate::instruction::{InstructionAddressError, InstructionView};

pub(crate) use crate::instruction::InstructionAddress;

/// Internal identifier for a published executable word definition.
///
/// ADR #1368 requires old definitions to remain executable after redefinition,
/// so IDs are allocated monotonically, never reused, and never exposed as
/// runtime `Value`s. This type identifies an entry in `PublishedWords` without
/// exposing the backing collection layout as a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WordId {
    slot: usize,
}

impl WordId {
    #[cfg(test)]
    const fn test_invalid(slot: usize) -> Self {
        Self { slot }
    }
}

/// Minimal primitive implementation identity.
///
/// The final primitive handler signature belongs to the VM call implementation.
/// This ID is only enough for the published word collection to distinguish and
/// retain primitive definitions without adding primitive-specific instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PrimitiveId {
    slot: usize,
}

impl PrimitiveId {
    pub(crate) const fn from_slot(slot: usize) -> Self {
        Self { slot }
    }

    pub(crate) const fn as_slot(self) -> usize {
        self.slot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordDefinition {
    Primitive { primitive: PrimitiveId },
    Compiled { entry: InstructionAddress },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordLookupError {
    InvalidWordId { id: WordId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordDefinitionError {
    InvalidCompiledEntry { error: InstructionAddressError },
}

/// Monotonic collection of published, executable word definitions.
///
/// This collection deliberately has no removal, replacement, arbitrary insert,
/// or mutable-definition access API. ADR #1368 keeps name binding and VM state
/// outside this responsibility: callers may add a new published definition and
/// later look it up by `WordId`, but they cannot rewrite older IDs or expose
/// unpublished definitions through normal lookup.
///
/// ADR #1369 makes `WordId` issuance part of the construction-to-publication
/// boundary: callers must pass only completed, executable definitions to `add`.
/// In-progress code, failed builds, and unreferenced instruction fragments must
/// remain outside this collection so ordinary `Call(WordId)` cannot reach them.
#[derive(Debug, Default)]
pub(crate) struct PublishedWords {
    definitions: Vec<WordDefinition>,
}

impl PublishedWords {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&mut self, definition: WordDefinition) -> WordId {
        let id = WordId {
            slot: self.definitions.len(),
        };
        self.definitions.push(definition);
        id
    }

    pub(crate) fn add_validated(
        &mut self,
        definition: WordDefinition,
        instructions: InstructionView<'_>,
    ) -> Result<WordId, WordDefinitionError> {
        validate_definition(definition, instructions)?;
        Ok(self.add(definition))
    }

    pub(crate) fn get(&self, id: WordId) -> Result<&WordDefinition, WordLookupError> {
        self.definitions
            .get(id.slot)
            .ok_or(WordLookupError::InvalidWordId { id })
    }

    pub(crate) fn len(&self) -> usize {
        self.definitions.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

fn validate_definition(
    definition: WordDefinition,
    instructions: InstructionView<'_>,
) -> Result<(), WordDefinitionError> {
    match definition {
        WordDefinition::Primitive { .. } => Ok(()),
        WordDefinition::Compiled { entry } => instructions
            .validate_address(entry)
            .map(|_| ())
            .map_err(|error| WordDefinitionError::InvalidCompiledEntry { error }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{Instruction, InstructionAddressError, InstructionSequence};
    use crate::value::Value;
    use crate::word_lookup::PublishedWordLookup;

    fn primitive(slot: usize) -> WordDefinition {
        WordDefinition::Primitive {
            primitive: PrimitiveId::from_slot(slot),
        }
    }

    fn compiled(index: usize) -> WordDefinition {
        WordDefinition::Compiled {
            entry: InstructionAddress::from_index(index),
        }
    }

    fn compiled_entry(entry: InstructionAddress) -> WordDefinition {
        WordDefinition::Compiled { entry }
    }

    #[test]
    fn empty_collection_accepts_first_definition() {
        let mut words = PublishedWords::new();

        let id = words.add(primitive(0));

        assert_eq!(words.len(), 1);
        assert!(!words.is_empty());
        assert_eq!(words.get(id), Ok(&primitive(0)));
    }

    #[test]
    fn added_definitions_receive_distinct_ids_and_keep_identity() {
        let mut words = PublishedWords::new();

        let first_id = words.add(primitive(7));
        let second_id = words.add(compiled(12));
        let third_id = words.add(primitive(9));

        assert_ne!(first_id, second_id);
        assert_ne!(first_id, third_id);
        assert_ne!(second_id, third_id);
        assert_eq!(words.get(first_id), Ok(&primitive(7)));
        assert_eq!(words.get(second_id), Ok(&compiled(12)));
        assert_eq!(words.get(third_id), Ok(&primitive(9)));
    }

    #[test]
    fn later_additions_do_not_change_previous_ids() {
        let mut words = PublishedWords::new();

        let old_id = words.add(compiled(3));
        let old_definition = *words.get(old_id).expect("old id should be valid");

        let new_id = words.add(compiled(99));

        assert_eq!(words.get(old_id), Ok(&old_definition));
        assert_eq!(words.get(new_id), Ok(&compiled(99)));
    }

    #[test]
    fn primitive_and_compiled_definitions_are_distinguishable_after_lookup() {
        let mut words = PublishedWords::new();

        let primitive_id = words.add(primitive(5));
        let compiled_id = words.add(compiled(42));

        match words
            .get(primitive_id)
            .expect("primitive id should be valid")
        {
            WordDefinition::Primitive { primitive } => {
                assert_eq!(primitive.as_slot(), 5);
            }
            WordDefinition::Compiled { .. } => panic!("primitive id returned compiled word"),
        }

        match words.get(compiled_id).expect("compiled id should be valid") {
            WordDefinition::Compiled { entry } => {
                assert_eq!(entry.as_index(), 42);
            }
            WordDefinition::Primitive { .. } => panic!("compiled id returned primitive word"),
        }
    }

    #[test]
    fn lookup_rejects_invalid_ids_without_mutating_collection() {
        let mut words = PublishedWords::new();

        let empty_id = WordId::test_invalid(0);
        assert_eq!(
            words.get(empty_id),
            Err(WordLookupError::InvalidWordId { id: empty_id })
        );
        assert_eq!(words.len(), 0);
        assert!(words.is_empty());

        let valid_id = words.add(primitive(1));
        let out_of_range_id = WordId::test_invalid(2);
        let max_id = WordId::test_invalid(usize::MAX);

        assert_eq!(
            words.get(out_of_range_id),
            Err(WordLookupError::InvalidWordId {
                id: out_of_range_id
            })
        );
        assert_eq!(
            words.get(max_id),
            Err(WordLookupError::InvalidWordId { id: max_id })
        );
        assert_eq!(words.len(), 1);
        assert_eq!(words.get(valid_id), Ok(&primitive(1)));
    }

    #[test]
    fn read_only_lookup_rejects_invalid_ids_without_widening_word_id_visibility() {
        let mut words = PublishedWords::new();

        let empty_id = WordId::test_invalid(0);
        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(empty_id),
            Err(WordLookupError::InvalidWordId { id: empty_id })
        );
        assert_eq!(words.len(), 0);

        let valid_id = words.add(primitive(1));
        let out_of_range_id = WordId::test_invalid(2);
        let max_id = WordId::test_invalid(usize::MAX);

        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(out_of_range_id),
            Err(WordLookupError::InvalidWordId {
                id: out_of_range_id
            })
        );
        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(max_id),
            Err(WordLookupError::InvalidWordId { id: max_id })
        );
        assert_eq!(words.len(), 1);
        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(valid_id),
            Ok(&primitive(1))
        );
    }

    #[test]
    fn old_definitions_remain_accessible_after_multiple_additions() {
        let mut words = PublishedWords::new();

        let first_id = words.add(primitive(10));
        let second_id = words.add(compiled(20));
        let third_id = words.add(primitive(30));

        assert_eq!(words.get(first_id), Ok(&primitive(10)));
        assert_eq!(words.get(second_id), Ok(&compiled(20)));
        assert_eq!(words.get(third_id), Ok(&primitive(30)));

        let fourth_id = words.add(compiled(40));

        assert_eq!(words.get(first_id), Ok(&primitive(10)));
        assert_eq!(words.get(second_id), Ok(&compiled(20)));
        assert_eq!(words.get(third_id), Ok(&primitive(30)));
        assert_eq!(words.get(fourth_id), Ok(&compiled(40)));
    }

    #[test]
    fn definitions_do_not_require_names_or_vm_state() {
        let mut words = PublishedWords::new();

        let primitive_id = words.add(primitive(0));
        let compiled_id = words.add(compiled(8));

        assert_eq!(words.get(primitive_id), Ok(&primitive(0)));
        assert_eq!(words.get(compiled_id), Ok(&compiled(8)));
    }

    #[test]
    fn validated_add_accepts_compiled_entry_in_the_shared_instruction_sequence() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(Value::integer(42)));
        code.append(Instruction::Halt);
        let mut words = PublishedWords::new();

        let id = words
            .add_validated(compiled_entry(entry), code.view())
            .expect("compiled entry should point at an existing instruction");

        assert_eq!(words.len(), 1);
        assert_eq!(words.get(id), Ok(&compiled_entry(entry)));
    }

    #[test]
    fn validated_add_rejects_compiled_entry_at_end_without_issuing_word_id() {
        let mut code = InstructionSequence::new();
        code.append(Instruction::Halt);
        let end = InstructionAddress::from_index(code.len());
        let mut words = PublishedWords::new();

        let result = words.add_validated(compiled_entry(end), code.view());

        assert_eq!(
            result,
            Err(WordDefinitionError::InvalidCompiledEntry {
                error: InstructionAddressError::EndAddress { address: end }
            })
        );
        assert!(words.is_empty());
    }

    #[test]
    fn validated_add_rejects_compiled_entry_outside_current_owner_view() {
        let mut source_code = InstructionSequence::new();
        source_code.append(Instruction::Push(Value::integer(1)));
        let source_entry = source_code.append(Instruction::Halt);
        let target_code = InstructionSequence::new();
        let mut words = PublishedWords::new();

        // InstructionAddress intentionally has no owner tag. Publication must
        // validate against the owner view that will back VM execution.
        let result = words.add_validated(compiled_entry(source_entry), target_code.view());

        assert_eq!(
            result,
            Err(WordDefinitionError::InvalidCompiledEntry {
                error: InstructionAddressError::InvalidAddress {
                    address: source_entry
                }
            })
        );
        assert!(words.is_empty());
    }
}
