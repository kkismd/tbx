use crate::binding::{BindingReplaceError, Bindings};
use crate::name::NormalizedName;
use crate::word::{CompletedWordDefinition, PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WordRedefinition {
    previous: WordId,
    current: WordId,
}

impl WordRedefinition {
    pub(crate) const fn previous(self) -> WordId {
        self.previous
    }

    pub(crate) const fn current(self) -> WordId {
        self.current
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordRedefinitionError {
    UndefinedName,
    TargetIsNotWord,
    BindingCommitInvariantViolated,
}

/// Publishes a completed replacement definition for an existing word name.
///
/// ADR #1368 requires fully early-bound redefinition: old `WordId`s and their
/// definitions stay executable, and only future name lookup moves to the new
/// `WordId`. ADR #1369 makes the binding update the publication commit point,
/// so all ordinary preconditions are checked before the monotonic word append.
pub(crate) fn redefine_word(
    words: &mut PublishedWords,
    bindings: &mut Bindings,
    name: &NormalizedName,
    definition: CompletedWordDefinition,
) -> Result<WordRedefinition, WordRedefinitionError> {
    let previous = bindings
        .current_word(name)
        .map_err(WordRedefinitionError::from_precheck_error)?;

    let current = words.add(definition);

    bindings
        .replace_word(name, previous, current)
        .map_err(WordRedefinitionError::from_commit_error)?;

    Ok(WordRedefinition { previous, current })
}

impl WordRedefinitionError {
    fn from_precheck_error(error: BindingReplaceError) -> Self {
        match error {
            BindingReplaceError::MissingName => Self::UndefinedName,
            BindingReplaceError::TargetIsNotWord => Self::TargetIsNotWord,
            BindingReplaceError::CurrentWordMismatch { .. } => Self::BindingCommitInvariantViolated,
        }
    }

    fn from_commit_error(error: BindingReplaceError) -> Self {
        match error {
            BindingReplaceError::MissingName
            | BindingReplaceError::TargetIsNotWord
            | BindingReplaceError::CurrentWordMismatch { .. } => {
                Self::BindingCommitInvariantViolated
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::value::Value;
    use crate::word::{PrimitiveId, WordDefinition};

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn primitive(slot: usize) -> CompletedWordDefinition {
        CompletedWordDefinition::primitive(PrimitiveId::from_slot(slot))
    }

    fn primitive_definition(slot: usize) -> WordDefinition {
        WordDefinition::Primitive {
            primitive: PrimitiveId::from_slot(slot),
        }
    }

    fn compiled(code: &mut InstructionSequence, value: i16) -> CompletedWordDefinition {
        let entry = code.append(Instruction::Push(Value::integer(value)));
        CompletedWordDefinition::compiled(entry, code.view())
            .expect("test compiled entry should be valid")
    }

    fn publish_initial(
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        input: &str,
        definition: CompletedWordDefinition,
    ) -> WordId {
        let id = words.add(definition);
        bindings
            .insert_new(name(input), Binding::Word(id))
            .expect("initial test binding should register");
        id
    }

    fn assert_word_binding(bindings: &Bindings, input: &str, expected: WordId) {
        assert_eq!(bindings.get(&name(input)), Some(&Binding::Word(expected)));
    }

    fn assert_redefinition(
        words: &PublishedWords,
        bindings: &Bindings,
        input: &str,
        result: WordRedefinition,
        old_definition: CompletedWordDefinition,
        new_definition: CompletedWordDefinition,
    ) {
        assert_ne!(result.previous(), result.current());
        assert_word_binding(bindings, input, result.current());
        assert_eq!(
            words.get(result.previous()),
            Ok(&old_definition.definition())
        );
        assert_eq!(
            words.get(result.current()),
            Ok(&new_definition.definition())
        );
    }

    #[test]
    fn primitive_word_can_be_redefined_as_primitive_word() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = primitive(1);
        let new_definition = primitive(2);
        let old = publish_initial(&mut words, &mut bindings, "PRINT", old_definition);

        let result = redefine_word(&mut words, &mut bindings, &name("PRINT"), new_definition)
            .expect("existing word should redefine");

        assert_eq!(result.previous(), old);
        assert_redefinition(
            &words,
            &bindings,
            "PRINT",
            result,
            old_definition,
            new_definition,
        );
    }

    #[test]
    fn primitive_word_can_be_redefined_as_compiled_word() {
        let mut code = InstructionSequence::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = primitive(3);
        let new_definition = compiled(&mut code, 100);
        let old = publish_initial(&mut words, &mut bindings, "ABS", old_definition);

        let result = redefine_word(&mut words, &mut bindings, &name("ABS"), new_definition)
            .expect("existing word should redefine");

        assert_eq!(result.previous(), old);
        assert_redefinition(
            &words,
            &bindings,
            "ABS",
            result,
            old_definition,
            new_definition,
        );
    }

    #[test]
    fn compiled_word_can_be_redefined_as_primitive_word() {
        let mut code = InstructionSequence::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = compiled(&mut code, 200);
        let new_definition = primitive(4);
        let old = publish_initial(&mut words, &mut bindings, "RUN", old_definition);

        let result = redefine_word(&mut words, &mut bindings, &name("RUN"), new_definition)
            .expect("existing word should redefine");

        assert_eq!(result.previous(), old);
        assert_redefinition(
            &words,
            &bindings,
            "RUN",
            result,
            old_definition,
            new_definition,
        );
    }

    #[test]
    fn compiled_word_can_be_redefined_as_compiled_word() {
        let mut code = InstructionSequence::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = compiled(&mut code, 300);
        let new_definition = compiled(&mut code, 301);
        let old = publish_initial(&mut words, &mut bindings, "LOOP", old_definition);

        let result = redefine_word(&mut words, &mut bindings, &name("LOOP"), new_definition)
            .expect("existing word should redefine");

        assert_eq!(result.previous(), old);
        assert_redefinition(
            &words,
            &bindings,
            "LOOP",
            result,
            old_definition,
            new_definition,
        );
    }

    #[test]
    fn undefined_name_is_rejected_before_issuing_word_id() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let other_definition = primitive(5);
        let other = publish_initial(&mut words, &mut bindings, "OTHER", other_definition);

        let result = redefine_word(&mut words, &mut bindings, &name("MISSING"), primitive(400));

        assert_eq!(result, Err(WordRedefinitionError::UndefinedName));
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_word_binding(&bindings, "OTHER", other);
        assert_eq!(words.get(other), Ok(&other_definition.definition()));
    }

    #[test]
    fn case_variant_redefinition_updates_the_existing_binding() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = primitive(6);
        let mut code = InstructionSequence::new();
        let new_definition = compiled(&mut code, 500);
        let old = publish_initial(&mut words, &mut bindings, "foo", old_definition);

        let result = redefine_word(&mut words, &mut bindings, &name("FOO"), new_definition)
            .expect("case-equivalent word should redefine");

        assert_eq!(result.previous(), old);
        assert_eq!(bindings.len(), 1);
        assert_redefinition(
            &words,
            &bindings,
            "Foo",
            result,
            old_definition,
            new_definition,
        );
    }

    #[test]
    fn repeated_redefinitions_keep_all_previous_definitions_addressable() {
        let mut code = InstructionSequence::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let first_definition = primitive(7);
        let second_definition = compiled(&mut code, 600);
        let third_definition = primitive(8);
        let fourth_definition = compiled(&mut code, 601);
        let first = publish_initial(&mut words, &mut bindings, "CHAIN", first_definition);

        let second = redefine_word(&mut words, &mut bindings, &name("CHAIN"), second_definition)
            .expect("second definition should publish")
            .current();
        let third = redefine_word(&mut words, &mut bindings, &name("CHAIN"), third_definition)
            .expect("third definition should publish")
            .current();
        let fourth = redefine_word(&mut words, &mut bindings, &name("CHAIN"), fourth_definition)
            .expect("fourth definition should publish")
            .current();

        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_ne!(third, fourth);
        assert_word_binding(&bindings, "CHAIN", fourth);
        assert_eq!(words.get(first), Ok(&first_definition.definition()));
        assert_eq!(words.get(second), Ok(&second_definition.definition()));
        assert_eq!(words.get(third), Ok(&third_definition.definition()));
        assert_eq!(words.get(fourth), Ok(&fourth_definition.definition()));
        assert_eq!(words.len(), 4);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn redefinition_does_not_require_vm_state_or_handler_table() {
        let mut code = InstructionSequence::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        publish_initial(&mut words, &mut bindings, "BOUNDARY", primitive(9));

        let result = redefine_word(
            &mut words,
            &mut bindings,
            &name("BOUNDARY"),
            compiled(&mut code, 700),
        )
        .expect("completed definition should publish without VM state");

        assert_word_binding(&bindings, "BOUNDARY", result.current());
        assert_eq!(words.len(), 2);
    }
}
