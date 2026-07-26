use std::collections::HashMap;

use crate::name::NormalizedName;
use crate::word::WordId;

/// Current published binding for one normalized TBX Next name.
///
/// ADR #1368 keeps words, scalars, and arrays in one logical namespace. This
/// enum starts with word bindings only because scalar and array IDs do not exist
/// yet; later variants must use the same registry so ordinary registration can
/// reject cross-kind name conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    Word(WordId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingInsertError {
    NameConflict,
}

/// Crate-internal map from normalized names to their current published binding.
///
/// This registry owns only name-to-binding state. It deliberately does not own
/// word definitions, query VM state, or provide replacement/removal APIs:
/// ordinary registration is non-overwriting, and executable word lookup remains
/// the responsibility of the word definition collection.
#[derive(Debug, Default)]
pub(crate) struct Bindings {
    entries: HashMap<NormalizedName, Binding>,
}

impl Bindings {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert_new(
        &mut self,
        name: NormalizedName,
        binding: Binding,
    ) -> Result<(), BindingInsertError> {
        if self.entries.contains_key(&name) {
            return Err(BindingInsertError::NameConflict);
        }

        self.entries.insert(name, binding);
        Ok(())
    }

    pub(crate) fn get(&self, name: &NormalizedName) -> Option<&Binding> {
        self.entries.get(name)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::word::{PrimitiveId, PublishedWords, WordDefinition};

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn primitive_word(slot: usize) -> WordDefinition {
        WordDefinition::Primitive {
            primitive: PrimitiveId::from_slot(slot),
        }
    }

    fn add_word(words: &mut PublishedWords, primitive_slot: usize) -> WordId {
        words.add(primitive_word(primitive_slot))
    }

    fn word_binding(words: &mut PublishedWords, primitive_slot: usize) -> Binding {
        Binding::Word(add_word(words, primitive_slot))
    }

    #[test]
    fn empty_registry_accepts_first_word_binding() {
        let mut words = PublishedWords::new();
        let binding = word_binding(&mut words, 0);
        let mut bindings = Bindings::new();

        assert_eq!(bindings.insert_new(name("PRINT"), binding), Ok(()));

        assert_eq!(bindings.len(), 1);
        assert!(!bindings.is_empty());
        assert_eq!(bindings.get(&name("PRINT")), Some(&binding));
    }

    #[test]
    fn registered_name_returns_the_same_word_id() {
        let mut words = PublishedWords::new();
        let id = add_word(&mut words, 7);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("ABS"), Binding::Word(id))
            .expect("new name should register");

        assert_eq!(bindings.get(&name("ABS")), Some(&Binding::Word(id)));
    }

    #[test]
    fn distinct_names_keep_distinct_word_bindings() {
        let mut words = PublishedWords::new();
        let first = word_binding(&mut words, 1);
        let second = word_binding(&mut words, 2);
        let third = word_binding(&mut words, 3);
        let mut bindings = Bindings::new();

        assert_eq!(bindings.insert_new(name("ALPHA"), first), Ok(()));
        assert_eq!(bindings.insert_new(name("BETA"), second), Ok(()));
        assert_eq!(bindings.insert_new(name("GAMMA?"), third), Ok(()));

        assert_eq!(bindings.get(&name("ALPHA")), Some(&first));
        assert_eq!(bindings.get(&name("BETA")), Some(&second));
        assert_eq!(bindings.get(&name("GAMMA?")), Some(&third));
    }

    #[test]
    fn lookup_preserves_binding_kind() {
        let mut words = PublishedWords::new();
        let id = add_word(&mut words, 5);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("CALL_ME"), Binding::Word(id))
            .expect("new name should register");

        match bindings
            .get(&name("CALL_ME"))
            .expect("binding should exist")
        {
            Binding::Word(actual_id) => assert_eq!(*actual_id, id),
        }
    }

    #[test]
    fn registry_works_without_fetching_word_definitions() {
        let mut words = PublishedWords::new();
        let id = add_word(&mut words, 9);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("STORED_ONLY_AS_ID"), Binding::Word(id))
            .expect("new name should register");

        assert_eq!(
            bindings.get(&name("STORED_ONLY_AS_ID")),
            Some(&Binding::Word(id))
        );
    }

    #[test]
    fn empty_registry_lookup_returns_unregistered() {
        let bindings = Bindings::new();

        assert_eq!(bindings.get(&name("MISSING")), None);
        assert!(bindings.is_empty());
    }

    #[test]
    fn lookup_does_not_fallback_to_other_registered_names() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("EXISTING"), word_binding(&mut words, 1))
            .expect("new name should register");

        assert_eq!(bindings.get(&name("MISSING")), None);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn unregistered_lookup_does_not_mutate_registry() {
        let bindings = Bindings::new();

        assert_eq!(bindings.get(&name("UNKNOWN")), None);
        assert_eq!(bindings.len(), 0);
        assert!(bindings.is_empty());
    }

    #[test]
    fn case_variants_resolve_to_the_same_registered_binding() {
        let mut words = PublishedWords::new();
        let binding = word_binding(&mut words, 4);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("foo"), binding)
            .expect("new name should register");

        assert_eq!(bindings.get(&name("foo")), Some(&binding));
        assert_eq!(bindings.get(&name("Foo")), Some(&binding));
        assert_eq!(bindings.get(&name("FOO")), Some(&binding));
    }

    #[test]
    fn predicate_name_case_variants_resolve_to_the_same_binding() {
        let mut words = PublishedWords::new();
        let binding = word_binding(&mut words, 6);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("ready?"), binding)
            .expect("new name should register");

        assert_eq!(bindings.get(&name("ready?")), Some(&binding));
        assert_eq!(bindings.get(&name("Ready?")), Some(&binding));
        assert_eq!(bindings.get(&name("READY?")), Some(&binding));
    }

    #[test]
    fn case_variant_registration_is_a_name_conflict() {
        let mut words = PublishedWords::new();
        let first = word_binding(&mut words, 10);
        let second = word_binding(&mut words, 11);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("foo"), first)
            .expect("new name should register");

        assert_eq!(
            bindings.insert_new(name("FOO"), second),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(bindings.get(&name("Foo")), Some(&first));
    }

    #[test]
    fn duplicate_registration_with_different_word_id_is_rejected_atomically() {
        let mut words = PublishedWords::new();
        let first = word_binding(&mut words, 20);
        let second = word_binding(&mut words, 21);
        let other = word_binding(&mut words, 22);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("DUP"), first)
            .expect("new name should register");
        bindings
            .insert_new(name("OTHER"), other)
            .expect("new name should register");

        assert_eq!(
            bindings.insert_new(name("DUP"), second),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(bindings.get(&name("DUP")), Some(&first));
        assert_eq!(bindings.get(&name("OTHER")), Some(&other));
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn duplicate_registration_with_same_word_id_is_still_rejected() {
        let mut words = PublishedWords::new();
        let binding = word_binding(&mut words, 30);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("SAME"), binding)
            .expect("new name should register");

        assert_eq!(
            bindings.insert_new(name("SAME"), binding),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(bindings.get(&name("SAME")), Some(&binding));
    }
}
