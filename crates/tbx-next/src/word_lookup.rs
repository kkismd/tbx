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
    use crate::name::NormalizedName;
    use crate::redefinition::redefine_word;
    use crate::word::{InstructionAddress, PrimitiveId};

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

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

    fn publish_initial(
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        input: &str,
        definition: WordDefinition,
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
        let id = words.add(WordDefinition::Primitive {
            primitive: primitive_id,
        });
        let lookup = PublishedWordLookup::new(&words);

        match lookup.lookup_word(id).expect("word id should be valid") {
            WordDefinition::Primitive { primitive } => assert_eq!(*primitive, primitive_id),
            WordDefinition::Compiled { .. } => panic!("primitive id returned compiled word"),
        }
    }

    #[test]
    fn compiled_lookup_preserves_entry_address() {
        let mut words = PublishedWords::new();
        let entry = InstructionAddress::from_index(42);
        let id = words.add(WordDefinition::Compiled { entry });
        let lookup = PublishedWordLookup::new(&words);

        match lookup.lookup_word(id).expect("word id should be valid") {
            WordDefinition::Compiled { entry: actual } => assert_eq!(*actual, entry),
            WordDefinition::Primitive { .. } => panic!("compiled id returned primitive word"),
        }
    }

    #[test]
    fn multiple_definitions_lookup_by_their_own_ids() {
        let mut words = PublishedWords::new();
        let first = words.add(primitive(1));
        let second = words.add(compiled(20));
        let third = words.add(primitive(3));
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(lookup.lookup_word(first), Ok(&primitive(1)));
        assert_eq!(lookup.lookup_word(second), Ok(&compiled(20)));
        assert_eq!(lookup.lookup_word(third), Ok(&primitive(3)));
    }

    #[test]
    fn later_additions_do_not_change_lookup_for_existing_ids() {
        let mut words = PublishedWords::new();
        let old = words.add(compiled(10));
        let old_definition = *PublishedWordLookup::new(&words)
            .lookup_word(old)
            .expect("old id should be valid");

        let new = words.add(compiled(99));
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(lookup.lookup_word(old), Ok(&old_definition));
        assert_eq!(lookup.lookup_word(new), Ok(&compiled(99)));
    }

    #[test]
    fn invalid_ids_are_reported_without_mutating_words() {
        let mut words = PublishedWords::new();
        let empty_id = WordId::test_invalid(0);

        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(empty_id),
            Err(WordLookupError::InvalidWordId { id: empty_id })
        );
        assert_eq!(words.len(), 0);

        let valid = words.add(primitive(5));
        let out_of_range = WordId::test_invalid(2);
        let max_id = WordId::test_invalid(usize::MAX);

        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(out_of_range),
            Err(WordLookupError::InvalidWordId { id: out_of_range })
        );
        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(max_id),
            Err(WordLookupError::InvalidWordId { id: max_id })
        );
        assert_eq!(words.len(), 1);
        assert_eq!(
            PublishedWordLookup::new(&words).lookup_word(valid),
            Ok(&primitive(5))
        );
    }

    #[test]
    fn lookup_does_not_require_bindings_or_names() {
        let mut words = PublishedWords::new();
        let primitive_id = words.add(primitive(11));
        let compiled_id = words.add(compiled(12));
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(
            vm_like_dispatch_target(lookup, primitive_id),
            Ok(&primitive(11))
        );
        assert_eq!(
            vm_like_dispatch_target(lookup, compiled_id),
            Ok(&compiled(12))
        );
    }

    #[test]
    fn lookup_result_is_independent_from_current_name_binding() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = primitive(30);
        let new_definition = compiled(300);
        let old = publish_initial(&mut words, &mut bindings, "TARGET", old_definition);

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
            Ok(&new_definition)
        );
    }
}
