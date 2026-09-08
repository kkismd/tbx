use std::cell::RefCell;
use std::io::Write;

use crate::binding::Bindings;
use crate::bootstrap::{
    register_builtin_global_variables, register_builtin_source_words, BuiltinGlobalBootstrapError,
    PrimitiveBootstrapError, SourceWordBootstrapError,
};
use crate::diagnostic::{DiagnosticRenderer, RenderedDiagnostic, UserDiagnostic};
use crate::global_variable::GlobalVariables;
use crate::operator::{register_named_operator_primitives, OperatorBootstrapError, OperatorWords};
use crate::output_primitive::register_output_primitives;
use crate::primitive::PrimitiveRegistry;
use crate::published_code::PublishedCode;
use crate::runtime_output::WriteRuntimeOutput;
use crate::source::{SourceId, SourceTexts};
use crate::source_processor::{
    compile_source, run_unit, SourceCompileContext, SourceExecutionContext, SourceRunResult,
};
use crate::source_session::SourceProcessingSession;
use crate::source_word::{AdditionalSourceProcessor, SourceWordRegistry};
use crate::stack_primitive::register_stack_primitives;
use crate::user_facing::{UserFacingFailure, UserFacingFailureClass, UserFacingRunResult};
use crate::word::PublishedWords;
use crate::word_lookup::PublishedWordLookup;

#[cfg(test)]
use crate::cli_source::{register_embedded_standard_library, STDLIB_SOURCE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BatchExecutionResult {
    Success(SourceRunResult),
    Failure(Box<BatchExecutionFailure>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BatchExecutionFailure {
    cause: BatchExecutionFailureCause,
    diagnostic: RenderedDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BatchExecutionFailureCause {
    Setup(BatchSetupError),
    StandardLibrary(Box<UserFacingFailure>),
    Source(Box<UserFacingFailure>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchSetupError {
    Operators(OperatorBootstrapError),
    Stack(PrimitiveBootstrapError),
    Output(PrimitiveBootstrapError),
    SourceWords(SourceWordBootstrapError),
    Globals(BuiltinGlobalBootstrapError),
}

struct BatchEnvironment {
    bindings: Bindings,
    primitives: PrimitiveRegistry,
    words: PublishedWords,
    operators: OperatorWords,
    source_words: SourceWordRegistry,
    globals: GlobalVariables,
    published_code: PublishedCode,
}

impl BatchExecutionFailure {
    pub(crate) fn class(&self) -> UserFacingFailureClass {
        match &self.cause {
            BatchExecutionFailureCause::Setup(_)
            | BatchExecutionFailureCause::StandardLibrary(_) => UserFacingFailureClass::Environment,
            BatchExecutionFailureCause::Source(failure) => failure.class(),
        }
    }

    pub(crate) const fn diagnostic(&self) -> &RenderedDiagnostic {
        &self.diagnostic
    }
}

impl BatchEnvironment {
    fn new() -> Result<Self, BatchSetupError> {
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let operators =
            register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                .map_err(BatchSetupError::Operators)?;
        register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Stack)?;
        register_output_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Output)?;

        let mut source_words = SourceWordRegistry::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .map_err(BatchSetupError::SourceWords)?;

        let mut globals = GlobalVariables::new();
        register_builtin_global_variables(&mut globals, &mut bindings)
            .map_err(BatchSetupError::Globals)?;

        Ok(Self {
            bindings,
            primitives,
            words,
            operators,
            source_words,
            globals,
            published_code: PublishedCode::new(),
        })
    }

    fn compile(
        &mut self,
        sources: &SourceTexts,
        source_id: SourceId,
    ) -> Result<
        crate::source_processor::TemporaryExecutionUnit,
        crate::source_processor::SourceProcessorError,
    > {
        self.compile_with_additional_source_processor(sources, source_id, None)
    }

    fn compile_with_additional_source_processor(
        &mut self,
        sources: &SourceTexts,
        source_id: SourceId,
        processor: Option<&RefCell<dyn AdditionalSourceProcessor>>,
    ) -> Result<
        crate::source_processor::TemporaryExecutionUnit,
        crate::source_processor::SourceProcessorError,
    > {
        let context = SourceCompileContext::with_source_word_and_runtime_publication_and_operators(
            &mut self.bindings,
            &mut self.source_words,
            self.operators.lookup(),
            &mut self.globals,
            &mut self.published_code,
            &mut self.words,
        );
        let context = match processor {
            Some(processor) => context.with_additional_source_processor(processor),
            None => context,
        };
        compile_source(sources.view(), source_id, context)
    }
}

pub(crate) fn execute_registered_source<W>(
    sources: &SourceTexts,
    source_id: SourceId,
    writer: &mut W,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    execute_registered_sources(sources, source_id, source_id, writer)
}

/// Executes all sources against one session-owned source store. The batch
/// environment and publication state live for this call, alongside the
/// session, so later acquisition can append and process sources without
/// replacing the mapping owner (#1642/#1649).
pub(crate) fn execute_source_session<W>(
    session: &mut SourceProcessingSession,
    stdlib_source_id: SourceId,
    source_id: SourceId,
    writer: &mut W,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut run = match BatchExecutionRun::new(session) {
        Ok(run) => run,
        Err(error) => return setup_failure(session.sources(), error),
    };
    run.execute(stdlib_source_id, source_id, writer)
}

/// Owns one source-processing run's publication state while borrowing the
/// session that owns all source text and acquisition metadata. Additional
/// source handling must use this run so nested sources share the same
/// bindings, source words, published code, and globals.
struct BatchExecutionRun<'a> {
    session: &'a mut SourceProcessingSession,
    environment: BatchEnvironment,
}

impl<'a> BatchExecutionRun<'a> {
    fn new(session: &'a mut SourceProcessingSession) -> Result<Self, BatchSetupError> {
        Ok(Self {
            session,
            environment: BatchEnvironment::new()?,
        })
    }

    fn compile(
        &mut self,
        source_id: SourceId,
    ) -> Result<
        crate::source_processor::TemporaryExecutionUnit,
        crate::source_processor::SourceProcessorError,
    > {
        self.environment.compile(self.session.sources(), source_id)
    }

    fn compile_with_additional_source_processor(
        &mut self,
        source_id: SourceId,
        processor: &RefCell<dyn AdditionalSourceProcessor>,
    ) -> Result<
        crate::source_processor::TemporaryExecutionUnit,
        crate::source_processor::SourceProcessorError,
    > {
        self.environment.compile_with_additional_source_processor(
            self.session.sources(),
            source_id,
            Some(processor),
        )
    }

    /// Compiles an acquired source through the same publication environment as
    /// its caller. This is the connection point used by the future
    /// `AdditionalSourceProcessor` integration.
    fn process_registered_source(
        &mut self,
        source_id: SourceId,
    ) -> Result<
        crate::source_processor::TemporaryExecutionUnit,
        crate::source_processor::SourceProcessorError,
    > {
        self.compile(source_id)
    }

    fn execute<W>(
        &mut self,
        stdlib_source_id: SourceId,
        source_id: SourceId,
        writer: &mut W,
    ) -> BatchExecutionResult
    where
        W: Write + ?Sized,
    {
        if stdlib_source_id != source_id {
            let stdlib_result = self.compile(stdlib_source_id);
            if let Err(error) = stdlib_result {
                return standard_library_failure(self.session.sources(), error);
            }
        }

        let compile_result = self.compile(source_id);
        match compile_result {
            Ok(unit) => self.execute_unit(unit, writer),
            Err(error) => user_facing_result(self.session.sources(), Err(error)),
        }
    }

    fn execute_unit<W>(
        &mut self,
        unit: crate::source_processor::TemporaryExecutionUnit,
        writer: &mut W,
    ) -> BatchExecutionResult
    where
        W: Write + ?Sized,
    {
        let code_spaces = [self.environment.published_code.instruction_view()];
        let source_mappings = [self.environment.published_code.source_mapping()];
        let mut output = WriteRuntimeOutput::new(writer);
        let context = SourceExecutionContext::with_runtime_environment(
            &self.environment.bindings,
            self.environment.source_words.lookup(),
            self.environment.operators.lookup(),
            &code_spaces,
            &source_mappings,
            PublishedWordLookup::new(&self.environment.words),
            self.environment.primitives.lookup(),
        )
        .with_mut_globals(self.environment.globals.view_mut())
        .with_output(&mut output);
        user_facing_result(self.session.sources(), run_unit(&unit, context))
    }
}

pub(crate) fn execute_registered_sources<W>(
    sources: &SourceTexts,
    stdlib_source_id: SourceId,
    source_id: SourceId,
    writer: &mut W,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut environment = match BatchEnvironment::new() {
        Ok(environment) => environment,
        Err(error) => return setup_failure(sources, error),
    };

    if stdlib_source_id != source_id {
        let stdlib_result = environment.compile(sources, stdlib_source_id);

        if let Err(error) = stdlib_result {
            return standard_library_failure(sources, error);
        }
    }

    let compile_result = environment.compile(sources, source_id);

    let source_result = match compile_result {
        Ok(unit) => {
            let code_spaces = [environment.published_code.instruction_view()];
            let source_mappings = [environment.published_code.source_mapping()];
            let mut output = WriteRuntimeOutput::new(writer);
            let context = SourceExecutionContext::with_runtime_environment(
                &environment.bindings,
                environment.source_words.lookup(),
                environment.operators.lookup(),
                &code_spaces,
                &source_mappings,
                PublishedWordLookup::new(&environment.words),
                environment.primitives.lookup(),
            )
            .with_mut_globals(environment.globals.view_mut())
            .with_output(&mut output);
            run_unit(&unit, context)
        }
        Err(error) => Err(error),
    };

    user_facing_result(sources, source_result)
}

#[cfg(test)]
pub(crate) fn execute_with_embedded_standard_library<W>(
    source: &str,
    display_name: &str,
    writer: &mut W,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut sources = SourceTexts::new();
    let stdlib_source_id = sources.register(STDLIB_SOURCE, crate::cli_source::STDLIB_DISPLAY_NAME);
    let source_id = sources.register(source, display_name);
    execute_registered_sources(&sources, stdlib_source_id, source_id, writer)
}

fn standard_library_failure(
    sources: &SourceTexts,
    error: crate::source_processor::SourceProcessorError,
) -> BatchExecutionResult {
    let failure = match UserFacingRunResult::from_source_result(sources.view(), Err(error)) {
        UserFacingRunResult::Failure(failure) => failure,
        UserFacingRunResult::Success(_) => unreachable!("an error must classify as a failure"),
    };
    let diagnostic = DiagnosticRenderer::new(sources.view())
        .render(failure.diagnostic())
        .expect("standard-library diagnostic must render");
    BatchExecutionResult::Failure(Box::new(BatchExecutionFailure {
        cause: BatchExecutionFailureCause::StandardLibrary(failure),
        diagnostic,
    }))
}

fn user_facing_result(
    sources: &SourceTexts,
    result: Result<SourceRunResult, crate::source_processor::SourceProcessorError>,
) -> BatchExecutionResult {
    match UserFacingRunResult::from_source_result(sources.view(), result) {
        UserFacingRunResult::Success(result) => BatchExecutionResult::Success(result),
        UserFacingRunResult::Failure(failure) => {
            // #1592 validates a primary span's source ownership and display
            // information before this boundary. An unresolved span is replaced
            // with a source-less diagnostic, so #1591 rendering cannot fail here.
            let diagnostic = DiagnosticRenderer::new(sources.view())
                .render(failure.diagnostic())
                .expect("user-facing classification must produce a renderable diagnostic");
            BatchExecutionResult::Failure(Box::new(BatchExecutionFailure {
                cause: BatchExecutionFailureCause::Source(failure),
                diagnostic,
            }))
        }
    }
}

fn setup_failure(sources: &SourceTexts, error: BatchSetupError) -> BatchExecutionResult {
    let diagnostic =
        UserDiagnostic::without_source("execution environment", "runtime environment setup failed");
    let diagnostic = DiagnosticRenderer::new(sources.view())
        .render(&diagnostic)
        .expect("source-less setup diagnostic must render");
    BatchExecutionResult::Failure(Box::new(BatchExecutionFailure {
        cause: BatchExecutionFailureCause::Setup(error),
        diagnostic,
    }))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::*;
    use crate::binding::Binding;
    use crate::bootstrap::register_native_source_word;
    use crate::name::NormalizedName;
    use crate::source::SourceTexts;
    use crate::source_word::{NativeSourceWordContext, SourceWordError};
    use crate::user_facing::UserFacingFailureClass;
    use crate::value::Value;

    fn name(value: &str) -> crate::name::NormalizedName {
        crate::name::NormalizedName::new(value).expect("test name should be valid")
    }

    #[derive(Debug, Default)]
    struct RecordingWriter {
        bytes: Vec<u8>,
        fail_after_writes: Option<usize>,
        writes: usize,
    }

    impl RecordingWriter {
        fn failing_after(successful_writes: usize) -> Self {
            Self {
                fail_after_writes: Some(successful_writes),
                ..Self::default()
            }
        }

        fn text(&self) -> &str {
            std::str::from_utf8(&self.bytes).expect("runtime output should be UTF-8")
        }
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_after_writes == Some(self.writes) {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
            }
            self.writes += 1;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn source(text: &str, display_name: &str) -> (SourceTexts, crate::source::SourceId) {
        let mut sources = SourceTexts::new();
        let source_id = sources.register(text, display_name);
        (sources, source_id)
    }

    fn sources_with_standard_library(
        standard_library: &str,
        source: &str,
    ) -> (SourceTexts, SourceId, SourceId) {
        let mut sources = SourceTexts::new();
        let standard_library_id = sources.register(standard_library, "<tbx-next-stdlib>");
        let source_id = sources.register(source, "program.tbx");
        (sources, standard_library_id, source_id)
    }

    fn test_additional_source_word(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let specification = context.statement_reader_mut().read_name().map_err(|_| {
            SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            }
        })?;
        context.process_additional_source(specification.span())?;
        context.statement_reader_mut().finish().map_err(|_| {
            SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            }
        })
    }

    struct ReentrantTestProcessor {
        run: *mut (),
        source_b: Option<SourceId>,
        fail: bool,
    }

    impl AdditionalSourceProcessor for ReentrantTestProcessor {
        fn process_additional_source(
            &mut self,
            _source_specification: &str,
            request_span: crate::source::SourceSpan,
        ) -> Result<(), SourceWordError> {
            // The production connection point is synchronous. This test-only
            // adapter re-enters the same run while A's source-word handler is
            // suspended; the run contract guarantees that no outer compile
            // operation touches the shared publication state until this call
            // returns.
            let run = unsafe { &mut *(self.run as *mut BatchExecutionRun<'_>) };
            let source_b = run.session.register(
                if self.fail {
                    "UNKNOWN"
                } else {
                    "DEF ANSWER\nEVAL 42\nEND"
                },
                "b.tbx",
                crate::source_session::SourceAcquisition::filesystem("/workspace/b.tbx"),
            );
            let result = run.compile(source_b);
            self.source_b = Some(source_b);
            if self.fail {
                return Err(SourceWordError::UnsupportedSourceWord { span: request_span });
            }
            result
                .map(|_| ())
                .map_err(|_| SourceWordError::UnsupportedSourceWord { span: request_span })
        }
    }

    fn success(result: BatchExecutionResult) -> crate::source_processor::SourceRunResult {
        match result {
            BatchExecutionResult::Success(result) => result,
            BatchExecutionResult::Failure(failure) => {
                panic!("expected success, got {failure:?}")
            }
        }
    }

    fn failure(result: BatchExecutionResult) -> BatchExecutionFailure {
        match result {
            BatchExecutionResult::Failure(failure) => *failure,
            BatchExecutionResult::Success(result) => {
                panic!("expected failure, got {result:?}")
            }
        }
    }

    #[test]
    fn registered_file_and_stdin_sources_share_the_same_execution_path() {
        for display_name in ["program.tbx", "<stdin>"] {
            let (sources, source_id) = source("EVAL 2 + 3\nEVAL ADD(4, 5)", display_name);
            let mut writer = RecordingWriter::default();

            let result = success(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(result.data_stack(), [Value::integer(5), Value::integer(9)]);
            assert_eq!(writer.text(), "");
        }
    }

    #[test]
    fn m20_environment_supports_variables_definitions_stack_words_and_output() {
        let text = "LET A = 4\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(A)\nPRINT\nCR";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(4)]);
        assert_eq!(writer.text(), "4\n");
    }

    #[test]
    fn additional_source_callback_uses_the_same_session_and_publication_environment() {
        let mut session = SourceProcessingSession::new();
        let stdlib_source_id = register_embedded_standard_library(&mut session);
        let source_a = session.register(
            "EVAL ANSWER()\nPRINT",
            "a.tbx",
            crate::source_session::SourceAcquisition::filesystem("/workspace/a.tbx"),
        );
        let mut run = BatchExecutionRun::new(&mut session).expect("run should initialize");
        run.compile(stdlib_source_id)
            .expect("standard library should compile");

        // This models the synchronous callback point used by additional
        // source processing. The callback registers and compiles B before A
        // resumes, using the same run-owned publication environment.
        let process_callback = |run: &mut BatchExecutionRun<'_>| {
            let source_b = run.session.register(
                "DEF ANSWER\nEVAL 42\nEND",
                "b.tbx",
                crate::source_session::SourceAcquisition::filesystem("/workspace/b.tbx"),
            );
            let unit_b = run
                .process_registered_source(source_b)
                .expect("additional source should compile in the same run");
            assert_eq!(
                unit_b
                    .source_span(unit_b.entry_location())
                    .expect("B mapping should resolve")
                    .unwrap()
                    .source_id(),
                source_b
            );
            source_b
        };
        let source_b = process_callback(&mut run);
        let unit_a = run
            .compile(source_a)
            .expect("A should resolve B after callback");
        let mut writer = RecordingWriter::default();
        let result = run.execute_unit(unit_a, &mut writer);
        drop(run);

        assert!(matches!(result, BatchExecutionResult::Success(_)));
        assert_eq!(writer.text(), "42");
        assert_eq!(
            session.sources().view().source(source_a),
            Ok("EVAL ANSWER()\nPRINT")
        );
        assert_eq!(
            session.sources().view().source(source_b),
            Ok("DEF ANSWER\nEVAL 42\nEND")
        );
        assert_eq!(
            session.acquisition(source_a).unwrap().canonical_path(),
            Some(std::path::Path::new("/workspace/a.tbx"))
        );
        assert_eq!(
            session.acquisition(source_b).unwrap().canonical_path(),
            Some(std::path::Path::new("/workspace/b.tbx"))
        );
    }

    #[test]
    fn native_source_word_dispatch_reenters_the_same_run_before_a_resumes() {
        let mut session = SourceProcessingSession::new();
        let stdlib_source_id = register_embedded_standard_library(&mut session);
        let source_a = session.register(
            "LOAD b\nEVAL ANSWER()\nPRINT",
            "a.tbx",
            crate::source_session::SourceAcquisition::filesystem("/workspace/a.tbx"),
        );
        let mut run = BatchExecutionRun::new(&mut session).expect("run should initialize");
        run.compile(stdlib_source_id)
            .expect("standard library should compile");
        register_native_source_word(
            &mut run.environment.source_words,
            &mut run.environment.bindings,
            NormalizedName::new("LOAD").expect("test name should be valid"),
            test_additional_source_word,
        )
        .expect("test source word should register");

        let mut processor = RefCell::new(ReentrantTestProcessor {
            run: (&mut run as *mut BatchExecutionRun<'_>).cast(),
            source_b: None,
            fail: false,
        });
        let unit = run
            .compile_with_additional_source_processor(source_a, &processor)
            .expect("A should resume after B is processed");
        let source_b = processor
            .get_mut()
            .source_b
            .expect("callback should register B");
        let mut writer = RecordingWriter::default();
        let result = run.execute_unit(unit, &mut writer);
        assert!(matches!(result, BatchExecutionResult::Success(_)));
        assert_eq!(writer.text(), "42");
        assert_eq!(
            run.session.sources().view().source(source_a),
            Ok("LOAD b\nEVAL ANSWER()\nPRINT")
        );
        assert_eq!(
            run.session.sources().view().source(source_b),
            Ok("DEF ANSWER\nEVAL 42\nEND")
        );
    }

    #[test]
    fn native_source_word_dispatch_propagates_nested_source_failure() {
        let mut session = SourceProcessingSession::new();
        let stdlib_source_id = register_embedded_standard_library(&mut session);
        let source_a = session.register(
            "LOAD b\nEVAL 99",
            "a.tbx",
            crate::source_session::SourceAcquisition::NoFilesystemLocation,
        );
        let mut run = BatchExecutionRun::new(&mut session).expect("run should initialize");
        run.compile(stdlib_source_id)
            .expect("standard library should compile");
        register_native_source_word(
            &mut run.environment.source_words,
            &mut run.environment.bindings,
            NormalizedName::new("LOAD").expect("test name should be valid"),
            test_additional_source_word,
        )
        .expect("test source word should register");

        let mut processor = RefCell::new(ReentrantTestProcessor {
            run: (&mut run as *mut BatchExecutionRun<'_>).cast(),
            source_b: None,
            fail: true,
        });
        let error = run
            .compile_with_additional_source_processor(source_a, &processor)
            .expect_err("nested B failure should fail A");
        assert!(matches!(
            error,
            crate::source_processor::SourceProcessorError::SourceWord(_)
        ));
        assert!(processor.get_mut().source_b.is_some());
    }

    #[test]
    fn batch_top_level_can_publish_and_use_a_source_word_before_a_definition() {
        let text = "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nLET A = 0\nSLET A = 7\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(A)";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(7)]);
    }

    #[test]
    fn batch_top_level_can_publish_and_use_a_block_source_word() {
        let text = "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS\nWRAP\nENDWRAP\nEVAL 9";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(9)]);
    }

    #[test]
    fn batch_top_level_can_publish_multiple_source_words_in_sequence() {
        let text = "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nSYNTAX SADD\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nLET A = 0\nSLET A = 3\nSADD A = 4\nEVAL A";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(4)]);
    }

    #[test]
    fn located_compile_failure_renders_registered_display_name_and_position() {
        for display_name in ["relative/program.tbx", "<stdin>"] {
            let (sources, source_id) = source("UNKNOWN", display_name);
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
            let primary = failure
                .diagnostic()
                .primary()
                .expect("compile failure should have a source position");
            assert_eq!(primary.display_name(), display_name);
            assert_eq!(primary.line_number(), 1);
            assert_eq!(primary.column_number(), 1);
        }
    }

    #[test]
    fn foreign_source_id_is_source_less_environment_failure() {
        let (sources, _) = source("EVAL 1", "program.tbx");
        let (foreign_sources, foreign_source_id) = source("EVAL 2", "other.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(
            &sources,
            foreign_source_id,
            &mut writer,
        ));
        drop(foreign_sources);

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure.diagnostic().primary().is_none());
        assert_eq!(failure.diagnostic().target(), Some("source program"));
    }

    #[test]
    fn runtime_failure_is_a_located_user_program_failure() {
        let (sources, source_id) = source("DUP", "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(
            failure
                .diagnostic()
                .primary()
                .map(|primary| primary.source_line()),
            Some("DUP")
        );
    }

    #[test]
    fn runtime_output_failure_is_a_located_environment_failure() {
        let (sources, source_id) = source("EVAL 7\nPRINT", "program.tbx");
        let mut writer = RecordingWriter::failing_after(0);

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure.diagnostic().primary().is_some());
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn successful_runtime_output_is_not_rolled_back_by_a_later_failure() {
        let (sources, source_id) = source("EVAL 7\nPRINT\nCR", "program.tbx");
        let mut writer = RecordingWriter::failing_after(1);

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert_eq!(writer.text(), "7");
    }

    #[test]
    fn standard_library_source_word_is_available_to_the_user_source() {
        let standard_library =
            "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS";
        let source = "LET A = 0\nSLET A = 7\nEVAL A";
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(standard_library, source);
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(7)]);
    }

    #[test]
    fn standard_library_runtime_definition_keeps_its_source_mapping() {
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library("DEF FAIL\nEVAL 1 / 0\nEND", "FAIL");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(
            failure
                .diagnostic()
                .primary()
                .map(|primary| primary.display_name()),
            Some("<tbx-next-stdlib>")
        );
    }

    #[test]
    fn standard_library_block_source_word_and_marker_are_available_to_user_source() {
        let standard_library =
            "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS";
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(standard_library, "WRAP\nENDWRAP\nEVAL 9");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(9)]);
    }

    #[test]
    fn standard_library_marker_reservation_rejects_user_binding_with_same_name() {
        let standard_library =
            "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS";
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(standard_library, "DEF ENDWRAP\nEND");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert!(failure
            .diagnostic()
            .primary()
            .is_some_and(|primary| primary.source_line() == "DEF ENDWRAP"));
    }

    #[test]
    fn standard_library_marker_reservation_keeps_the_published_source_word_owner() {
        let standard_library =
            "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS";
        let mut sources = SourceTexts::new();
        let standard_library_id = sources.register(standard_library, "<tbx-next-stdlib>");
        let mut environment = BatchEnvironment::new().expect("batch environment should build");

        environment
            .compile(&sources, standard_library_id)
            .expect("standard library should compile");

        let owner = match environment.bindings.get(&name("WRAP")) {
            Some(Binding::SourceWord(owner)) => *owner,
            other => panic!("expected published source word, got {other:?}"),
        };
        assert_eq!(
            environment
                .bindings
                .syntax_marker_reservation(&name("ENDWRAP"))
                .map(|reservation| reservation.owner()),
            Some(owner)
        );
    }

    #[test]
    fn standard_library_top_level_unit_is_not_executed() {
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library("EVAL 99\nPRINT\nCR", "EVAL 1\nPRINT\nCR");
        let mut writer = RecordingWriter::default();

        success(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(writer.text(), "1\n");
    }

    #[test]
    fn standard_library_failure_short_circuits_user_source_and_is_environment_failure() {
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library("UNKNOWN", "EVAL 7\nPRINT");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure
            .diagnostic()
            .primary()
            .is_some_and(|primary| primary.display_name() == "<tbx-next-stdlib>"));
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn standard_library_lex_failure_short_circuits_user_source() {
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library("?", "EVAL 7\nPRINT");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure
            .diagnostic()
            .primary()
            .is_some_and(|primary| primary.display_name() == "<tbx-next-stdlib>"));
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn standard_library_publication_failure_short_circuits_user_source() {
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library("SYNTAX A\nSTATEMENT\nENDS", "EVAL 7\nPRINT");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_sources(
            &sources,
            standard_library_id,
            source_id,
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure
            .diagnostic()
            .primary()
            .is_some_and(|primary| primary.display_name() == "<tbx-next-stdlib>"));
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn embedded_standard_library_test_entry_uses_the_production_source() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "EVAL 3 + 4",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(7)]);
    }

    #[test]
    fn embedded_standard_library_while_repeats_until_condition_is_false() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nWHILE A < 3\nLET A = A + 1\nWEND\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(3)]);

        for (source, expected) in [
            ("WHILE 0\nLET A = 1\nWEND\nEVAL 0", 0),
            ("LET A = 0\nWHILE A < 1\nLET A = A + 1\nWEND\nEVAL A", 1),
        ] {
            let mut writer = RecordingWriter::default();
            let result = success(execute_with_embedded_standard_library(
                source,
                "program.tbx",
                &mut writer,
            ));

            assert_eq!(result.data_stack(), [Value::integer(expected)]);
        }
    }

    #[test]
    fn embedded_standard_library_do_runs_once_and_repeats_until_condition_is_true() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nDO\nLET A = A + 1\nUNTIL A >= 3\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(3)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nDO\nLET A = A + 1\nUNTIL A >= 1\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(1)]);
    }

    #[test]
    fn embedded_standard_library_control_structures_support_nested_and_native_if_blocks() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nLET B = 0\nIF 1\nWHILE A < 2\nDO\nLET B = B + 1\nUNTIL B >= 2\nLET A = A + 1\nWEND\nENDIF\nEVAL A\nEVAL B",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(3)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nDO\nLET B = 0\nWHILE B < 2\nLET B = B + 1\nWEND\nLET A = A + 1\nUNTIL A >= 2\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(2)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nLET B = 0\nWHILE A < 2\nLET B = 0\nWHILE B < 2\nLET B = B + 1\nWEND\nLET A = A + 1\nWEND\nEVAL A\nEVAL B",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(2)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nLET B = 0\nDO\nLET B = 0\nDO\nLET B = B + 1\nUNTIL B >= 2\nLET A = A + 1\nUNTIL A >= 2\nEVAL A\nEVAL B",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(2)]);
    }

    #[test]
    fn embedded_standard_library_control_structure_markers_are_reserved_by_their_owner() {
        let mut sources = SourceTexts::new();
        let stdlib_source_id =
            sources.register(STDLIB_SOURCE, crate::cli_source::STDLIB_DISPLAY_NAME);
        let mut environment = BatchEnvironment::new().expect("batch environment should build");

        environment
            .compile(&sources, stdlib_source_id)
            .expect("embedded standard library should compile");

        let Some(Binding::SourceWord(while_id)) = environment.bindings.get(&name("WHILE")) else {
            panic!("WHILE should publish as a source word");
        };
        let Some(Binding::SourceWord(do_id)) = environment.bindings.get(&name("DO")) else {
            panic!("DO should publish as a source word");
        };
        assert_eq!(
            environment
                .bindings
                .syntax_marker_reservation(&name("WEND"))
                .map(|reservation| reservation.owner()),
            Some(*while_id)
        );
        assert_eq!(
            environment
                .bindings
                .syntax_marker_reservation(&name("UNTIL"))
                .map(|reservation| reservation.owner()),
            Some(*do_id)
        );
        assert!(STDLIB_SOURCE.contains("SYNTAX WHILE"));
        assert!(STDLIB_SOURCE.contains("SYNTAX DO"));
    }

    #[test]
    fn embedded_standard_library_control_structure_markers_reject_binding_and_owner_mismatch() {
        for source in [
            "DEF WEND\nEND",
            "WHILE 1\nUNTIL 1",
            "DO\nWEND",
            "WHILE 1",
            "DO",
        ] {
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_with_embedded_standard_library(
                source,
                "program.tbx",
                &mut writer,
            ));

            assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
            assert!(failure.diagnostic().primary().is_some());
        }
    }
}
