use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

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
use crate::source::{SourceAcquisition, SourceId, SourceTexts};
use crate::source_processor::{
    compile_source, run_unit, run_unit_with_data_stack, AdditionalSourceAcquisitionError,
    SourceCompileContext, SourceExecutionContext, SourceFormCursor, SourceProcessorError,
    SourceRunResult,
};
use crate::source_word::{AdditionalSourceRequest, SourceWordRegistry};
use crate::stack_primitive::register_stack_primitives;
use crate::user_facing::{UserFacingFailure, UserFacingFailureClass, UserFacingRunResult};
use crate::value::Value;
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
    InvalidInitialSource(crate::source::SourceError),
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

struct SourceFrame {
    source_id: SourceId,
    forms: SourceFormCursor,
    form_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceIdentityState {
    Processing,
    Completed,
}

#[derive(Debug, Default)]
pub(crate) struct SourceAcquisitionStates {
    states: HashMap<std::path::PathBuf, SourceIdentityState>,
}

impl SourceAcquisitionStates {
    /// Returns whether a canonical identity may be acquired. A processing
    /// identity is a cycle; a completed identity is an intentional no-op.
    fn begin(&mut self, path: &std::path::Path) -> Result<bool, ()> {
        match self.states.get(path) {
            Some(SourceIdentityState::Processing) => Err(()),
            Some(SourceIdentityState::Completed) => Ok(false),
            None => {
                self.states
                    .insert(path.to_path_buf(), SourceIdentityState::Processing);
                Ok(true)
            }
        }
    }

    fn begin_source(&mut self, acquisition: &SourceAcquisition) {
        if let SourceAcquisition::FileSystem { canonical_path } = acquisition {
            self.states
                .entry(canonical_path.clone())
                .or_insert(SourceIdentityState::Processing);
        }
    }

    fn complete(&mut self, acquisition: &SourceAcquisition) {
        if let SourceAcquisition::FileSystem { canonical_path } = acquisition {
            if self.states.get(canonical_path) == Some(&SourceIdentityState::Processing) {
                self.states
                    .insert(canonical_path.clone(), SourceIdentityState::Completed);
            }
        }
    }

    fn remove(&mut self, path: &std::path::Path) {
        self.states.remove(path);
    }
}

/// Test/host seam for acquisition. The callback runs only between complete
/// forms and may register a new source in the same `SourceTexts` owner.
pub(crate) type AdditionalSourceHook<'a> = dyn FnMut(
        &mut SourceTexts,
        &mut SourceAcquisitionStates,
        AdditionalSourceRequest,
    ) -> Result<Option<SourceId>, crate::source_processor::SourceProcessorError>
    + 'a;

/// Owns source storage, publication state, and nested source frames for one
/// processing run. A frame retains only tokenized form data and a position;
/// it never retains a `SourceView`, allowing source registration at a form
/// boundary without cloning or snapshotting `SourceTexts`.
pub(crate) struct SourceProcessingSession {
    sources: SourceTexts,
    environment: BatchEnvironment,
    frames: Vec<SourceFrame>,
    acquisition_states: SourceAcquisitionStates,
}

impl SourceProcessingSession {
    fn new(sources: SourceTexts, initial_source_id: SourceId) -> Result<Self, BatchSetupError> {
        let environment = BatchEnvironment::new()?;
        Self::with_environment(sources, environment, initial_source_id)
    }

    fn with_environment(
        sources: SourceTexts,
        environment: BatchEnvironment,
        initial_source_id: SourceId,
    ) -> Result<Self, BatchSetupError> {
        let mut session = Self {
            sources,
            environment,
            frames: Vec::new(),
            acquisition_states: SourceAcquisitionStates::default(),
        };
        if let Err(error) = session.push_source(initial_source_id) {
            if let crate::source_processor::SourceProcessorError::Source(source) = error {
                return Err(BatchSetupError::InvalidInitialSource(source));
            }
            unreachable!("initial source tokenization cannot fail after ownership transfer");
        }
        Ok(session)
    }

    fn with_environment_and_cursor(
        sources: SourceTexts,
        environment: BatchEnvironment,
        source_id: SourceId,
        forms: SourceFormCursor,
    ) -> Self {
        let mut session = Self {
            sources,
            environment,
            frames: vec![SourceFrame {
                source_id,
                forms,
                form_index: 0,
            }],
            acquisition_states: SourceAcquisitionStates::default(),
        };
        let acquisition = session
            .sources
            .view()
            .acquisition(source_id)
            .expect("initial source must be registered")
            .clone();
        session.acquisition_states.begin_source(&acquisition);
        session
    }

    pub(crate) fn sources(&self) -> &SourceTexts {
        &self.sources
    }

    pub(crate) fn sources_mut(&mut self) -> &mut SourceTexts {
        &mut self.sources
    }

    pub(crate) fn push_source(
        &mut self,
        source_id: SourceId,
    ) -> Result<(), crate::source_processor::SourceProcessorError> {
        let acquisition = self.sources.view().acquisition(source_id)?.clone();
        self.acquisition_states.begin_source(&acquisition);
        let forms = SourceFormCursor::new(self.sources.view(), source_id)?;
        self.frames.push(SourceFrame {
            source_id,
            forms,
            form_index: 0,
        });
        Ok(())
    }

    /// Runs all frames depth-first. The hook is invoked after each complete
    /// form, when compiler and publication borrows have ended. Returning a
    /// source id pushes that source before the caller frame's next form.
    pub(crate) fn run_with_hook<W>(
        &mut self,
        writer: &mut W,
        hook: &mut AdditionalSourceHook<'_>,
    ) -> Result<Option<SourceRunResult>, crate::source_processor::SourceProcessorError>
    where
        W: Write + ?Sized,
    {
        let mut last_result = None;
        let mut data_stack: Vec<Value> = Vec::new();
        while !self.frames.is_empty() {
            let frame_index = self.frames.len() - 1;
            let unit = {
                let sources = &self.sources;
                let environment = &mut self.environment;
                let frame = &mut self.frames[frame_index];
                let context = environment.compile_context();
                frame.forms.compile_next_form(sources.view(), context)?
            };
            let Some(form) = unit else {
                let frame = self.frames.pop().expect("frame exists while running");
                let acquisition = self.sources.view().acquisition(frame.source_id)?.clone();
                self.acquisition_states.complete(&acquisition);
                continue;
            };
            {
                let code_spaces = [self.environment.published_code.instruction_view()];
                let source_mappings = [self.environment.published_code.source_mapping()];
                let mut output = WriteRuntimeOutput::new(&mut *writer);
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
                last_result = Some(run_unit_with_data_stack(&form.unit, context, &data_stack)?);
                data_stack = last_result
                    .as_ref()
                    .expect("the form result was just stored")
                    .data_stack()
                    .to_vec();
            }

            self.frames[frame_index].form_index += 1;
            if let Some(request) = form.additional_source {
                if let Some(additional_source_id) =
                    hook(&mut self.sources, &mut self.acquisition_states, request)?
                {
                    self.push_source(additional_source_id)?;
                }
            }
        }
        Ok(last_result)
    }

    pub(crate) fn run_with_filesystem<W>(
        &mut self,
        writer: &mut W,
    ) -> Result<Option<SourceRunResult>, SourceProcessorError>
    where
        W: Write + ?Sized,
    {
        let mut hook = |sources: &mut SourceTexts,
                        states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            acquire_filesystem_source_with_states(sources, states, request)
        };
        self.run_with_hook(writer, &mut hook)
    }
}

fn acquire_filesystem_source(
    sources: &mut SourceTexts,
    request: AdditionalSourceRequest,
) -> Result<SourceId, SourceProcessorError> {
    let mut states = SourceAcquisitionStates::default();
    let span = request.span;
    let specification = request.specification.clone();
    acquire_filesystem_source_with_states(sources, &mut states, request)?.ok_or(
        SourceProcessorError::AdditionalSourceAcquisition {
            span,
            specification,
            kind: AdditionalSourceAcquisitionError::Cycle,
        },
    )
}

fn acquire_filesystem_source_with_states(
    sources: &mut SourceTexts,
    states: &mut SourceAcquisitionStates,
    request: AdditionalSourceRequest,
) -> Result<Option<SourceId>, SourceProcessorError> {
    let view = sources.view();
    let requested_path = Path::new(request.specification.as_ref());
    let path = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        let acquisition = view.acquisition(request.span.source_id()).map_err(|_| {
            SourceProcessorError::AdditionalSourceAcquisition {
                span: request.span,
                specification: request.specification.clone(),
                kind: AdditionalSourceAcquisitionError::RelativePathRequiresFileSource,
            }
        })?;
        let SourceAcquisition::FileSystem { canonical_path } = acquisition else {
            return Err(SourceProcessorError::AdditionalSourceAcquisition {
                span: request.span,
                specification: request.specification.clone(),
                kind: AdditionalSourceAcquisitionError::RelativePathRequiresFileSource,
            });
        };
        let parent = canonical_path.parent().ok_or_else(|| {
            SourceProcessorError::AdditionalSourceAcquisition {
                span: request.span,
                specification: request.specification.clone(),
                kind: AdditionalSourceAcquisitionError::RelativePathRequiresFileSource,
            }
        })?;
        parent.join(requested_path)
    };

    let canonical_path = fs::canonicalize(&path).map_err(|source| {
        SourceProcessorError::AdditionalSourceAcquisition {
            span: request.span,
            specification: request.specification.clone(),
            kind: AdditionalSourceAcquisitionError::Canonicalize {
                path,
                message: source.to_string().into_boxed_str(),
            },
        }
    })?;
    match states.begin(&canonical_path) {
        Ok(false) => return Ok(None),
        Err(()) => {
            return Err(SourceProcessorError::AdditionalSourceAcquisition {
                span: request.span,
                specification: request.specification.clone(),
                kind: AdditionalSourceAcquisitionError::Cycle,
            })
        }
        Ok(true) => {}
    }
    let text = fs::read_to_string(&canonical_path).map_err(|source| {
        states.remove(&canonical_path);
        SourceProcessorError::AdditionalSourceAcquisition {
            span: request.span,
            specification: request.specification.clone(),
            kind: AdditionalSourceAcquisitionError::Read {
                path: canonical_path.clone(),
                message: source.to_string().into_boxed_str(),
            },
        }
    })?;

    Ok(Some(sources.register_with_acquisition(
        text,
        request.specification,
        SourceAcquisition::FileSystem { canonical_path },
    )))
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
        compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_word_and_runtime_publication_and_operators(
                &mut self.bindings,
                &mut self.source_words,
                self.operators.lookup(),
                &mut self.globals,
                &mut self.published_code,
                &mut self.words,
            ),
        )
    }

    fn compile_context(&mut self) -> SourceCompileContext<'_> {
        SourceCompileContext::with_source_word_and_runtime_publication_and_operators(
            &mut self.bindings,
            &mut self.source_words,
            self.operators.lookup(),
            &mut self.globals,
            &mut self.published_code,
            &mut self.words,
        )
        .with_additional_source_capability()
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

pub(crate) fn execute_registered_sources_with_filesystem<W>(
    sources: SourceTexts,
    stdlib_source_id: SourceId,
    source_id: SourceId,
    writer: &mut W,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut environment = match BatchEnvironment::new() {
        Ok(environment) => environment,
        Err(error) => return setup_failure(&sources, error),
    };

    if stdlib_source_id != source_id {
        if let Err(error) = environment.compile(&sources, stdlib_source_id) {
            return standard_library_failure(&sources, error);
        }
    }

    let forms = match SourceFormCursor::new(sources.view(), source_id) {
        Ok(forms) => forms,
        Err(error) => return user_facing_result(&sources, Err(error)),
    };
    let mut session = SourceProcessingSession::with_environment_and_cursor(
        sources,
        environment,
        source_id,
        forms,
    );

    match session.run_with_filesystem(writer) {
        Ok(Some(result)) => BatchExecutionResult::Success(result),
        Ok(None) => BatchExecutionResult::Success(
            // Empty input has no form result. The legacy path supplies the
            // established empty-run result without adding a second source.
            match execute_registered_sources(session.sources(), stdlib_source_id, source_id, writer)
            {
                BatchExecutionResult::Success(result) => result,
                BatchExecutionResult::Failure(failure) => {
                    return BatchExecutionResult::Failure(failure)
                }
            },
        ),
        Err(error) => user_facing_result(session.sources(), Err(error)),
    }
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
    let stdlib_source_id = register_embedded_standard_library(&mut sources);
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
    use crate::source::SourceTexts;
    use crate::user_facing::UserFacingFailureClass;
    use crate::value::Value;

    fn name(value: &str) -> crate::name::NormalizedName {
        crate::name::NormalizedName::new(value).expect("test name should be valid")
    }

    fn request_source_word(
        context: &mut crate::source_word::NativeSourceWordContext<'_, '_>,
    ) -> Result<(), crate::source_word::SourceWordError> {
        let specification = context.statement_reader_mut().read_name().map_err(|_| {
            crate::source_word::SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            }
        })?;
        context.process_additional_source(specification.span())?;
        context.statement_reader_mut().finish().map_err(|_| {
            crate::source_word::SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            }
        })
    }

    fn noop_source_word(
        _context: &mut crate::source_word::NativeSourceWordContext<'_, '_>,
    ) -> Result<(), crate::source_word::SourceWordError> {
        Ok(())
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
        let stdlib_source_id = register_embedded_standard_library(&mut sources);
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

    #[test]
    fn native_source_word_requests_nested_sources_and_returns_to_each_caller_form() {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("REQUEST B\nNOOP", "main.tbx");
        let mut session = SourceProcessingSession::new(sources, source_id)
            .expect("processing session should build");
        crate::bootstrap::register_native_source_word(
            &mut session.environment.source_words,
            &mut session.environment.bindings,
            name("REQUEST"),
            request_source_word,
        )
        .expect("test source word should register");
        crate::bootstrap::register_native_source_word(
            &mut session.environment.source_words,
            &mut session.environment.bindings,
            name("NOOP"),
            noop_source_word,
        )
        .expect("test no-op source word should register");
        let mut writer = RecordingWriter::default();
        let mut added_b = false;
        let mut added_c = false;
        let mut order_ids = Vec::new();
        let mut hook = |sources: &mut SourceTexts,
                        _states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            order_ids.push(request.span.source_id());
            match request.specification.as_ref() {
                "B" if !added_b => {
                    added_b = true;
                    Ok(Some(sources.register_with_acquisition(
                        "REQUEST C\nNOOP",
                        "nested-display.tbx",
                        crate::source::SourceAcquisition::FileSystem {
                            canonical_path: "/canonical/nested.tbx".into(),
                        },
                    )))
                }
                "C" if !added_c => {
                    added_c = true;
                    Ok(Some(sources.register("NOOP", "leaf.tbx")))
                }
                specification => panic!("unexpected additional source: {specification}"),
            }
        };

        session
            .run_with_hook(&mut writer, &mut hook)
            .expect("nested source should complete");
        assert_eq!(writer.text(), "");
        assert_eq!(order_ids.len(), 2);
        assert_eq!(order_ids.first(), Some(&source_id));
        assert_ne!(order_ids.get(1), Some(&source_id));
        assert_eq!(session.sources().len(), 3);
    }

    #[test]
    fn additional_source_capability_is_unavailable_inside_definition_body() {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("DEF FOO\nREQUEST B\nEND", "main.tbx");
        let mut session = SourceProcessingSession::new(sources, source_id)
            .expect("processing session should build");
        crate::bootstrap::register_native_source_word(
            &mut session.environment.source_words,
            &mut session.environment.bindings,
            name("REQUEST"),
            request_source_word,
        )
        .expect("test source word should register");
        let mut writer = RecordingWriter::default();
        let mut hook = |_sources: &mut SourceTexts,
                        _states: &mut SourceAcquisitionStates,
                        _request: AdditionalSourceRequest| {
            panic!("definition body must not receive additional source capability")
        };

        let error = session
            .run_with_hook(&mut writer, &mut hook)
            .expect_err("definition body request should be unavailable");
        assert!(
            matches!(
                &error,
                crate::source_processor::SourceProcessorError::SourceWord(
                    crate::source_word::SourceWordError::DefBodyCompile { .. }
                )
            ),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn filesystem_hook_resolves_nested_relative_sources_and_keeps_display_names() {
        let root =
            std::path::PathBuf::from(".tmp").join(format!("issue-1651-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).expect("fixture directory should be created");
        std::fs::create_dir_all(root.join("shared")).expect("fixture directory should be created");
        let main_path = root.join("main.tbx");
        let nested_path = root.join("sub/b.tbx");
        let leaf_path = root.join("shared/c.tbx");
        std::fs::write(&main_path, "REQUEST sub/b.tbx\nNOOP")
            .expect("main fixture should be written");
        std::fs::write(&nested_path, "REQUEST ../shared/c.tbx\nNOOP")
            .expect("nested fixture should be written");
        std::fs::write(&leaf_path, "NOOP").expect("leaf fixture should be written");

        let main_canonical = std::fs::canonicalize(&main_path).expect("main path should resolve");
        let leaf_canonical = std::fs::canonicalize(&leaf_path).expect("leaf path should resolve");
        let mut sources = SourceTexts::new();
        let source_id = sources.register_with_acquisition(
            "REQUEST sub/b.tbx\nNOOP",
            "requested/main.tbx",
            crate::source::SourceAcquisition::FileSystem {
                canonical_path: main_canonical.clone(),
            },
        );
        let span = sources
            .view()
            .span(source_id, 0, 1)
            .expect("request span should be valid");
        let absolute_id = acquire_filesystem_source(
            &mut sources,
            AdditionalSourceRequest {
                specification: leaf_canonical
                    .to_string_lossy()
                    .into_owned()
                    .into_boxed_str(),
                span,
            },
        )
        .expect("absolute source should be acquired");
        let nested_id = acquire_filesystem_source(
            &mut sources,
            AdditionalSourceRequest {
                specification: "sub/b.tbx".into(),
                span,
            },
        )
        .expect("nested source should be acquired");
        let nested_span = sources
            .view()
            .span(nested_id, 0, 1)
            .expect("nested request span should be valid");
        let leaf_id = acquire_filesystem_source(
            &mut sources,
            AdditionalSourceRequest {
                specification: "../shared/c.tbx".into(),
                span: nested_span,
            },
        )
        .expect("leaf source should be acquired");

        let view = sources.view();
        assert_eq!(
            view.display_name(absolute_id),
            Ok(leaf_canonical.to_string_lossy().as_ref())
        );
        assert_eq!(view.source(absolute_id), Ok("NOOP"));
        assert_eq!(
            view.acquisition(absolute_id),
            Ok(&crate::source::SourceAcquisition::FileSystem {
                canonical_path: leaf_canonical.clone(),
            })
        );
        assert_eq!(view.display_name(nested_id), Ok("sub/b.tbx"));
        assert_eq!(view.display_name(leaf_id), Ok("../shared/c.tbx"));
        assert_eq!(
            view.acquisition(nested_id),
            Ok(&crate::source::SourceAcquisition::FileSystem {
                canonical_path: std::fs::canonicalize(&nested_path)
                    .expect("nested path should resolve"),
            })
        );
        assert_eq!(
            view.acquisition(leaf_id),
            Ok(&crate::source::SourceAcquisition::FileSystem {
                canonical_path: leaf_canonical.clone(),
            })
        );
        assert_eq!(view.source(leaf_id), Ok("NOOP"));
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    fn filesystem_hook_rejects_relative_request_from_non_file_source() {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("REQUEST child.tbx", "<stdin>");
        let span = sources
            .view()
            .span(source_id, 0, "REQUEST child.tbx".len())
            .expect("request span should be valid");
        let error = acquire_filesystem_source(
            &mut sources,
            AdditionalSourceRequest {
                specification: "child.tbx".into(),
                span,
            },
        )
        .expect_err("stdin relative request must fail");
        assert!(matches!(
            error,
            SourceProcessorError::AdditionalSourceAcquisition {
                span: actual,
                specification,
                kind: AdditionalSourceAcquisitionError::RelativePathRequiresFileSource,
            } if actual == span && specification.as_ref() == "child.tbx"
        ));
    }

    #[test]
    fn filesystem_acquisition_skips_completed_identity_and_rejects_processing_identity() {
        let root = std::path::PathBuf::from(".tmp")
            .join(format!("issue-1658-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory should be created");
        let path = root.join("child.tbx");
        std::fs::write(&path, "NOOP").expect("fixture source should be written");
        let canonical_path = std::fs::canonicalize(&path).expect("fixture path should resolve");

        let mut sources = SourceTexts::new();
        let parent_id = sources.register("REQUEST child.tbx", "main.tbx");
        let span = sources
            .view()
            .span(parent_id, 0, 1)
            .expect("request span should be valid");
        let request = |specification: &str| AdditionalSourceRequest {
            specification: specification.into(),
            span,
        };

        let mut states = SourceAcquisitionStates::default();
        states
            .begin(&canonical_path)
            .expect("identity should begin");
        let cycle = acquire_filesystem_source_with_states(
            &mut sources,
            &mut states,
            request(canonical_path.to_string_lossy().as_ref()),
        )
        .expect_err("processing identity should be a cycle");
        assert!(matches!(
            cycle,
            SourceProcessorError::AdditionalSourceAcquisition {
                span: actual,
                specification,
                kind: AdditionalSourceAcquisitionError::Cycle,
            } if actual == span && specification.as_ref() == canonical_path.to_string_lossy().as_ref()
        ));
        assert_eq!(sources.len(), 1, "cycle must not register a source");

        states.complete(&SourceAcquisition::FileSystem {
            canonical_path: canonical_path.clone(),
        });
        let completed = acquire_filesystem_source_with_states(
            &mut sources,
            &mut states,
            request(canonical_path.to_string_lossy().as_ref()),
        )
        .expect("completed identity should be a no-op");
        assert_eq!(completed, None);
        assert_eq!(
            sources.len(),
            1,
            "completed identity must not register a source"
        );
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    fn top_level_filesystem_source_is_completed_only_after_its_frame_finishes() {
        let canonical_path = std::path::PathBuf::from(".tmp")
            .join(format!("issue-1658-top-level-{}.tbx", std::process::id()));
        let mut sources = SourceTexts::new();
        let source_id = sources.register_with_acquisition(
            "",
            "main.tbx",
            SourceAcquisition::FileSystem {
                canonical_path: canonical_path.clone(),
            },
        );
        let mut session = SourceProcessingSession::new(sources, source_id)
            .expect("processing session should build");
        assert_eq!(
            session.acquisition_states.states.get(&canonical_path),
            Some(&SourceIdentityState::Processing)
        );
        let mut writer = RecordingWriter::default();
        let mut hook = |_sources: &mut SourceTexts,
                        _states: &mut SourceAcquisitionStates,
                        _request: AdditionalSourceRequest| {
            unreachable!("source has no additional request")
        };
        session
            .run_with_hook(&mut writer, &mut hook)
            .expect("source should complete");
        assert_eq!(
            session.acquisition_states.states.get(&canonical_path),
            Some(&SourceIdentityState::Completed)
        );
    }
}
