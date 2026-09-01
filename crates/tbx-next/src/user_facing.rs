use crate::diagnostic::UserDiagnostic;
use crate::lexer::LexError;
use crate::primitive::PrimitiveError;
use crate::source::{SourceError, SourceSpan, SourceView};
use crate::source_mapping::SourceMappingLookupError;
use crate::source_processor::{RuntimeError, SourceProcessorError, SourceRunResult};
use crate::vm::VmError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UserFacingRunResult {
    Success(SourceRunResult),
    Failure(Box<UserFacingFailure>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserFacingFailureClass {
    UserProgram,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UserFacingFailure {
    class: UserFacingFailureClass,
    diagnostic: UserDiagnostic,
    original_error: SourceProcessorError,
    diagnostic_failure: Option<UserFacingDiagnosticFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserFacingDiagnosticFailure {
    SourceMappingLookup {
        source: SourceMappingLookupError,
        original_runtime: VmError,
    },
    SourceLookup {
        source: SourceError,
    },
}

impl UserFacingRunResult {
    pub(crate) fn from_source_result(
        sources: SourceView<'_>,
        result: Result<SourceRunResult, SourceProcessorError>,
    ) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(error) => Self::Failure(Box::new(UserFacingFailure::from_source_error(
                sources, error,
            ))),
        }
    }
}

impl UserFacingFailure {
    fn from_source_error(sources: SourceView<'_>, error: SourceProcessorError) -> Self {
        let mut class = classify_source_error(&error);
        let mut diagnostic_failure = runtime_mapping_failure(&error);
        if diagnostic_failure.is_some() {
            class = UserFacingFailureClass::Environment;
        }

        if diagnostic_failure.is_none() {
            if let Some(span) = error.primary_span() {
                if let Err(source) = validate_diagnostic_span(sources, span) {
                    diagnostic_failure = Some(UserFacingDiagnosticFailure::SourceLookup { source });
                    class = UserFacingFailureClass::Environment;
                }
            }
        }

        let diagnostic = build_diagnostic(&error, diagnostic_failure);

        Self {
            class,
            diagnostic,
            original_error: error,
            diagnostic_failure,
        }
    }

    pub(crate) const fn class(&self) -> UserFacingFailureClass {
        self.class
    }

    pub(crate) const fn diagnostic(&self) -> &UserDiagnostic {
        &self.diagnostic
    }

    pub(crate) const fn original_error(&self) -> &SourceProcessorError {
        &self.original_error
    }

    pub(crate) const fn diagnostic_failure(&self) -> Option<UserFacingDiagnosticFailure> {
        self.diagnostic_failure
    }
}

fn classify_source_error(error: &SourceProcessorError) -> UserFacingFailureClass {
    match error {
        SourceProcessorError::Lex(LexError::InvalidCharacter { .. })
        | SourceProcessorError::Compile(_)
        | SourceProcessorError::SourceWord(_) => UserFacingFailureClass::UserProgram,
        SourceProcessorError::Runtime(error) => {
            if is_runtime_external_output_failure(*error) {
                UserFacingFailureClass::Environment
            } else {
                UserFacingFailureClass::UserProgram
            }
        }
        SourceProcessorError::Source(_)
        | SourceProcessorError::Lex(LexError::Source(_))
        | SourceProcessorError::CodeSpaceLookup(_)
        | SourceProcessorError::InstructionBuild(_)
        | SourceProcessorError::SourceMappingLookup(_)
        | SourceProcessorError::SourceWordContextUnavailable { .. }
        | SourceProcessorError::SourceWordLookup(_) => UserFacingFailureClass::Environment,
    }
}

fn is_runtime_external_output_failure(error: RuntimeError) -> bool {
    matches!(
        error.vm().kind(),
        crate::vm::VmErrorKind::PrimitiveFailed {
            source: PrimitiveError::OutputFailed { .. },
            ..
        }
    )
}

fn runtime_mapping_failure(error: &SourceProcessorError) -> Option<UserFacingDiagnosticFailure> {
    let SourceProcessorError::Runtime(error) = error else {
        return None;
    };

    match error.source_span() {
        // #1578/#1461 require mapping absence/inconsistency to remain a
        // diagnostic infrastructure failure, separate from the original VM
        // runtime failure. A legitimate `None` stays source-less.
        Err(source) => Some(UserFacingDiagnosticFailure::SourceMappingLookup {
            source,
            original_runtime: error.vm(),
        }),
        Ok(Some(_)) | Ok(None) => None,
    }
}

fn validate_diagnostic_span(sources: SourceView<'_>, span: SourceSpan) -> Result<(), SourceError> {
    sources.source(span.source_id())?;
    sources.display_name(span.source_id())?;
    Ok(())
}

fn build_diagnostic(
    error: &SourceProcessorError,
    diagnostic_failure: Option<UserFacingDiagnosticFailure>,
) -> UserDiagnostic {
    if let Some(diagnostic_failure) = diagnostic_failure {
        return diagnostic_for_infrastructure_failure(diagnostic_failure);
    }

    match error.primary_span() {
        Some(span) => UserDiagnostic::at_span(span, diagnostic_message(error)),
        None => UserDiagnostic::without_source(diagnostic_target(error), diagnostic_message(error)),
    }
}

fn diagnostic_for_infrastructure_failure(failure: UserFacingDiagnosticFailure) -> UserDiagnostic {
    match failure {
        UserFacingDiagnosticFailure::SourceMappingLookup { .. } => UserDiagnostic::without_source(
            "runtime diagnostic",
            "runtime source location could not be resolved",
        )
        .with_note("original runtime failure is preserved separately"),
        UserFacingDiagnosticFailure::SourceLookup { .. } => UserDiagnostic::without_source(
            "diagnostic source",
            "source text or display information could not be resolved",
        ),
    }
}

fn diagnostic_message(error: &SourceProcessorError) -> &'static str {
    match error {
        SourceProcessorError::Lex(LexError::InvalidCharacter { .. }) => "lexical error",
        SourceProcessorError::Lex(LexError::Source(_)) | SourceProcessorError::Source(_) => {
            "source lookup failed"
        }
        SourceProcessorError::Compile(_) => "compile error",
        SourceProcessorError::CodeSpaceLookup(_) => "code space lookup failed",
        SourceProcessorError::InstructionBuild(_) => "instruction build failed",
        SourceProcessorError::SourceMappingLookup(_) => "source mapping lookup failed",
        SourceProcessorError::SourceWordContextUnavailable { .. } => {
            "source word context unavailable"
        }
        SourceProcessorError::SourceWordLookup(_) => "source word lookup failed",
        SourceProcessorError::SourceWord(_) => "source word error",
        SourceProcessorError::Runtime(error) => {
            if is_runtime_external_output_failure(*error) {
                "runtime output failed"
            } else {
                "runtime error"
            }
        }
    }
}

fn diagnostic_target(error: &SourceProcessorError) -> &'static str {
    match error {
        SourceProcessorError::Runtime(_) => "runtime",
        SourceProcessorError::Lex(_) | SourceProcessorError::Compile(_) => "source program",
        SourceProcessorError::Source(_)
        | SourceProcessorError::CodeSpaceLookup(_)
        | SourceProcessorError::InstructionBuild(_)
        | SourceProcessorError::SourceMappingLookup(_)
        | SourceProcessorError::SourceWordContextUnavailable { .. }
        | SourceProcessorError::SourceWordLookup(_)
        | SourceProcessorError::SourceWord(_) => "source processing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::bootstrap::{register_builtin_source_words, register_primitive};
    use crate::diagnostic::DiagnosticRenderer;
    use crate::instruction::{
        Instruction, InstructionAddress, InstructionAddressError, InstructionSequence,
    };
    use crate::lexer::InvalidCharacterReason;
    use crate::operator::register_operator_primitives;
    use crate::primitive::{PrimitiveContext, PrimitiveRegistry};
    use crate::runtime_output::{RuntimeOutputError, TestOutput};
    use crate::source::{SourceId, SourceTexts};
    use crate::source_mapping::InstructionSourceMapping;
    use crate::source_processor::{
        compile_source, run_source, SourceCompileContext, SourceExecutionContext,
    };
    use crate::source_word::SourceWordRegistry;
    use crate::value::Value;
    use crate::vm::{RunOutcome, VmErrorKind};
    use crate::word::{CompletedWordDefinition, PublishedWords};
    use crate::word_lookup::PublishedWordLookup;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text, "test.tbx");
        (sources, id)
    }

    fn span(sources: &SourceTexts, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        sources
            .view()
            .span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn name(value: &str) -> crate::name::NormalizedName {
        crate::name::NormalizedName::new(value).expect("test name should be valid")
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn push_7(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(7));
        Ok(())
    }

    fn fail(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(1));
        Err(PrimitiveError::Failed)
    }

    fn publish_initial(
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        source_name: &str,
        definition: CompletedWordDefinition,
    ) -> crate::word::WordId {
        let word = words.add(definition);
        bindings
            .insert_new(name(source_name), Binding::Word(word))
            .expect("test binding should publish");
        word
    }

    fn basic_runtime_context<'a>(
        bindings: &'a Bindings,
        words: &'a PublishedWords,
        primitives: &'a PrimitiveRegistry,
    ) -> SourceExecutionContext<'a> {
        SourceExecutionContext::new(
            bindings,
            PublishedWordLookup::new(words),
            primitives.lookup(),
        )
    }

    fn classify(
        sources: &SourceTexts,
        result: Result<SourceRunResult, SourceProcessorError>,
    ) -> UserFacingRunResult {
        UserFacingRunResult::from_source_result(sources.view(), result)
    }

    fn failure(result: UserFacingRunResult) -> UserFacingFailure {
        match result {
            UserFacingRunResult::Failure(failure) => *failure,
            UserFacingRunResult::Success(_) => panic!("expected failure"),
        }
    }

    #[test]
    fn successful_halt_is_not_a_failure_classification() {
        let (sources, source_id) = source("");
        let bindings = Bindings::new();
        let words = PublishedWords::new();
        let primitives = PrimitiveRegistry::new();

        let result = classify(
            &sources,
            run_source(
                sources.view(),
                source_id,
                basic_runtime_context(&bindings, &words, &primitives),
            ),
        );

        match result {
            UserFacingRunResult::Success(result) => {
                assert_eq!(result.outcome(), RunOutcome::Halted);
            }
            UserFacingRunResult::Failure(failure) => {
                panic!("success should not classify as failure: {failure:?}")
            }
        }
    }

    #[test]
    fn lexical_syntax_semantic_and_compile_failures_are_user_program_failures() {
        let (lex_sources, lex_source_id) = source("?");
        let lex_error = SourceProcessorError::Lex(LexError::InvalidCharacter {
            span: span(&lex_sources, lex_source_id, 0, 1),
            character: '?',
            reason: InvalidCharacterReason::UnexpectedQuestionMark,
        });
        assert_eq!(
            failure(classify(&lex_sources, Err(lex_error))).class(),
            UserFacingFailureClass::UserProgram
        );

        let (syntax_sources, syntax_source_id) = source("BIF");
        let syntax_error = compile_source(
            syntax_sources.view(),
            syntax_source_id,
            SourceCompileContext::new(&Bindings::new()),
        )
        .expect_err("incomplete BIF should be a syntax compile failure");
        assert_eq!(
            failure(classify(&syntax_sources, Err(syntax_error))).class(),
            UserFacingFailureClass::UserProgram
        );

        let (semantic_sources, semantic_source_id) = source("UNKNOWN");
        let semantic_error = compile_source(
            semantic_sources.view(),
            semantic_source_id,
            SourceCompileContext::new(&Bindings::new()),
        )
        .expect_err("undefined word should be a semantic compile failure");
        assert_eq!(
            failure(classify(&semantic_sources, Err(semantic_error))).class(),
            UserFacingFailureClass::UserProgram
        );

        let (compile_sources, compile_source_id) = source("32768");
        let compile_error = compile_source(
            compile_sources.view(),
            compile_source_id,
            SourceCompileContext::new(&Bindings::new()),
        )
        .expect_err("integer overflow should be a compile failure");
        assert_eq!(
            failure(classify(&compile_sources, Err(compile_error))).class(),
            UserFacingFailureClass::UserProgram
        );
    }

    #[test]
    fn ordinary_runtime_failure_is_user_program_failure_with_mapped_primary_span() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail);
        register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
            .expect("primitive should register");
        let (sources, source_id) = source("fail");
        let expected_span = span(&sources, source_id, 0, 4);

        let failure = failure(classify(
            &sources,
            run_source(
                sources.view(),
                source_id,
                basic_runtime_context(&bindings, &words, &primitives),
            ),
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(failure.diagnostic_failure(), None);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(failure.diagnostic())
            .expect("runtime diagnostic should render");
        assert_eq!(
            rendered.primary().map(|primary| primary.source_line()),
            Some("fail")
        );
        assert_eq!(failure.original_error().primary_span(), Some(expected_span));
    }

    #[test]
    fn runtime_external_output_failure_is_environment_failure_even_with_source_span() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let mut source_words = SourceWordRegistry::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("source words should register");
        crate::output_primitive::register_output_primitives(
            &mut primitives,
            &mut words,
            &mut bindings,
        )
        .expect("output primitives should register");
        let (sources, source_id) = source("EVAL 7\nPRINT");
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);

        let failure = failure(classify(
            &sources,
            run_source(
                sources.view(),
                source_id,
                SourceExecutionContext::with_source_words_and_operators(
                    &bindings,
                    source_words.lookup(),
                    operators.lookup(),
                    PublishedWordLookup::new(&words),
                    primitives.lookup(),
                )
                .with_output(&mut output),
            ),
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert_eq!(failure.diagnostic_failure(), None);
        assert!(failure.original_error().primary_span().is_some());
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(failure.diagnostic())
            .expect("output diagnostic should render");
        assert!(rendered.primary().is_some());
    }

    #[test]
    fn legitimate_unmapped_runtime_failure_does_not_become_mapping_failure() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail);
        let fail_word = publish_initial(
            &mut words,
            &mut bindings,
            "FAIL",
            CompletedWordDefinition::primitive(primitive),
        );
        let mut published_code = InstructionSequence::new();
        let entry = published_code.append(Instruction::Call(fail_word));
        published_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "BAD",
            CompletedWordDefinition::compiled(
                published_code.view().location(entry),
                published_code.view(),
            )
            .expect("compiled definition should be valid"),
        );
        let mut mapping = InstructionSourceMapping::new(published_code.code_space());
        mapping
            .append_unmapped(entry)
            .expect("unmapped entry should append");
        let code_spaces = [published_code.view()];
        let mappings = [mapping.view()];
        let (sources, source_id) = source("bad");

        let failure = failure(classify(
            &sources,
            run_source(
                sources.view(),
                source_id,
                SourceExecutionContext::with_code_spaces_and_mappings(
                    &bindings,
                    &code_spaces,
                    &mappings,
                    PublishedWordLookup::new(&words),
                    primitives.lookup(),
                ),
            ),
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(failure.diagnostic_failure(), None);
        assert_eq!(failure.original_error().primary_span(), None);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(failure.diagnostic())
            .expect("source-less runtime diagnostic should render");
        assert!(rendered.primary().is_none());
        assert!(!rendered.to_string().contains(":1:1"));
    }

    #[test]
    fn missing_runtime_mapping_is_environment_failure_and_preserves_runtime_error() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail);
        let fail_word = publish_initial(
            &mut words,
            &mut bindings,
            "FAIL",
            CompletedWordDefinition::primitive(primitive),
        );
        let mut published_code = InstructionSequence::new();
        let entry = published_code.append(Instruction::Call(fail_word));
        published_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "BAD",
            CompletedWordDefinition::compiled(
                published_code.view().location(entry),
                published_code.view(),
            )
            .expect("compiled definition should be valid"),
        );
        let code_spaces = [published_code.view()];
        let (sources, source_id) = source("bad");

        let failure = failure(classify(
            &sources,
            run_source(
                sources.view(),
                source_id,
                SourceExecutionContext::with_code_spaces(
                    &bindings,
                    &code_spaces,
                    PublishedWordLookup::new(&words),
                    primitives.lookup(),
                ),
            ),
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        match failure
            .diagnostic_failure()
            .expect("missing mapping should be recorded separately")
        {
            UserFacingDiagnosticFailure::SourceMappingLookup {
                source: SourceMappingLookupError::UnknownCodeSpace { code_space: actual },
                original_runtime,
            } => {
                assert_eq!(actual, published_code.code_space());
                assert_eq!(
                    original_runtime.location(),
                    published_code.view().location(entry)
                );
                assert!(matches!(
                    original_runtime.kind(),
                    VmErrorKind::PrimitiveFailed { .. }
                ));
            }
            other => panic!("unexpected diagnostic failure: {other:?}"),
        }
    }

    #[test]
    fn wrong_code_space_and_missing_mapping_entry_are_environment_failures() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let mut mapping = InstructionSourceMapping::new(code.code_space());
        let other_code = InstructionSequence::new();
        assert_eq!(
            mapping
                .view()
                .source_span(other_code.view().location(entry))
                .expect_err("wrong code space should fail"),
            SourceMappingLookupError::Address {
                source: InstructionAddressError::CodeSpaceMismatch {
                    expected: code.code_space(),
                    actual: other_code.code_space(),
                    address: entry,
                }
            }
        );
        assert_eq!(
            mapping
                .append_unmapped(address(0))
                .expect("first entry should append"),
            ()
        );
        assert_eq!(
            mapping
                .view()
                .source_span(code.view().location(address(1)))
                .expect_err("missing end entry should fail"),
            SourceMappingLookupError::Address {
                source: InstructionAddressError::EndAddress {
                    address: address(1)
                }
            }
        );
    }

    #[test]
    fn source_lookup_failure_during_diagnostic_is_environment_failure_without_fake_span() {
        let (foreign_sources, foreign_source_id) = source("?");
        let foreign_span = span(&foreign_sources, foreign_source_id, 0, 1);
        let error = SourceProcessorError::Lex(LexError::InvalidCharacter {
            span: foreign_span,
            character: '?',
            reason: InvalidCharacterReason::UnexpectedQuestionMark,
        });
        let local_sources = SourceTexts::new();

        let failure = failure(UserFacingRunResult::from_source_result(
            local_sources.view(),
            Err(error),
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(matches!(
            failure.diagnostic_failure(),
            Some(UserFacingDiagnosticFailure::SourceLookup {
                source: SourceError::InvalidSourceId { .. }
            })
        ));
        let rendered = DiagnosticRenderer::new(local_sources.view())
            .render(failure.diagnostic())
            .expect("diagnostic source failure should render source-less");
        assert!(rendered.primary().is_none());
        assert!(!rendered.to_string().contains(":1:1"));
    }

    #[test]
    fn classification_uses_variants_not_display_text() {
        let (sources, source_id) = source("BIF");
        let compile_error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::new(&Bindings::new()),
        )
        .expect_err("incomplete BIF should fail before runtime");
        let failure = failure(classify(&sources, Err(compile_error)));
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(failure.diagnostic())
            .expect("compile diagnostic should render");

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(rendered.message(), "compile error");
    }

    #[test]
    fn source_processor_environment_failures_do_not_get_user_program_class() {
        let (sources, source_id) = source("A");
        let error =
            SourceProcessorError::SourceMappingLookup(SourceMappingLookupError::UnknownCodeSpace {
                code_space: crate::source_mapping::SourceMappedCode::new().code_space(),
            });

        let failure = failure(classify(&sources, Err(error)));

        assert_eq!(source_id, span(&sources, source_id, 0, 1).source_id());
        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(failure.diagnostic())
            .expect("environment diagnostic should render");
        assert!(rendered.primary().is_none());
    }
}
