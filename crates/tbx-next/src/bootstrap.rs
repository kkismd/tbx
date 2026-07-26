use crate::binding::{Binding, BindingInsertError, Bindings};
use crate::name::NormalizedName;
use crate::word::{PrimitiveId, PublishedWords, WordDefinition, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimitiveBootstrapError {
    NameConflict,
    BindingRegistrationInvariantViolated,
}

/// Registers one primitive word through the bootstrap-only publication boundary.
///
/// ADR #1368 gives primitives no separate namespace or privileged lookup path:
/// they are published as ordinary word bindings in the single binding table.
/// Because `PublishedWords` is monotonic and has no rollback API, the name
/// conflict check must happen before issuing a new `WordId`. Initial bootstrap
/// is logically serial, so a failed `insert_new` after this precheck indicates
/// an internal invariant violation rather than a concurrent redefinition race.
pub(crate) fn register_primitive(
    words: &mut PublishedWords,
    bindings: &mut Bindings,
    name: NormalizedName,
    primitive: PrimitiveId,
) -> Result<WordId, PrimitiveBootstrapError> {
    if bindings.get(&name).is_some() {
        return Err(PrimitiveBootstrapError::NameConflict);
    }

    let id = words.add(WordDefinition::Primitive { primitive });

    bindings
        .insert_new(name, Binding::Word(id))
        .map_err(PrimitiveBootstrapError::from_binding_insert_error)?;

    Ok(id)
}

impl PrimitiveBootstrapError {
    fn from_binding_insert_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::BindingRegistrationInvariantViolated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn primitive(slot: usize) -> PrimitiveId {
        PrimitiveId::from_slot(slot)
    }

    fn assert_primitive(words: &PublishedWords, id: WordId, expected: PrimitiveId) {
        match words.get(id).expect("word id should be published") {
            WordDefinition::Primitive { primitive } => assert_eq!(*primitive, expected),
            WordDefinition::Compiled { .. } => {
                panic!("primitive registration returned compiled word")
            }
        }
    }

    fn assert_word_binding(bindings: &Bindings, input: &str, expected: WordId) {
        assert_eq!(bindings.get(&name(input)), Some(&Binding::Word(expected)));
    }

    #[test]
    fn empty_collections_accept_first_primitive() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitive = primitive(0);

        let id = register_primitive(&mut words, &mut bindings, name("PRINT"), primitive)
            .expect("new primitive name should register");

        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_word_binding(&bindings, "PRINT", id);
        assert_primitive(&words, id, primitive);
    }

    #[test]
    fn returned_word_id_matches_registered_binding() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let id = register_primitive(&mut words, &mut bindings, name("ABS"), primitive(7))
            .expect("new primitive name should register");

        assert_word_binding(&bindings, "ABS", id);
    }

    #[test]
    fn multiple_names_register_multiple_primitives_in_order() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let first = register_primitive(&mut words, &mut bindings, name("FIRST"), primitive(1))
            .expect("first primitive should register");
        let second = register_primitive(&mut words, &mut bindings, name("SECOND"), primitive(2))
            .expect("second primitive should register");
        let third = register_primitive(&mut words, &mut bindings, name("THIRD?"), primitive(3))
            .expect("third primitive should register");

        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);
        assert_eq!(words.len(), 3);
        assert_eq!(bindings.len(), 3);
        assert_word_binding(&bindings, "FIRST", first);
        assert_word_binding(&bindings, "SECOND", second);
        assert_word_binding(&bindings, "THIRD?", third);
        assert_primitive(&words, first, primitive(1));
        assert_primitive(&words, second, primitive(2));
        assert_primitive(&words, third, primitive(3));
    }

    #[test]
    fn later_registration_does_not_change_earlier_name_or_word_id() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let first = register_primitive(&mut words, &mut bindings, name("OLD"), primitive(10))
            .expect("first primitive should register");
        let old_definition = *words.get(first).expect("first id should be published");

        let second = register_primitive(&mut words, &mut bindings, name("NEW"), primitive(11))
            .expect("second primitive should register");

        assert_word_binding(&bindings, "OLD", first);
        assert_word_binding(&bindings, "NEW", second);
        assert_eq!(words.get(first), Ok(&old_definition));
        assert_primitive(&words, second, primitive(11));
    }

    #[test]
    fn registered_primitive_is_visible_through_case_variants() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let id = register_primitive(&mut words, &mut bindings, name("foo"), primitive(4))
            .expect("new primitive name should register");

        assert_word_binding(&bindings, "foo", id);
        assert_word_binding(&bindings, "Foo", id);
        assert_word_binding(&bindings, "FOO", id);
    }

    #[test]
    fn predicate_name_is_visible_through_case_variants() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let id = register_primitive(&mut words, &mut bindings, name("ready?"), primitive(5))
            .expect("new primitive name should register");

        assert_word_binding(&bindings, "ready?", id);
        assert_word_binding(&bindings, "Ready?", id);
        assert_word_binding(&bindings, "READY?", id);
    }

    #[test]
    fn duplicate_bootstrap_registration_is_rejected_without_mutation() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let first_primitive = primitive(20);
        let first = register_primitive(&mut words, &mut bindings, name("DUP"), first_primitive)
            .expect("first primitive should register");

        let result = register_primitive(&mut words, &mut bindings, name("DUP"), primitive(21));

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_word_binding(&bindings, "DUP", first);
        assert_primitive(&words, first, first_primitive);
    }

    #[test]
    fn case_variant_bootstrap_registration_is_rejected_without_mutation() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let first = register_primitive(&mut words, &mut bindings, name("foo"), primitive(30))
            .expect("first primitive should register");

        let result = register_primitive(&mut words, &mut bindings, name("FOO"), primitive(31));

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_word_binding(&bindings, "Foo", first);
        assert_primitive(&words, first, primitive(30));
    }

    #[test]
    fn same_primitive_same_name_registration_is_still_rejected() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitive = primitive(40);
        let first = register_primitive(&mut words, &mut bindings, name("SAME"), primitive)
            .expect("first primitive should register");

        let result = register_primitive(&mut words, &mut bindings, name("SAME"), primitive);

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_word_binding(&bindings, "SAME", first);
        assert_primitive(&words, first, primitive);
    }

    #[test]
    fn same_primitive_can_register_under_distinct_names() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitive = primitive(50);

        let first = register_primitive(&mut words, &mut bindings, name("ALIAS_ONE"), primitive)
            .expect("first primitive name should register");
        let second = register_primitive(&mut words, &mut bindings, name("ALIAS_TWO"), primitive)
            .expect("second primitive name should register");

        assert_ne!(first, second);
        assert_eq!(words.len(), 2);
        assert_eq!(bindings.len(), 2);
        assert_word_binding(&bindings, "ALIAS_ONE", first);
        assert_word_binding(&bindings, "ALIAS_TWO", second);
        assert_primitive(&words, first, primitive);
        assert_primitive(&words, second, primitive);
    }

    #[test]
    fn registration_contract_does_not_require_vm_or_handler_table() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let id = register_primitive(
            &mut words,
            &mut bindings,
            name("VM_FREE_BOUNDARY"),
            primitive(60),
        )
        .expect("new primitive name should register");

        assert_word_binding(&bindings, "VM_FREE_BOUNDARY", id);
        assert_primitive(&words, id, primitive(60));
    }
}
