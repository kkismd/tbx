use crate::word::{PublishedWords, WordDefinition, WordId, WordLookupError};

/// Read-only lookup boundary for executable published word definitions.
///
/// ADR #1367/#1368 keep VM dispatch on resolved `WordId`s separate from name
/// binding, bootstrap, redefinition, and mutable VM state. This view gives a
/// future VM only the lookup operation it needs while delegating validity and
/// old-definition preservation to `PublishedWords`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PublishedWordLookup<'a> {
    words: &'a PublishedWords,
}

impl<'a> PublishedWordLookup<'a> {
    pub(crate) const fn new(words: &'a PublishedWords) -> Self {
        Self { words }
    }

    pub(crate) fn lookup_word(self, id: WordId) -> Result<&'a WordDefinition, WordLookupError> {
        self.words.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::instruction::CodeLocation;
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::name::NormalizedName;
    use crate::redefinition::redefine_word;
    use crate::value::Value;
    use crate::word::{CompletedWordDefinition, PrimitiveId};

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn primitive(slot: usize) -> WordDefinition {
        WordDefinition::Primitive {
            primitive: PrimitiveId::from_slot(slot),
        }
    }

    fn completed_primitive(slot: usize) -> CompletedWordDefinition {
        CompletedWordDefinition::primitive(PrimitiveId::from_slot(slot))
    }

    fn compiled_definition(entry: CodeLocation) -> WordDefinition {
        WordDefinition::Compiled { entry }
    }

    fn completed_compiled(code: &mut InstructionSequence, value: i16) -> CompletedWordDefinition {
        let entry = code.append(Instruction::Push(Value::integer(value)));
        CompletedWordDefinition::compiled(code.view().location(entry), code.view())
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

    fn vm_like_dispatch_target(
        lookup: PublishedWordLookup<'_>,
        id: WordId,
    ) -> Result<&WordDefinition, WordLookupError> {
        lookup.lookup_word(id)
    }

    #[test]
    fn primitive_lookup_preserves_primitive_identity() {
        let mut words = PublishedWords::new();
        let primitive_id = PrimitiveId::from_slot(17);
        let id = words.add(CompletedWordDefinition::primitive(primitive_id));
        let lookup = PublishedWordLookup::new(&words);

        match lookup.lookup_word(id).expect("word id should be valid") {
            WordDefinition::Primitive { primitive } => assert_eq!(*primitive, primitive_id),
            WordDefinition::Compiled { .. } => panic!("primitive id returned compiled word"),
        }
    }

    #[test]
    fn compiled_lookup_preserves_entry_location() {
        let mut code = InstructionSequence::new();
        let mut words = PublishedWords::new();
        let entry = code.append(Instruction::Halt);
        let location = code.view().location(entry);
        let id = words.add(
            CompletedWordDefinition::compiled(location, code.view())
                .expect("compiled entry should be valid"),
        );
        let lookup = PublishedWordLookup::new(&words);

        match lookup.lookup_word(id).expect("word id should be valid") {
            WordDefinition::Compiled { entry: actual } => {
                assert_eq!(*actual, location);
                assert_eq!(actual.code_space(), code.code_space());
                assert_eq!(actual.address(), entry);
            }
            WordDefinition::Primitive { .. } => panic!("compiled id returned primitive word"),
        }
    }

    #[test]
    fn multiple_definitions_lookup_by_their_own_ids() {
        let mut words = PublishedWords::new();
        let mut code = InstructionSequence::new();
        let second_definition = completed_compiled(&mut code, 20);
        let first = words.add(completed_primitive(1));
        let second = words.add(second_definition);
        let third = words.add(completed_primitive(3));
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(lookup.lookup_word(first), Ok(&primitive(1)));
        assert_eq!(
            lookup.lookup_word(second),
            Ok(&second_definition.definition())
        );
        assert_eq!(lookup.lookup_word(third), Ok(&primitive(3)));
    }

    #[test]
    fn later_additions_do_not_change_lookup_for_existing_ids() {
        let mut words = PublishedWords::new();
        let mut code = InstructionSequence::new();
        let old_definition = completed_compiled(&mut code, 10);
        let new_definition = completed_compiled(&mut code, 99);
        let old = words.add(old_definition);
        let old_definition = *PublishedWordLookup::new(&words)
            .lookup_word(old)
            .expect("old id should be valid");

        let new = words.add(new_definition);
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(lookup.lookup_word(old), Ok(&old_definition));
        assert_eq!(lookup.lookup_word(new), Ok(&new_definition.definition()));
    }

    #[test]
    fn lookup_does_not_require_bindings_or_names() {
        let mut words = PublishedWords::new();
        let mut code = InstructionSequence::new();
        let compiled_definition = completed_compiled(&mut code, 12);
        let primitive_id = words.add(completed_primitive(11));
        let compiled_id = words.add(compiled_definition);
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(
            vm_like_dispatch_target(lookup, primitive_id),
            Ok(&primitive(11))
        );
        assert_eq!(
            vm_like_dispatch_target(lookup, compiled_id),
            Ok(&compiled_definition.definition())
        );
    }

    #[test]
    fn lookup_result_is_independent_from_current_name_binding() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut code = InstructionSequence::new();
        let old_definition = primitive(30);
        let new_definition = completed_compiled(&mut code, 300);
        let old = publish_initial(&mut words, &mut bindings, "TARGET", completed_primitive(30));

        let redefinition =
            redefine_word(&mut words, &mut bindings, &name("TARGET"), new_definition)
                .expect("existing word should redefine");
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(redefinition.previous(), old);
        assert_eq!(
            bindings.get(&name("TARGET")),
            Some(&Binding::Word(redefinition.current()))
        );
        assert_eq!(
            lookup.lookup_word(redefinition.previous()),
            Ok(&old_definition)
        );
        assert_eq!(
            lookup.lookup_word(redefinition.current()),
            Ok(&new_definition.definition())
        );
    }
}
