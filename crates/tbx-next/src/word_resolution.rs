use crate::binding::{Binding, Bindings};
use crate::name::{NameError, NormalizedName};
use crate::word::WordId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordResolutionError {
    InvalidWordName,
    UndefinedName,
    TargetIsNotWord,
}

/// Resolves a source word name through the current published binding only.
///
/// The resolver returns the `WordId` selected at lookup time and does not retain
/// names, scan published definitions, or create fallback bindings. Future
/// compiler paths must store this ID in their real compiled representation so
/// later redefinitions only affect future resolutions.
pub(crate) fn resolve_word_name(
    bindings: &Bindings,
    source_name: &str,
) -> Result<WordId, WordResolutionError> {
    let name = NormalizedName::new(source_name).map_err(WordResolutionError::from)?;
    resolve_normalized_word(bindings, &name)
}

pub(crate) fn resolve_normalized_word(
    bindings: &Bindings,
    name: &NormalizedName,
) -> Result<WordId, WordResolutionError> {
    match bindings.get(name) {
        Some(Binding::Word(id)) => Ok(*id),
        None => Err(WordResolutionError::UndefinedName),
    }
}

impl From<NameError> for WordResolutionError {
    fn from(error: NameError) -> Self {
        match error {
            NameError::InvalidWordName => Self::InvalidWordName,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::bootstrap::register_primitive;
    use crate::name::NormalizedName;
    use crate::redefinition::redefine_word;
    use crate::word::{InstructionAddress, PrimitiveId, PublishedWords, WordDefinition, WordId};
    use crate::word_lookup::PublishedWordLookup;

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

    #[test]
    fn source_word_name_resolves_through_normalized_binding_identity() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let id = publish_initial(&mut words, &mut bindings, "foo", primitive(1));

        assert_eq!(resolve_word_name(&bindings, "foo"), Ok(id));
        assert_eq!(resolve_word_name(&bindings, "Foo"), Ok(id));
        assert_eq!(resolve_word_name(&bindings, "FOO"), Ok(id));
    }

    #[test]
    fn predicate_word_name_resolves_after_case_normalization() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let id = publish_initial(&mut words, &mut bindings, "ready?", primitive(2));

        assert_eq!(resolve_word_name(&bindings, "ready?"), Ok(id));
        assert_eq!(resolve_word_name(&bindings, "Ready?"), Ok(id));
        assert_eq!(resolve_word_name(&bindings, "READY?"), Ok(id));
    }

    #[test]
    fn invalid_source_name_is_rejected_by_normalized_name_contract() {
        let bindings = Bindings::new();

        for invalid in ["", "1ABC", "A-B", "READY??", "NAMEé"] {
            assert_eq!(
                resolve_word_name(&bindings, invalid),
                Err(WordResolutionError::InvalidWordName),
                "{invalid:?} should be rejected"
            );
        }
    }

    #[test]
    fn undefined_name_is_rejected_without_mutation() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let id = publish_initial(&mut words, &mut bindings, "KNOWN", primitive(3));

        assert_eq!(
            resolve_word_name(&bindings, "MISSING"),
            Err(WordResolutionError::UndefinedName)
        );
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.get(&name("KNOWN")), Some(&Binding::Word(id)));
    }

    #[test]
    fn primitive_and_compiled_words_use_the_same_binding_path() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitive_id = publish_initial(&mut words, &mut bindings, "PRIM", primitive(4));
        let compiled_id = publish_initial(&mut words, &mut bindings, "USER_WORD", compiled(10));
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(resolve_word_name(&bindings, "prim"), Ok(primitive_id));
        assert_eq!(resolve_word_name(&bindings, "user_word"), Ok(compiled_id));
        assert_eq!(lookup.lookup_word(primitive_id), Ok(&primitive(4)));
        assert_eq!(lookup.lookup_word(compiled_id), Ok(&compiled(10)));
    }

    #[test]
    fn saved_resolution_keeps_old_word_id_after_redefinition() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = primitive(6);
        let new_definition = compiled(30);
        let old = publish_initial(&mut words, &mut bindings, "TARGET", old_definition);

        let old_resolution =
            resolve_word_name(&bindings, "target").expect("old target should resolve");
        let redefinition =
            redefine_word(&mut words, &mut bindings, &name("TARGET"), new_definition)
                .expect("existing word should redefine");
        let new_resolution =
            resolve_word_name(&bindings, "target").expect("new target should resolve");
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(old_resolution, old);
        assert_eq!(old_resolution, redefinition.previous());
        assert_eq!(new_resolution, redefinition.current());
        assert_ne!(old_resolution, new_resolution);
        assert_eq!(lookup.lookup_word(old_resolution), Ok(&old_definition));
        assert_eq!(lookup.lookup_word(new_resolution), Ok(&new_definition));
    }

    #[test]
    fn same_kind_redefinition_only_changes_future_resolutions() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old_definition = compiled(40);
        let new_definition = compiled(41);
        publish_initial(&mut words, &mut bindings, "CHAIN", old_definition);

        let old_resolution =
            resolve_word_name(&bindings, "CHAIN").expect("old target should resolve");
        let redefinition = redefine_word(&mut words, &mut bindings, &name("CHAIN"), new_definition)
            .expect("existing word should redefine");
        let new_resolution =
            resolve_word_name(&bindings, "CHAIN").expect("new target should resolve");
        let lookup = PublishedWordLookup::new(&words);

        assert_eq!(old_resolution, redefinition.previous());
        assert_eq!(new_resolution, redefinition.current());
        assert_eq!(lookup.lookup_word(old_resolution), Ok(&old_definition));
        assert_eq!(lookup.lookup_word(new_resolution), Ok(&new_definition));
    }

    #[test]
    fn unpublished_word_definition_is_not_resolved_by_name() {
        let mut words = PublishedWords::new();
        let bindings = Bindings::new();
        let unpublished = words.add(compiled(50));

        assert_eq!(
            resolve_word_name(&bindings, "UNPUBLISHED"),
            Err(WordResolutionError::UndefinedName)
        );
        assert_eq!(words.get(unpublished), Ok(&compiled(50)));
        assert_eq!(words.len(), 1);
        assert!(bindings.is_empty());
    }

    #[test]
    fn primitive_bootstrap_words_resolve_like_ordinary_words() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let id = register_primitive(
            &mut words,
            &mut bindings,
            name("BOOTSTRAPPED"),
            PrimitiveId::from_slot(60),
        )
        .expect("primitive should bootstrap");

        assert_eq!(resolve_word_name(&bindings, "bootstrapped"), Ok(id));
    }
}
