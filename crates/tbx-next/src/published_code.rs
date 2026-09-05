use crate::binding::{Binding, BindingInsertError, BindingReplaceError, Bindings};
use crate::block_code::{BlockCodeBuildError, BlockCodeBuilder};
use crate::instruction::{
    CodeLocation, Instruction, InstructionAddress, InstructionAddressError, InstructionView,
};
use crate::name::NormalizedName;
use crate::redefinition::{redefine_word, WordRedefinition, WordRedefinitionError};
use crate::source::SourceSpan;
use crate::source_mapping::{InstructionSourceMappingView, SourceMappedCode};
use crate::word::{CompletedWordDefinition, PublishedWords, WordDefinitionError, WordId};

/// Session-owned code space for all published compiled runtime words.
///
/// Published word bodies append into one durable instruction owner. Failed
/// builds may leave unreachable fragments behind, but only a completed entry
/// can be converted into a `WordId` through the publication coordinator below.
#[derive(Debug, Default)]
pub(crate) struct PublishedCode {
    code: SourceMappedCode,
}

#[derive(Debug)]
pub(crate) struct PublishedWordBuilder<'a> {
    block: BlockCodeBuilder<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PublishedWordEntry {
    location: CodeLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublishedWord {
    id: WordId,
    entry: CodeLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordBodyBuildError {
    SourceMappingAppend {
        source: crate::source_mapping::SourceMappingAppendError,
    },
    BranchTargetPatch {
        source: crate::instruction::BranchTargetPatchError,
    },
    BranchInstructionRequiresPatch {
        instruction: Instruction,
    },
    AddressOutsideCurrentBody {
        address: InstructionAddress,
    },
    UnknownBranchPatch {
        branch: InstructionAddress,
    },
    UnresolvedBranchPatch {
        branch: InstructionAddress,
    },
    DefinitionBodyCompileRejected,
    InvalidEntry {
        source: InstructionAddressError,
    },
    #[cfg(test)]
    BodyRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewWordPublicationError {
    NameConflict,
    ReservedName,
    Build { source: WordBodyBuildError },
    Definition { source: WordDefinitionError },
    BindingCommitInvariantViolated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordRepublicationError {
    UndefinedName,
    TargetIsNotWord,
    Build { source: WordBodyBuildError },
    Definition { source: WordDefinitionError },
    BindingCommitInvariantViolated,
}

impl PublishedCode {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn publish_new_word(
        &mut self,
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        name: NormalizedName,
        build: impl FnOnce(&Bindings, &mut PublishedWordBuilder<'_>) -> Result<(), WordBodyBuildError>,
    ) -> Result<PublishedWord, NewWordPublicationError> {
        bindings
            .validate_new_name(&name)
            .map_err(NewWordPublicationError::from_precheck_error)?;

        let entry = self
            .build_word_body(|builder| build(bindings, builder))
            .map_err(|source| NewWordPublicationError::Build { source })?;
        let definition = CompletedWordDefinition::compiled(entry.location, self.instruction_view())
            .map_err(|source| NewWordPublicationError::Definition { source })?;
        let id = words.add(definition);

        bindings
            .insert_new(name, Binding::Word(id))
            .map_err(|_| NewWordPublicationError::BindingCommitInvariantViolated)?;

        Ok(PublishedWord {
            id,
            entry: entry.location,
        })
    }

    pub(crate) fn redefine_word(
        &mut self,
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        name: &NormalizedName,
        build: impl FnOnce(&mut PublishedWordBuilder<'_>) -> Result<(), WordBodyBuildError>,
    ) -> Result<WordRedefinition, WordRepublicationError> {
        bindings
            .current_word(name)
            .map_err(WordRepublicationError::from_precheck_error)?;

        let entry = self
            .build_word_body(build)
            .map_err(|source| WordRepublicationError::Build { source })?;
        let definition = CompletedWordDefinition::compiled(entry.location, self.instruction_view())
            .map_err(|source| WordRepublicationError::Definition { source })?;

        redefine_word(words, bindings, name, definition).map_err(WordRepublicationError::from)
    }

    pub(crate) fn instruction_view(&self) -> InstructionView<'_> {
        self.code.instruction_view()
    }

    pub(crate) fn source_mapping(&self) -> InstructionSourceMappingView<'_> {
        self.code.source_mapping()
    }

    pub(crate) fn len(&self) -> usize {
        self.code.len()
    }

    fn build_word_body(
        &mut self,
        build: impl FnOnce(&mut PublishedWordBuilder<'_>) -> Result<(), WordBodyBuildError>,
    ) -> Result<PublishedWordEntry, WordBodyBuildError> {
        let entry_address = InstructionAddress::from_index(self.code.len());

        let mut builder = PublishedWordBuilder {
            block: BlockCodeBuilder::new(&mut self.code),
        };
        build(&mut builder)?;
        builder.finish()?;

        self.code
            .validate_address(entry_address)
            .map_err(|source| WordBodyBuildError::InvalidEntry { source })?;

        Ok(PublishedWordEntry {
            location: self.code.instruction_view().location(entry_address),
        })
    }

    #[cfg(test)]
    pub(crate) fn test_build_word_body<E>(
        &mut self,
        build: impl FnOnce(&mut PublishedWordBuilder<'_>) -> Result<(), E>,
    ) -> Result<(), E>
    where
        E: From<WordBodyBuildError>,
    {
        let mut builder = PublishedWordBuilder {
            block: BlockCodeBuilder::new(&mut self.code),
        };
        build(&mut builder)?;
        builder.finish().map_err(E::from)
    }
}

impl NewWordPublicationError {
    fn from_precheck_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::NameConflict,
            BindingInsertError::ReservedName => Self::ReservedName,
        }
    }
}

impl PublishedWord {
    pub(crate) const fn id(self) -> WordId {
        self.id
    }

    pub(crate) const fn entry(self) -> CodeLocation {
        self.entry
    }
}

impl PublishedWordBuilder<'_> {
    pub(crate) fn current_address(&self) -> InstructionAddress {
        self.block.current_address()
    }

    pub(crate) fn current_len(&self) -> usize {
        self.block.current_len()
    }

    pub(crate) fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_mapped(instruction, span)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_unmapped(instruction)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_resolved_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_resolved_mapped(instruction, span)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_resolved_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_resolved_unmapped(instruction)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_mapped_jump_placeholder(span)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_unmapped_jump_placeholder(
        &mut self,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_unmapped_jump_placeholder()
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_mapped_jump_if_zero_placeholder(span)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn append_unmapped_jump_if_zero_placeholder(
        &mut self,
    ) -> Result<InstructionAddress, WordBodyBuildError> {
        self.block
            .append_unmapped_jump_if_zero_placeholder()
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), WordBodyBuildError> {
        self.block
            .patch_branch_target(branch, target)
            .map_err(WordBodyBuildError::from)
    }

    pub(crate) fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), WordBodyBuildError> {
        self.block
            .validate_local_target(address)
            .map_err(WordBodyBuildError::from)
    }

    fn finish(self) -> Result<(), WordBodyBuildError> {
        self.block
            .finish()
            .map(|_| ())
            .map_err(WordBodyBuildError::from)
    }
}

impl From<BlockCodeBuildError> for WordBodyBuildError {
    fn from(error: BlockCodeBuildError) -> Self {
        match error {
            BlockCodeBuildError::SourceMappingAppend { source } => {
                Self::SourceMappingAppend { source }
            }
            BlockCodeBuildError::BranchTargetPatch { source } => Self::BranchTargetPatch { source },
            BlockCodeBuildError::BranchInstructionRequiresPatch { instruction } => {
                Self::BranchInstructionRequiresPatch { instruction }
            }
            BlockCodeBuildError::AddressOutsideCurrentBlock { address } => {
                Self::AddressOutsideCurrentBody { address }
            }
            BlockCodeBuildError::UnknownBranchPatch { branch } => {
                Self::UnknownBranchPatch { branch }
            }
            BlockCodeBuildError::UnresolvedBranchPatch { branch } => {
                Self::UnresolvedBranchPatch { branch }
            }
        }
    }
}

impl WordRepublicationError {
    fn from_precheck_error(error: BindingReplaceError) -> Self {
        match error {
            BindingReplaceError::MissingName => Self::UndefinedName,
            BindingReplaceError::TargetIsNotWord => Self::TargetIsNotWord,
            BindingReplaceError::CurrentWordMismatch { .. } => Self::BindingCommitInvariantViolated,
        }
    }
}

impl From<WordRedefinitionError> for WordRepublicationError {
    fn from(error: WordRedefinitionError) -> Self {
        match error {
            WordRedefinitionError::UndefinedName => Self::BindingCommitInvariantViolated,
            WordRedefinitionError::TargetIsNotWord => Self::BindingCommitInvariantViolated,
            WordRedefinitionError::BindingCommitInvariantViolated => {
                Self::BindingCommitInvariantViolated
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_variable::GlobalVariables;
    use crate::instruction::{InstructionLookup, InstructionSequence};
    use crate::source::{SourceId, SourceTexts, SourceView};
    use crate::source_word::{NativeSourceWordContext, SourceWordError, SourceWordRegistry};
    use crate::value::Value;
    use crate::word::{PrimitiveId, WordDefinition};

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text, "test.tbx");
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn push(value: i16) -> Instruction {
        Instruction::Push(Value::integer(value))
    }

    fn publish_push(
        code: &mut PublishedCode,
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        input: &str,
        value: i16,
    ) -> PublishedWord {
        code.publish_new_word(words, bindings, name(input), |_, builder| {
            builder.append_unmapped(push(value))?;
            builder.append_unmapped(Instruction::Return)?;
            Ok(())
        })
        .expect("test word should publish")
    }

    fn source_word_binding() -> Binding {
        fn handler(_context: &mut NativeSourceWordContext<'_, '_>) -> Result<(), SourceWordError> {
            Ok(())
        }

        let mut registry = SourceWordRegistry::new();
        Binding::SourceWord(registry.register(handler))
    }

    fn variable_binding() -> Binding {
        let mut globals = GlobalVariables::new();
        Binding::Variable(globals.allocate())
    }

    fn primitive_word(words: &mut PublishedWords) -> Binding {
        Binding::Word(
            words.add(CompletedWordDefinition::primitive(PrimitiveId::from_slot(
                99,
            ))),
        )
    }

    #[test]
    fn first_and_second_words_use_current_published_code_tail_as_entry() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let first = publish_push(&mut code, &mut words, &mut bindings, "FIRST", 10);
        let second = publish_push(&mut code, &mut words, &mut bindings, "SECOND", 20);

        assert_eq!(first.entry().address(), InstructionAddress::from_index(0));
        assert_eq!(second.entry().address(), InstructionAddress::from_index(2));
        assert_eq!(first.entry().code_space(), second.entry().code_space());
        assert_eq!(code.len(), 4);
        assert_eq!(
            code.instruction_view().get_location(first.entry()),
            Ok(&push(10))
        );
        assert_eq!(
            code.instruction_view().get_location(second.entry()),
            Ok(&push(20))
        );
    }

    #[test]
    fn mapped_and_unmapped_instructions_keep_mapping_entries() {
        let (sources, source_id) = source("WORD BODY");
        let first_span = span(sources.view(), source_id, 0, 4);
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let word = code
            .publish_new_word(&mut words, &mut bindings, name("MAPPED"), |_, builder| {
                builder.append_mapped(push(1), first_span)?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("mapped word should publish");
        let return_location = code
            .instruction_view()
            .location(InstructionAddress::from_index(1));

        assert_eq!(
            code.source_mapping().source_span(word.entry()),
            Ok(Some(first_span))
        );
        assert_eq!(code.source_mapping().source_span(return_location), Ok(None));
    }

    #[test]
    fn build_failure_does_not_publish_word_or_binding_and_leaves_fragment() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let result =
            code.publish_new_word(&mut words, &mut bindings, name("BROKEN"), |_, builder| {
                builder.append_unmapped(push(1))?;
                Err(WordBodyBuildError::BodyRejected)
            });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::BodyRejected
            })
        );
        assert_eq!(code.len(), 1);
        assert_eq!(words.len(), 0);
        assert!(bindings.get(&name("BROKEN")).is_none());

        let after_failure = publish_push(&mut code, &mut words, &mut bindings, "NEXT", 2);
        assert_eq!(
            after_failure.entry().address(),
            InstructionAddress::from_index(1)
        );
        assert_eq!(words.len(), 1);
    }

    #[test]
    fn empty_successful_body_is_rejected_before_word_id_is_issued() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let result = code.publish_new_word(&mut words, &mut bindings, name("EMPTY"), |_, _| Ok(()));

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::InvalidEntry {
                    source: InstructionAddressError::EndAddress {
                        address: InstructionAddress::from_index(0)
                    }
                }
            })
        );
        assert_eq!(words.len(), 0);
        assert!(bindings.is_empty());
    }

    #[test]
    fn new_word_publication_commits_matching_word_id_to_words_and_binding() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let word = publish_push(&mut code, &mut words, &mut bindings, "FOO", 7);

        assert_eq!(bindings.get(&name("foo")), Some(&Binding::Word(word.id())));
        assert_eq!(
            words.get(word.id()),
            Ok(&WordDefinition::Compiled {
                entry: word.entry()
            })
        );
    }

    #[test]
    fn name_conflicts_are_rejected_before_building_code() {
        for existing in [
            primitive_word(&mut PublishedWords::new()),
            source_word_binding(),
            variable_binding(),
        ] {
            let mut code = PublishedCode::new();
            let mut words = PublishedWords::new();
            let mut bindings = Bindings::new();
            bindings
                .insert_new(name("TAKEN"), existing)
                .expect("test binding should register");

            let result =
                code.publish_new_word(&mut words, &mut bindings, name("TAKEN"), |_, builder| {
                    builder.append_unmapped(push(1))?;
                    Ok(())
                });

            assert_eq!(result, Err(NewWordPublicationError::NameConflict));
            assert_eq!(code.len(), 0);
            assert_eq!(words.len(), 0);
            assert_eq!(bindings.get(&name("TAKEN")), Some(&existing));
        }
    }

    #[test]
    fn reserved_name_is_rejected_before_building_code() {
        for input in ["REM", "rem", "Rem"] {
            let mut code = PublishedCode::new();
            let mut words = PublishedWords::new();
            let mut bindings = Bindings::new();
            let mut build_called = false;

            let result =
                code.publish_new_word(&mut words, &mut bindings, name(input), |_, builder| {
                    build_called = true;
                    builder.append_unmapped(push(1))?;
                    Ok(())
                });

            assert_eq!(result, Err(NewWordPublicationError::ReservedName));
            assert!(!build_called);
            assert_eq!(code.len(), 0);
            assert_eq!(words.len(), 0);
            assert!(bindings.is_empty());
        }
    }

    #[test]
    fn redefinition_appends_new_word_id_and_moves_only_current_binding() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = publish_push(&mut code, &mut words, &mut bindings, "TARGET", 1);

        let result = code
            .redefine_word(&mut words, &mut bindings, &name("target"), |builder| {
                builder.append_unmapped(push(2))?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("runtime word should redefine");
        let new_entry = match words.get(result.current()).expect("new id should resolve") {
            WordDefinition::Compiled { entry } => *entry,
            WordDefinition::Primitive { .. } => panic!("new definition should be compiled"),
        };

        assert_eq!(result.previous(), old.id());
        assert_ne!(result.previous(), result.current());
        assert_eq!(
            bindings.get(&name("TARGET")),
            Some(&Binding::Word(result.current()))
        );
        assert_eq!(
            words.get(result.previous()),
            Ok(&WordDefinition::Compiled { entry: old.entry() })
        );
        assert_eq!(old.entry().code_space(), new_entry.code_space());
        assert_ne!(old.entry().address(), new_entry.address());
        assert_eq!(
            code.instruction_view().get_location(old.entry()),
            Ok(&push(1))
        );
        assert_eq!(
            code.instruction_view().get_location(new_entry),
            Ok(&push(2))
        );
    }

    #[test]
    fn invalid_redefinition_targets_are_rejected_before_building_code() {
        for (binding, expected) in [
            (None, WordRepublicationError::UndefinedName),
            (
                Some(source_word_binding()),
                WordRepublicationError::TargetIsNotWord,
            ),
            (
                Some(variable_binding()),
                WordRepublicationError::TargetIsNotWord,
            ),
        ] {
            let mut code = PublishedCode::new();
            let mut words = PublishedWords::new();
            let mut bindings = Bindings::new();
            if let Some(binding) = binding {
                bindings
                    .insert_new(name("TARGET"), binding)
                    .expect("test binding should register");
            }

            let result =
                code.redefine_word(&mut words, &mut bindings, &name("TARGET"), |builder| {
                    builder.append_unmapped(push(1))?;
                    Ok(())
                });

            assert_eq!(result, Err(expected));
            assert_eq!(code.len(), 0);
            assert_eq!(words.len(), 0);
        }
    }

    #[test]
    fn failed_redefinition_keeps_old_definition_binding_and_mapping() {
        let (sources, source_id) = source("OLD NEW");
        let old_span = span(sources.view(), source_id, 0, 3);
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = code
            .publish_new_word(&mut words, &mut bindings, name("TARGET"), |_, builder| {
                builder.append_mapped(push(1), old_span)?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("old word should publish");

        let result = code.redefine_word(&mut words, &mut bindings, &name("TARGET"), |builder| {
            builder.append_unmapped(push(2))?;
            Err(WordBodyBuildError::BodyRejected)
        });

        assert_eq!(
            result,
            Err(WordRepublicationError::Build {
                source: WordBodyBuildError::BodyRejected
            })
        );
        assert_eq!(code.len(), 3);
        assert_eq!(words.len(), 1);
        assert_eq!(
            bindings.get(&name("TARGET")),
            Some(&Binding::Word(old.id()))
        );
        assert_eq!(
            words.get(old.id()),
            Ok(&WordDefinition::Compiled { entry: old.entry() })
        );
        assert_eq!(
            code.source_mapping().source_span(old.entry()),
            Ok(Some(old_span))
        );
    }

    #[test]
    fn early_bound_call_keeps_old_word_id_after_redefinition() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = publish_push(&mut code, &mut words, &mut bindings, "TARGET", 1);
        let caller = code
            .publish_new_word(&mut words, &mut bindings, name("CALLER"), |_, builder| {
                builder.append_unmapped(Instruction::Call(old.id()))?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("caller should publish");

        let result = code
            .redefine_word(&mut words, &mut bindings, &name("TARGET"), |builder| {
                builder.append_unmapped(push(2))?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("target should redefine");

        assert_ne!(old.id(), result.current());
        assert_eq!(
            code.instruction_view().get_location(caller.entry()),
            Ok(&Instruction::Call(old.id()))
        );
    }

    #[test]
    fn branch_patches_complete_inside_the_builder_boundary() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let word = code
            .publish_new_word(&mut words, &mut bindings, name("BRANCH"), |_, builder| {
                let branch = builder.append_unmapped_jump_placeholder()?;
                let target = builder.append_unmapped(Instruction::Return)?;
                builder.patch_branch_target(branch, target)?;
                Ok(())
            })
            .expect("branch word should publish");

        assert_eq!(
            code.instruction_view().get_location(word.entry()),
            Ok(&Instruction::Jump(InstructionAddress::from_index(1)))
        );
    }

    #[test]
    fn direct_branch_append_is_rejected_before_publication() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let branch = Instruction::Jump(InstructionAddress::from_index(0));

        let result =
            code.publish_new_word(&mut words, &mut bindings, name("BRANCH"), |_, builder| {
                builder.append_unmapped(branch)?;
                Ok(())
            });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::BranchInstructionRequiresPatch {
                    instruction: branch
                }
            })
        );
        assert_eq!(code.len(), 0);
        assert_eq!(words.len(), 0);
        assert!(bindings.is_empty());
    }

    #[test]
    fn unresolved_branch_patch_rejects_successful_publication() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let result =
            code.publish_new_word(&mut words, &mut bindings, name("BROKEN"), |_, builder| {
                let _branch = builder.append_unmapped_jump_placeholder()?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::UnresolvedBranchPatch {
                    branch: InstructionAddress::from_index(0)
                }
            })
        );
        assert_eq!(code.len(), 2);
        assert_eq!(words.len(), 0);
        assert!(bindings.is_empty());
    }

    #[test]
    fn later_build_cannot_patch_branch_from_previous_word() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = code
            .publish_new_word(&mut words, &mut bindings, name("OLD"), |_, builder| {
                let branch = builder.append_unmapped_jump_placeholder()?;
                let target = builder.append_unmapped(Instruction::Return)?;
                builder.patch_branch_target(branch, target)?;
                Ok(())
            })
            .expect("old word should publish");

        let result = code.publish_new_word(&mut words, &mut bindings, name("NEW"), |_, builder| {
            let target = builder.append_unmapped(Instruction::Return)?;
            builder.patch_branch_target(old.entry().address(), target)?;
            Ok(())
        });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::AddressOutsideCurrentBody {
                    address: old.entry().address()
                }
            })
        );
        assert_eq!(
            code.instruction_view().get_location(old.entry()),
            Ok(&Instruction::Jump(InstructionAddress::from_index(1)))
        );
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.get(&name("OLD")), Some(&Binding::Word(old.id())));
        assert!(bindings.get(&name("NEW")).is_none());
    }

    #[test]
    fn later_build_cannot_patch_branch_to_previous_word_target() {
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = publish_push(&mut code, &mut words, &mut bindings, "OLD", 1);

        let result = code.publish_new_word(&mut words, &mut bindings, name("NEW"), |_, builder| {
            let branch = builder.append_unmapped_jump_placeholder()?;
            builder.patch_branch_target(branch, old.entry().address())?;
            Ok(())
        });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::AddressOutsideCurrentBody {
                    address: old.entry().address()
                }
            })
        );
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.get(&name("OLD")), Some(&Binding::Word(old.id())));
        assert!(bindings.get(&name("NEW")).is_none());
    }

    #[test]
    fn read_only_views_validate_published_instruction_and_mapping() {
        let (sources, source_id) = source("A B");
        let old_span = span(sources.view(), source_id, 0, 1);
        let new_span = span(sources.view(), source_id, 2, 3);
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = code
            .publish_new_word(&mut words, &mut bindings, name("TARGET"), |_, builder| {
                builder.append_mapped(push(1), old_span)?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("old word should publish");
        let result = code
            .redefine_word(&mut words, &mut bindings, &name("TARGET"), |builder| {
                builder.append_mapped(push(2), new_span)?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("target should redefine");
        let new_entry = match words.get(result.current()).expect("new id should resolve") {
            WordDefinition::Compiled { entry } => *entry,
            WordDefinition::Primitive { .. } => panic!("new definition should be compiled"),
        };

        let view = code.instruction_view();
        let mapping = code.source_mapping();

        assert_eq!(
            view.validate_location(old.entry()),
            Ok(old.entry().address())
        );
        assert_eq!(view.validate_location(new_entry), Ok(new_entry.address()));
        assert_eq!(mapping.source_span(old.entry()), Ok(Some(old_span)));
        assert_eq!(mapping.source_span(new_entry), Ok(Some(new_span)));
    }

    #[test]
    fn published_code_space_stays_distinct_from_temporary_execution_code() {
        let mut published = PublishedCode::new();
        let mut temporary = InstructionSequence::new();
        let temporary_entry = temporary.append(Instruction::Halt);
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let word = publish_push(&mut published, &mut words, &mut bindings, "TARGET", 1);

        assert_ne!(
            published.instruction_view().code_space(),
            temporary.view().code_space()
        );
        let lookup = InstructionLookup::from(published.instruction_view());
        assert!(matches!(
            lookup.view_for(temporary.view().location(temporary_entry).code_space()),
            Err(crate::instruction::CodeSpaceLookupError::UnknownCodeSpace { code_space })
                if code_space == temporary.view().code_space()
        ));
        assert_eq!(
            published.instruction_view().get_location(word.entry()),
            Ok(&push(1))
        );
    }
}
