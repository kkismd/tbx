use crate::binding::{Binding, BindingInsertError, Bindings};
use crate::global_variable::{GlobalVarId, GlobalVariables};
use crate::name::NormalizedName;
use crate::source_word::{
    def_source_word, let_source_word, var_source_word, NativeSourceWordHandler, SourceWordId,
    SourceWordRegistry, SourceWordSyntaxMarker,
};
use crate::word::{CompletedWordDefinition, PrimitiveId, PublishedWords, WordId};

const BUILTIN_GLOBAL_VARIABLE_NAMES: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimitiveBootstrapError {
    NameConflict,
    ReservedName,
    BindingRegistrationInvariantViolated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinGlobalBootstrapError {
    NameConflict,
    ReservedName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceWordBootstrapError {
    NameConflict,
    ReservedName,
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
    bindings
        .validate_new_name(&name)
        .map_err(PrimitiveBootstrapError::from_precheck_error)?;

    let id = words.add(CompletedWordDefinition::primitive(primitive));

    bindings
        .insert_new(name, Binding::Word(id))
        .map_err(PrimitiveBootstrapError::from_binding_insert_error)?;

    Ok(id)
}

/// Registers the Tiny BASIC A-Z built-in scalar variables.
///
/// The A-Z variables are ordinary bindings in the shared namespace, backed by
/// session-owned global variable slots initialized by `GlobalVariables`.
/// Bootstrap checks every name before allocating storage so a conflict cannot
/// publish a partial prefix or leave unused variable slots behind.
pub(crate) fn register_builtin_global_variables(
    globals: &mut GlobalVariables,
    bindings: &mut Bindings,
) -> Result<Vec<GlobalVarId>, BuiltinGlobalBootstrapError> {
    let names = builtin_global_variable_names();

    for name in &names {
        bindings
            .validate_new_name(name)
            .map_err(BuiltinGlobalBootstrapError::from)?;
    }

    let mut ids = Vec::with_capacity(names.len());

    for name in names {
        let id = globals.allocate();
        bindings
            .insert_new(name, Binding::Variable(id))
            .expect("prechecked A-Z bootstrap names should remain available");
        ids.push(id);
    }

    Ok(ids)
}

/// Registers one native source word in the shared published name table.
///
/// Source word handlers are stored separately from executable word definitions
/// so a published source word cannot be reached through `WordId` or
/// `Instruction::Call`. As with primitive bootstrap, name conflicts are
/// prechecked before issuing the monotonic source-word ID.
pub(crate) fn register_native_source_word(
    source_words: &mut SourceWordRegistry,
    bindings: &mut Bindings,
    name: NormalizedName,
    handler: NativeSourceWordHandler,
) -> Result<SourceWordId, SourceWordBootstrapError> {
    register_native_source_word_with_markers(source_words, bindings, name, handler, Vec::new())
}

pub(crate) fn register_native_source_word_with_markers(
    source_words: &mut SourceWordRegistry,
    bindings: &mut Bindings,
    name: NormalizedName,
    handler: NativeSourceWordHandler,
    syntax_markers: Vec<SourceWordSyntaxMarker>,
) -> Result<SourceWordId, SourceWordBootstrapError> {
    let marker_names = syntax_markers
        .iter()
        .map(|marker| marker.name().clone())
        .collect::<Vec<_>>();

    bindings
        .validate_new_source_word_with_markers(&name, &marker_names)
        .map_err(SourceWordBootstrapError::from_precheck_error)?;

    let id = source_words.register_with_markers(handler, syntax_markers);

    bindings
        .insert_new_source_word_with_markers(name, id, &marker_names)
        .map_err(SourceWordBootstrapError::from_binding_insert_error)?;

    Ok(id)
}

pub(crate) fn register_builtin_source_words(
    source_words: &mut SourceWordRegistry,
    bindings: &mut Bindings,
) -> Result<BuiltinSourceWordIds, SourceWordBootstrapError> {
    // #1487 makes built-in source-word bindings the source of truth for name
    // occupation; bootstrap must fail rather than silently overwrite a binding.
    let var_name = builtin_name("VAR");
    let let_name = builtin_name("LET");
    let def_name = builtin_name("DEF");
    bindings
        .validate_new_name(&var_name)
        .map_err(SourceWordBootstrapError::from_precheck_error)?;
    bindings
        .validate_new_name(&let_name)
        .map_err(SourceWordBootstrapError::from_precheck_error)?;
    bindings
        .validate_new_name(&def_name)
        .map_err(SourceWordBootstrapError::from_precheck_error)?;

    let var = register_native_source_word(source_words, bindings, var_name, var_source_word)
        .expect("prechecked VAR source word should remain available");
    let let_ = register_native_source_word(source_words, bindings, let_name, let_source_word)
        .expect("prechecked LET source word should remain available");
    let def = register_native_source_word(source_words, bindings, def_name, def_source_word)
        .expect("prechecked DEF source word should remain available");

    Ok(BuiltinSourceWordIds { var, let_, def })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuiltinSourceWordIds {
    var: SourceWordId,
    let_: SourceWordId,
    def: SourceWordId,
}

impl BuiltinSourceWordIds {
    pub(crate) const fn var(self) -> SourceWordId {
        self.var
    }

    pub(crate) const fn let_(self) -> SourceWordId {
        self.let_
    }

    pub(crate) const fn def(self) -> SourceWordId {
        self.def
    }
}

fn builtin_global_variable_names() -> [NormalizedName; 26] {
    BUILTIN_GLOBAL_VARIABLE_NAMES.map(|name| {
        NormalizedName::new(name).expect("built-in global variable name should be valid")
    })
}

fn builtin_name(input: &str) -> NormalizedName {
    NormalizedName::new(input).expect("built-in source word name should be valid")
}

impl PrimitiveBootstrapError {
    fn from_precheck_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::NameConflict,
            BindingInsertError::ReservedName => Self::ReservedName,
        }
    }

    fn from_binding_insert_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::BindingRegistrationInvariantViolated,
            BindingInsertError::ReservedName => Self::BindingRegistrationInvariantViolated,
        }
    }
}

impl From<BindingInsertError> for BuiltinGlobalBootstrapError {
    fn from(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::NameConflict,
            BindingInsertError::ReservedName => Self::ReservedName,
        }
    }
}

impl SourceWordBootstrapError {
    fn from_precheck_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::NameConflict,
            BindingInsertError::ReservedName => Self::ReservedName,
        }
    }

    fn from_binding_insert_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::BindingRegistrationInvariantViolated,
            BindingInsertError::ReservedName => Self::BindingRegistrationInvariantViolated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_variable::GlobalVariables;
    use crate::source_word::{
        NativeSourceWordContext, SourceWordError, SourceWordSyntaxMarker,
        SourceWordSyntaxMarkerRole,
    };
    use crate::value::Value;
    use crate::word::WordDefinition;
    use std::collections::HashSet;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn primitive(slot: usize) -> PrimitiveId {
        PrimitiveId::from_slot(slot)
    }

    fn source_handler(
        _context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        Ok(())
    }

    fn marker(input: &str, role: SourceWordSyntaxMarkerRole) -> SourceWordSyntaxMarker {
        SourceWordSyntaxMarker::new(name(input), role)
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

    fn assert_variable_binding(bindings: &Bindings, input: &str, expected: GlobalVarId) {
        assert_eq!(
            bindings.get(&name(input)),
            Some(&Binding::Variable(expected))
        );
    }

    fn assert_source_word_binding(bindings: &Bindings, input: &str, expected: SourceWordId) {
        assert_eq!(
            bindings.get(&name(input)),
            Some(&Binding::SourceWord(expected))
        );
    }

    fn assert_distinct_variable_ids(ids: &[GlobalVarId]) {
        let distinct: HashSet<_> = ids.iter().copied().collect();

        assert_eq!(distinct.len(), ids.len());
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
    fn reserved_primitive_name_is_rejected_without_word_id() {
        for input in ["END", "end", "End"] {
            let mut words = PublishedWords::new();
            let mut bindings = Bindings::new();

            let result = register_primitive(&mut words, &mut bindings, name(input), primitive(21));

            assert_eq!(result, Err(PrimitiveBootstrapError::ReservedName));
            assert_eq!(words.len(), 0);
            assert!(bindings.is_empty());
        }
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

    #[test]
    fn empty_collections_accept_first_source_word() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();

        let id = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_TEST"),
            source_handler,
        )
        .expect("new source word name should register");

        assert_eq!(source_words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_source_word_binding(&bindings, "source_test", id);
    }

    #[test]
    fn source_word_registration_publishes_multiple_syntax_markers() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let markers = vec![
            marker("ELSIF", SourceWordSyntaxMarkerRole::BlockContinuation),
            marker("ELSE", SourceWordSyntaxMarkerRole::BlockContinuation),
            marker("ENDIF", SourceWordSyntaxMarkerRole::BlockTerminator),
        ];

        let id = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            markers,
        )
        .expect("source word with markers should register");

        assert_source_word_binding(&bindings, "if", id);
        assert_eq!(bindings.syntax_marker_reservation_len(), 3);
        assert_eq!(
            bindings
                .syntax_marker_reservation(&name("elsif"))
                .map(|reservation| reservation.owner()),
            Some(id)
        );
        assert_eq!(bindings.get(&name("ELSE")), None);
        let stored_markers = source_words
            .lookup()
            .syntax_markers(id)
            .expect("registered source word should have marker metadata");
        assert_eq!(stored_markers.len(), 3);
        assert_eq!(stored_markers[0].name(), &name("ELSIF"));
        assert_eq!(
            stored_markers[0].role(),
            SourceWordSyntaxMarkerRole::BlockContinuation
        );
        assert_eq!(stored_markers[2].name(), &name("ENDIF"));
        assert_eq!(
            stored_markers[2].role(),
            SourceWordSyntaxMarkerRole::BlockTerminator
        );
    }

    #[test]
    fn duplicate_source_word_registration_is_rejected_without_mutation() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let first = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("DUP_SOURCE"),
            source_handler,
        )
        .expect("first source word should register");

        let result = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("dup_source"),
            source_handler,
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_source_word_binding(&bindings, "DUP_SOURCE", first);
    }

    #[test]
    fn source_word_name_cannot_be_declared_as_own_marker() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();

        let result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![marker("if", SourceWordSyntaxMarkerRole::BlockTerminator)],
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert!(bindings.is_empty());
        assert_eq!(bindings.syntax_marker_reservation_len(), 0);
    }

    #[test]
    fn duplicate_marker_names_are_rejected_case_insensitively() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();

        let result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![
                marker("ELSE", SourceWordSyntaxMarkerRole::BlockContinuation),
                marker("else", SourceWordSyntaxMarkerRole::BlockTerminator),
            ],
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert!(bindings.is_empty());
        assert_eq!(bindings.syntax_marker_reservation_len(), 0);
    }

    #[test]
    fn reserved_source_word_name_is_rejected_without_source_word_id() {
        for input in ["END", "end", "End"] {
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();

            let result = register_native_source_word(
                &mut source_words,
                &mut bindings,
                name(input),
                source_handler,
            );

            assert_eq!(result, Err(SourceWordBootstrapError::ReservedName));
            assert_eq!(source_words.len(), 0);
            assert!(bindings.is_empty());
        }
    }

    #[test]
    fn reserved_marker_name_is_rejected_without_publication() {
        for input in ["END", "end", "End"] {
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();

            let result = register_native_source_word_with_markers(
                &mut source_words,
                &mut bindings,
                name("IF"),
                source_handler,
                vec![marker(input, SourceWordSyntaxMarkerRole::BlockTerminator)],
            );

            assert_eq!(result, Err(SourceWordBootstrapError::ReservedName));
            assert_eq!(source_words.len(), 0);
            assert!(bindings.is_empty());
            assert_eq!(bindings.syntax_marker_reservation_len(), 0);
        }
    }

    #[test]
    fn primitive_registration_conflicts_with_existing_source_word_binding() {
        let mut words = PublishedWords::new();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let source_word = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SHARED"),
            source_handler,
        )
        .expect("source word should register");

        let result = register_primitive(&mut words, &mut bindings, name("shared"), primitive(82));

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(words.len(), 0);
        assert_eq!(source_words.len(), 1);
        assert_source_word_binding(&bindings, "SHARED", source_word);
    }

    #[test]
    fn source_word_registration_conflicts_with_existing_runtime_word_binding() {
        let mut words = PublishedWords::new();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let word = register_primitive(&mut words, &mut bindings, name("SHARED"), primitive(83))
            .expect("primitive should register");

        let result = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("shared"),
            source_handler,
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert_word_binding(&bindings, "SHARED", word);
    }

    #[test]
    fn source_word_marker_conflicts_with_existing_runtime_word_binding() {
        let mut words = PublishedWords::new();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let word = register_primitive(&mut words, &mut bindings, name("ELSE"), primitive(84))
            .expect("primitive should register");

        let result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![marker(
                "else",
                SourceWordSyntaxMarkerRole::BlockContinuation,
            )],
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert_eq!(bindings.syntax_marker_reservation_len(), 0);
        assert_word_binding(&bindings, "ELSE", word);
    }

    #[test]
    fn source_word_registration_conflicts_with_existing_variable_binding() {
        let mut globals = GlobalVariables::new();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let variable = globals.allocate();
        bindings
            .insert_new(name("A"), Binding::Variable(variable))
            .expect("test variable should register");

        let result = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("a"),
            source_handler,
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert_variable_binding(&bindings, "A", variable);
    }

    #[test]
    fn source_word_marker_conflicts_with_existing_source_word_binding() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let existing = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("ELSE"),
            source_handler,
        )
        .expect("source word should register");

        let result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![marker(
                "else",
                SourceWordSyntaxMarkerRole::BlockContinuation,
            )],
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 1);
        assert_eq!(bindings.syntax_marker_reservation_len(), 0);
        assert_source_word_binding(&bindings, "ELSE", existing);
    }

    #[test]
    fn source_word_marker_conflicts_with_existing_variable_binding() {
        let mut globals = GlobalVariables::new();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let variable = globals.allocate();
        bindings
            .insert_new(name("ELSE"), Binding::Variable(variable))
            .expect("test variable should register");

        let result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![marker(
                "else",
                SourceWordSyntaxMarkerRole::BlockContinuation,
            )],
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert_eq!(bindings.syntax_marker_reservation_len(), 0);
        assert_variable_binding(&bindings, "ELSE", variable);
    }

    #[test]
    fn marker_registration_conflict_does_not_publish_prefix() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        bindings
            .insert_new(name("ENDIF"), Binding::Variable(globals.allocate()))
            .expect("test conflict should register");

        let result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![
                marker("ELSIF", SourceWordSyntaxMarkerRole::BlockContinuation),
                marker("ENDIF", SourceWordSyntaxMarkerRole::BlockTerminator),
            ],
        );

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 0);
        assert_eq!(bindings.get(&name("IF")), None);
        assert_eq!(bindings.syntax_marker_reservation(&name("ELSIF")), None);
        assert_eq!(bindings.syntax_marker_reservation_len(), 0);
    }

    #[test]
    fn existing_marker_reservation_rejects_later_source_word_name_and_marker() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let owner = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![marker("ENDIF", SourceWordSyntaxMarkerRole::BlockTerminator)],
        )
        .expect("owner source word should register");

        let source_name_result = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("endif"),
            source_handler,
        );
        let marker_result = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("WHILE"),
            source_handler,
            vec![marker("endif", SourceWordSyntaxMarkerRole::BlockTerminator)],
        );

        assert_eq!(
            source_name_result,
            Err(SourceWordBootstrapError::NameConflict)
        );
        assert_eq!(marker_result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 1);
        assert_source_word_binding(&bindings, "IF", owner);
        assert_eq!(
            bindings
                .syntax_marker_reservation(&name("ENDIF"))
                .map(|reservation| reservation.owner()),
            Some(owner)
        );
    }

    #[test]
    fn marker_reservation_rejects_later_runtime_word_and_variable_publication() {
        let mut words = PublishedWords::new();
        let mut globals = GlobalVariables::new();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let owner = register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("IF"),
            source_handler,
            vec![marker("ENDIF", SourceWordSyntaxMarkerRole::BlockTerminator)],
        )
        .expect("owner source word should register");
        let variable = globals.allocate();

        let runtime_result =
            register_primitive(&mut words, &mut bindings, name("endif"), primitive(85));
        let variable_result = bindings.insert_new(name("endif"), Binding::Variable(variable));

        assert_eq!(runtime_result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(variable_result, Err(BindingInsertError::NameConflict));
        assert_eq!(words.len(), 0);
        assert_eq!(bindings.len(), 1);
        assert_source_word_binding(&bindings, "IF", owner);
        assert_eq!(bindings.get(&name("ENDIF")), None);
    }

    #[test]
    fn builtin_source_word_bootstrap_publishes_var_as_source_word_binding() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();

        let ids = register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("empty namespace should accept built-in source words");

        assert_eq!(source_words.len(), 3);
        assert_source_word_binding(&bindings, "VAR", ids.var());
        assert_source_word_binding(&bindings, "var", ids.var());
        assert_source_word_binding(&bindings, "LET", ids.let_());
        assert_source_word_binding(&bindings, "let", ids.let_());
        assert_source_word_binding(&bindings, "DEF", ids.def());
        assert_source_word_binding(&bindings, "def", ids.def());
    }

    #[test]
    fn builtin_source_word_bootstrap_conflicts_without_overwriting_binding() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let existing = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("var"),
            source_handler,
        )
        .expect("test source word should register");

        let result = register_builtin_source_words(&mut source_words, &mut bindings);

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 1);
        assert_source_word_binding(&bindings, "VAR", existing);
    }

    #[test]
    fn builtin_source_word_bootstrap_let_conflict_does_not_publish_var_prefix() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let existing = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("let"),
            source_handler,
        )
        .expect("test LET source word should register");

        let result = register_builtin_source_words(&mut source_words, &mut bindings);

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 1);
        assert_eq!(bindings.get(&name("VAR")), None);
        assert_source_word_binding(&bindings, "LET", existing);
    }

    #[test]
    fn builtin_source_word_bootstrap_def_conflict_does_not_publish_prefix() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let existing = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("def"),
            source_handler,
        )
        .expect("test DEF source word should register");

        let result = register_builtin_source_words(&mut source_words, &mut bindings);

        assert_eq!(result, Err(SourceWordBootstrapError::NameConflict));
        assert_eq!(source_words.len(), 1);
        assert_eq!(bindings.get(&name("VAR")), None);
        assert_eq!(bindings.get(&name("LET")), None);
        assert_source_word_binding(&bindings, "DEF", existing);
    }

    #[test]
    fn builtin_global_bootstrap_registers_a_to_z_variables() {
        let mut globals = GlobalVariables::new();
        let mut bindings = Bindings::new();

        let ids = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("empty namespace should accept built-in globals");

        assert_eq!(ids.len(), 26);
        assert_distinct_variable_ids(&ids);
        assert_eq!(globals.len(), 26);
        assert_eq!(bindings.len(), 26);

        for (index, letter) in BUILTIN_GLOBAL_VARIABLE_NAMES.iter().enumerate() {
            let id = ids[index];
            assert_variable_binding(&bindings, letter, id);
            assert_eq!(globals.view().read(id), Ok(Value::integer(0)));
        }
    }

    #[test]
    fn builtin_global_bootstrap_uses_case_insensitive_lookup() {
        let mut globals = GlobalVariables::new();
        let mut bindings = Bindings::new();

        let ids = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("empty namespace should accept built-in globals");

        assert_variable_binding(&bindings, "a", ids[0]);
        assert_variable_binding(&bindings, "A", ids[0]);
        assert_variable_binding(&bindings, "z", ids[25]);
        assert_variable_binding(&bindings, "Z", ids[25]);
    }

    #[test]
    fn builtin_global_bootstrap_duplicate_run_preserves_existing_variables() {
        let mut globals = GlobalVariables::new();
        let mut bindings = Bindings::new();
        let ids = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("first A-Z bootstrap should register");

        let result = register_builtin_global_variables(&mut globals, &mut bindings);

        assert_eq!(result, Err(BuiltinGlobalBootstrapError::NameConflict));
        assert_eq!(globals.len(), 26);
        assert_eq!(bindings.len(), 26);
        assert_distinct_variable_ids(&ids);

        for (index, letter) in BUILTIN_GLOBAL_VARIABLE_NAMES.iter().enumerate() {
            assert_variable_binding(&bindings, letter, ids[index]);
            assert_eq!(globals.view().read(ids[index]), Ok(Value::integer(0)));
        }
    }

    #[test]
    fn builtin_global_bootstrap_conflicts_with_existing_word_binding() {
        let mut words = PublishedWords::new();
        let mut globals = GlobalVariables::new();
        let mut bindings = Bindings::new();
        let existing = words.add(CompletedWordDefinition::primitive(primitive(70)));
        bindings
            .insert_new(name("A"), Binding::Word(existing))
            .expect("test word should register");

        let result = register_builtin_global_variables(&mut globals, &mut bindings);

        assert_eq!(result, Err(BuiltinGlobalBootstrapError::NameConflict));
        assert_eq!(globals.len(), 0);
        assert_eq!(bindings.len(), 1);
        assert_word_binding(&bindings, "a", existing);
    }

    #[test]
    fn builtin_global_bootstrap_conflicts_with_existing_case_variant() {
        let mut globals = GlobalVariables::new();
        let existing = globals.allocate();
        let mut bindings = Bindings::new();
        bindings
            .insert_new(name("m"), Binding::Variable(existing))
            .expect("test variable should register");

        let result = register_builtin_global_variables(&mut globals, &mut bindings);

        assert_eq!(result, Err(BuiltinGlobalBootstrapError::NameConflict));
        assert_eq!(globals.len(), 1);
        assert_eq!(globals.view().read(existing), Ok(Value::integer(0)));
        assert_eq!(bindings.len(), 1);
        assert_variable_binding(&bindings, "M", existing);
    }

    #[test]
    fn builtin_global_bootstrap_conflict_after_prefix_does_not_publish_partial_names() {
        let mut globals = GlobalVariables::new();
        let existing = globals.allocate();
        let mut bindings = Bindings::new();
        bindings
            .insert_new(name("Z"), Binding::Variable(existing))
            .expect("test variable should register");

        let result = register_builtin_global_variables(&mut globals, &mut bindings);

        assert_eq!(result, Err(BuiltinGlobalBootstrapError::NameConflict));
        assert_eq!(globals.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.get(&name("A")), None);
        assert_eq!(bindings.get(&name("Y")), None);
        assert_variable_binding(&bindings, "z", existing);
    }
}
