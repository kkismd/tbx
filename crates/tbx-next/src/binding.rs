use std::collections::{HashMap, HashSet};

use crate::global_variable::GlobalVarId;
use crate::name::NormalizedName;
use crate::source_word::SourceWordId;
use crate::word::WordId;

/// Current published binding for one normalized TBX Next name.
///
/// ADR #1368 keeps words, scalars, and arrays in one logical namespace, so
/// ordinary registration rejects cross-kind name conflicts through this same
/// registry instead of splitting words and variables into separate maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    Word(WordId),
    SourceWord(SourceWordId),
    Variable(GlobalVarId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingInsertError {
    NameConflict,
    ReservedName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxMarkerReservation {
    owner: SourceWordId,
}

impl SyntaxMarkerReservation {
    pub(crate) const fn owner(self) -> SourceWordId {
        self.owner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingReplaceError {
    MissingName,
    TargetIsNotWord,
    CurrentWordMismatch { actual: WordId },
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
    // #1513 keeps syntax markers distinct from bindings while reserving their
    // names in the same case-insensitive publication namespace.
    syntax_marker_reservations: HashMap<NormalizedName, SyntaxMarkerReservation>,
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
        self.validate_new_name(&name)?;

        self.entries.insert(name, binding);
        Ok(())
    }

    pub(crate) fn validate_new_name(
        &self,
        name: &NormalizedName,
    ) -> Result<(), BindingInsertError> {
        if is_semantic_reserved_binding_name(name) {
            return Err(BindingInsertError::ReservedName);
        }

        if self.entries.contains_key(name) {
            return Err(BindingInsertError::NameConflict);
        }

        if self.syntax_marker_reservations.contains_key(name) {
            return Err(BindingInsertError::NameConflict);
        }

        Ok(())
    }

    pub(crate) fn insert_new_source_word_with_markers(
        &mut self,
        name: NormalizedName,
        id: SourceWordId,
        marker_names: &[NormalizedName],
    ) -> Result<(), BindingInsertError> {
        self.validate_new_source_word_with_markers(&name, marker_names)?;

        self.entries.insert(name, Binding::SourceWord(id));
        for marker_name in marker_names {
            self.syntax_marker_reservations
                .insert(marker_name.clone(), SyntaxMarkerReservation { owner: id });
        }
        Ok(())
    }

    pub(crate) fn validate_new_source_word_with_markers(
        &self,
        name: &NormalizedName,
        marker_names: &[NormalizedName],
    ) -> Result<(), BindingInsertError> {
        self.validate_new_name(name)?;

        let mut declared = HashSet::with_capacity(marker_names.len());
        for marker_name in marker_names {
            self.validate_new_name(marker_name)?;
            if marker_name == name || !declared.insert(marker_name) {
                return Err(BindingInsertError::NameConflict);
            }
        }

        Ok(())
    }

    pub(crate) fn get(&self, name: &NormalizedName) -> Option<&Binding> {
        self.entries.get(name)
    }

    pub(crate) fn syntax_marker_reservation(
        &self,
        name: &NormalizedName,
    ) -> Option<SyntaxMarkerReservation> {
        self.syntax_marker_reservations.get(name).copied()
    }

    pub(crate) fn current_word(
        &self,
        name: &NormalizedName,
    ) -> Result<WordId, BindingReplaceError> {
        match self.entries.get(name) {
            Some(Binding::Word(id)) => Ok(*id),
            Some(Binding::SourceWord(_) | Binding::Variable(_)) => {
                Err(BindingReplaceError::TargetIsNotWord)
            }
            None => Err(BindingReplaceError::MissingName),
        }
    }

    pub(crate) fn replace_word(
        &mut self,
        name: &NormalizedName,
        expected: WordId,
        replacement: WordId,
    ) -> Result<(), BindingReplaceError> {
        match self.entries.get_mut(name) {
            Some(Binding::Word(current)) if *current == expected => {
                *current = replacement;
                Ok(())
            }
            Some(Binding::Word(actual)) => {
                Err(BindingReplaceError::CurrentWordMismatch { actual: *actual })
            }
            Some(Binding::SourceWord(_) | Binding::Variable(_)) => {
                Err(BindingReplaceError::TargetIsNotWord)
            }
            None => Err(BindingReplaceError::MissingName),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn syntax_marker_reservation_len(&self) -> usize {
        self.syntax_marker_reservations.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// END is reserved by ADR #1500 3.1.1. REM is reserved by ADR #1536 3.7 while
// still staying outside the lexer keyword set.
fn is_semantic_reserved_binding_name(name: &NormalizedName) -> bool {
    matches!(name.as_str(), "END" | "REM")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_variable::GlobalVariables;
    use crate::source_word::SourceWordRegistry;
    use crate::word::{CompletedWordDefinition, PrimitiveId, PublishedWords};

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn add_word(words: &mut PublishedWords, primitive_slot: usize) -> WordId {
        words.add(CompletedWordDefinition::primitive(PrimitiveId::from_slot(
            primitive_slot,
        )))
    }

    fn word_binding(words: &mut PublishedWords, primitive_slot: usize) -> Binding {
        Binding::Word(add_word(words, primitive_slot))
    }

    fn variable_binding(globals: &mut GlobalVariables) -> Binding {
        Binding::Variable(globals.allocate())
    }

    fn source_word_binding(source_words: &mut SourceWordRegistry) -> Binding {
        fn handler(
            _context: &mut crate::source_word::NativeSourceWordContext<'_, '_>,
        ) -> Result<(), crate::source_word::SourceWordError> {
            Ok(())
        }

        Binding::SourceWord(source_words.register(handler))
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
            Binding::SourceWord(_) | Binding::Variable(_) => {
                panic!("word binding should preserve kind")
            }
        }
    }

    #[test]
    fn source_word_binding_registers_and_looks_up_in_the_same_namespace() {
        let mut source_words = SourceWordRegistry::new();
        let binding = source_word_binding(&mut source_words);
        let mut bindings = Bindings::new();

        assert_eq!(bindings.insert_new(name("SOURCE_ONLY"), binding), Ok(()));

        assert_eq!(bindings.get(&name("source_only")), Some(&binding));
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn variable_binding_registers_and_looks_up_in_the_same_namespace() {
        let mut globals = GlobalVariables::new();
        let binding = variable_binding(&mut globals);
        let mut bindings = Bindings::new();

        assert_eq!(bindings.insert_new(name("A"), binding), Ok(()));

        assert_eq!(bindings.get(&name("A")), Some(&binding));
        assert_eq!(bindings.len(), 1);
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
    fn semantic_reserved_name_case_variants_are_rejected() {
        for input in ["END", "end", "End", "REM", "rem", "Rem"] {
            let mut words = PublishedWords::new();
            let binding = word_binding(&mut words, 10);
            let mut bindings = Bindings::new();

            assert_eq!(
                bindings.validate_new_name(&name(input)),
                Err(BindingInsertError::ReservedName)
            );
            assert_eq!(
                bindings.insert_new(name(input), binding),
                Err(BindingInsertError::ReservedName)
            );
            assert!(bindings.is_empty());
        }
    }

    #[test]
    fn validate_new_name_distinguishes_reserved_name_from_conflict() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let existing = word_binding(&mut words, 20);

        bindings
            .insert_new(name("TAKEN"), existing)
            .expect("test binding should register");

        assert_eq!(
            bindings.validate_new_name(&name("TAKEN")),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(
            bindings.validate_new_name(&name("END")),
            Err(BindingInsertError::ReservedName)
        );
        assert_eq!(bindings.validate_new_name(&name("AVAILABLE")), Ok(()));
    }

    #[test]
    fn word_then_variable_same_name_is_rejected() {
        let mut words = PublishedWords::new();
        let word = word_binding(&mut words, 12);
        let mut globals = GlobalVariables::new();
        let variable = variable_binding(&mut globals);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("X"), word)
            .expect("word should register first");

        assert_eq!(
            bindings.insert_new(name("X"), variable),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(bindings.get(&name("X")), Some(&word));
    }

    #[test]
    fn variable_then_word_same_name_is_rejected() {
        let mut globals = GlobalVariables::new();
        let variable = variable_binding(&mut globals);
        let mut words = PublishedWords::new();
        let word = word_binding(&mut words, 13);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("Y"), variable)
            .expect("variable should register first");

        assert_eq!(
            bindings.insert_new(name("Y"), word),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(bindings.get(&name("Y")), Some(&variable));
    }

    #[test]
    fn word_and_variable_case_variant_registration_is_a_name_conflict() {
        let mut globals = GlobalVariables::new();
        let variable = variable_binding(&mut globals);
        let mut words = PublishedWords::new();
        let word = word_binding(&mut words, 14);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("score"), variable)
            .expect("variable should register first");

        assert_eq!(
            bindings.insert_new(name("SCORE"), word),
            Err(BindingInsertError::NameConflict)
        );
        assert_eq!(bindings.get(&name("Score")), Some(&variable));
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

    #[test]
    fn replace_word_updates_only_the_expected_existing_word_binding() {
        let mut words = PublishedWords::new();
        let old = add_word(&mut words, 40);
        let new = add_word(&mut words, 41);
        let other = add_word(&mut words, 42);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("TARGET"), Binding::Word(old))
            .expect("new name should register");
        bindings
            .insert_new(name("OTHER"), Binding::Word(other))
            .expect("new name should register");

        assert_eq!(bindings.current_word(&name("TARGET")), Ok(old));
        assert_eq!(bindings.replace_word(&name("TARGET"), old, new), Ok(()));

        assert_eq!(bindings.current_word(&name("TARGET")), Ok(new));
        assert_eq!(bindings.get(&name("OTHER")), Some(&Binding::Word(other)));
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn replace_word_rejects_missing_name_without_mutation() {
        let mut words = PublishedWords::new();
        let existing = add_word(&mut words, 50);
        let replacement = add_word(&mut words, 51);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("EXISTING"), Binding::Word(existing))
            .expect("new name should register");

        assert_eq!(
            bindings.replace_word(&name("MISSING"), existing, replacement),
            Err(BindingReplaceError::MissingName)
        );
        assert_eq!(
            bindings.current_word(&name("MISSING")),
            Err(BindingReplaceError::MissingName)
        );
        assert_eq!(bindings.current_word(&name("EXISTING")), Ok(existing));
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn replace_word_rejects_unexpected_current_word_without_mutation() {
        let mut words = PublishedWords::new();
        let expected = add_word(&mut words, 60);
        let actual = add_word(&mut words, 61);
        let replacement = add_word(&mut words, 62);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("TARGET"), Binding::Word(actual))
            .expect("new name should register");

        assert_eq!(
            bindings.replace_word(&name("TARGET"), expected, replacement),
            Err(BindingReplaceError::CurrentWordMismatch { actual })
        );
        assert_eq!(bindings.current_word(&name("TARGET")), Ok(actual));
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn current_word_rejects_variable_binding() {
        let mut globals = GlobalVariables::new();
        let variable = variable_binding(&mut globals);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("TOTAL"), variable)
            .expect("variable should register");

        assert_eq!(
            bindings.current_word(&name("TOTAL")),
            Err(BindingReplaceError::TargetIsNotWord)
        );
        assert_eq!(bindings.get(&name("TOTAL")), Some(&variable));
    }

    #[test]
    fn replace_word_rejects_variable_binding_without_mutation() {
        let mut globals = GlobalVariables::new();
        let variable = variable_binding(&mut globals);
        let mut words = PublishedWords::new();
        let expected = add_word(&mut words, 70);
        let replacement = add_word(&mut words, 71);
        let mut bindings = Bindings::new();

        bindings
            .insert_new(name("TOTAL"), variable)
            .expect("variable should register");

        assert_eq!(
            bindings.replace_word(&name("TOTAL"), expected, replacement),
            Err(BindingReplaceError::TargetIsNotWord)
        );
        assert_eq!(bindings.get(&name("TOTAL")), Some(&variable));
        assert_eq!(bindings.len(), 1);
    }
}
