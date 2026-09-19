use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::arithmetic_primitive::register_arithmetic_primitives;
use crate::binding::Bindings;
use crate::bootstrap::{
    register_builtin_global_variables, register_builtin_source_words, BuiltinGlobalBootstrapError,
    PrimitiveBootstrapError, SourceWordBootstrapError,
};
use crate::diagnostic::{DiagnosticRenderer, RenderedDiagnostic, UserDiagnostic};
use crate::global_array::GlobalArrays;
use crate::global_variable::GlobalVariables;
use crate::input_primitive::register_input_primitives;
use crate::operator::{register_named_operator_primitives, OperatorBootstrapError, OperatorWords};
use crate::output_primitive::register_output_primitives;
use crate::primitive::PrimitiveRegistry;
use crate::published_code::PublishedCode;
use crate::random::RandomState;
use crate::random_primitive::register_random_primitives;
use crate::runtime_input::RuntimeInput;
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
    Arithmetic(PrimitiveBootstrapError),
    Stack(PrimitiveBootstrapError),
    Output(PrimitiveBootstrapError),
    Input(PrimitiveBootstrapError),
    Random(PrimitiveBootstrapError),
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
    arrays: GlobalArrays,
    published_code: PublishedCode,
    random: RandomState,
}

const DEFAULT_RANDOM_SEED: u64 = 0x5442_582D_4E45_5854;

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
    failed: bool,
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
            failed: false,
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
            failed: false,
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
        input: Option<&mut dyn RuntimeInput>,
    ) -> Result<Option<SourceRunResult>, crate::source_processor::SourceProcessorError>
    where
        W: Write + ?Sized,
    {
        if self.failed {
            return Err(crate::source_processor::SourceProcessorError::ProcessingSessionFailed);
        }
        let result = self.run_with_hook_inner(writer, hook, input);
        if result.is_err() {
            // A session that has started processing cannot be safely resumed:
            // publication and runtime state may already contain prior forms.
            // Keeping the session failed avoids defining retry semantics here.
            self.failed = true;
        }
        result
    }

    fn run_with_hook_inner<W>(
        &mut self,
        writer: &mut W,
        hook: &mut AdditionalSourceHook<'_>,
        mut input: Option<&mut dyn RuntimeInput>,
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
                .with_mut_arrays(self.environment.arrays.view_mut())
                .with_random(&mut self.environment.random)
                .with_output(&mut output);
                let context = match input.as_deref_mut() {
                    Some(input) => context.with_input(input),
                    None => context,
                };
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
        input: Option<&mut dyn RuntimeInput>,
    ) -> Result<Option<SourceRunResult>, SourceProcessorError>
    where
        W: Write + ?Sized,
    {
        let mut hook = |sources: &mut SourceTexts,
                        states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            acquire_filesystem_source_with_states(sources, states, request)
        };
        self.run_with_hook(writer, &mut hook, input)
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
        Self::new_with_seed(DEFAULT_RANDOM_SEED)
    }

    fn new_with_seed(seed: u64) -> Result<Self, BatchSetupError> {
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let operators =
            register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                .map_err(BatchSetupError::Operators)?;
        register_arithmetic_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Arithmetic)?;
        register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Stack)?;
        register_output_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Output)?;
        register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Input)?;
        register_random_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Random)?;

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
            arrays: GlobalArrays::new(),
            published_code: PublishedCode::new(),
            random: RandomState::seeded(seed),
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
            )
            .with_global_arrays(&mut self.arrays),
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
        .with_global_arrays(&mut self.arrays)
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
    execute_registered_sources_with_seed(
        sources,
        stdlib_source_id,
        source_id,
        writer,
        DEFAULT_RANDOM_SEED,
    )
}

pub(crate) fn execute_registered_sources_with_seed<W>(
    sources: &SourceTexts,
    stdlib_source_id: SourceId,
    source_id: SourceId,
    writer: &mut W,
    seed: u64,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut environment = match BatchEnvironment::new_with_seed(seed) {
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
            .with_mut_arrays(environment.arrays.view_mut())
            .with_random(&mut environment.random)
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
    input: Option<&mut dyn RuntimeInput>,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    execute_registered_sources_with_filesystem_and_seed(
        sources,
        stdlib_source_id,
        source_id,
        writer,
        input,
        DEFAULT_RANDOM_SEED,
    )
}

pub(crate) fn execute_registered_sources_with_filesystem_and_seed<W>(
    sources: SourceTexts,
    stdlib_source_id: SourceId,
    source_id: SourceId,
    writer: &mut W,
    input: Option<&mut dyn RuntimeInput>,
    seed: u64,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut environment = match BatchEnvironment::new_with_seed(seed) {
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

    match session.run_with_filesystem(writer, input) {
        Ok(Some(result)) => BatchExecutionResult::Success(result),
        Ok(None) => BatchExecutionResult::Success(
            // Empty input has no form result. The legacy path supplies the
            // established empty-run result without adding a second source.
            match execute_registered_sources_with_seed(
                session.sources(),
                stdlib_source_id,
                source_id,
                writer,
                seed,
            ) {
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
    use crate::runtime_input::TestInput;
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
        let text = "LET A = 4\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(A)\nPUTDEC\nCR";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(4)]);
        assert_eq!(writer.text(), "4\n");
    }

    #[test]
    fn global_arrays_support_expression_reads_and_indexed_writes() {
        let text = "DIM @DATA[4]\nLET I = 2\nLET @DATA[1] = 7\nLET @DATA[I + 1] = @DATA[1] + 5\nEVAL @DATA[1]\nEVAL @DATA[(I + 1)]\nLET @DATA[4] = 9\nEVAL @DATA[4]";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            result.data_stack(),
            [Value::integer(7), Value::integer(12), Value::integer(9)]
        );
    }

    #[test]
    fn array_element_access_resolves_names_case_insensitively() {
        let (sources, source_id) = source(
            "DIM @Scores[2]\nLET @sCoReS[2] = 11\nEVAL @SCORES[2]",
            "program.tbx",
        );
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(11)]);
    }

    #[test]
    fn array_index_runtime_errors_are_preserved() {
        for index in ["0", "-1", "3"] {
            let text = format!("DIM @A[2]\nEVAL @A[{index}]");
            let (sources, source_id) = source(&text, "program.tbx");
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(
                failure.class(),
                UserFacingFailureClass::UserProgram,
                "{index}"
            );
        }
    }

    #[test]
    fn array_access_rejects_non_arrays_and_malformed_forms_at_compile_time() {
        for text in [
            "DIM @DATA[2]\nEVAL @DATA",
            "DIM @DATA[2]\nEVAL @DATA[]",
            "DIM @DATA[2]\nEVAL @DATA[1",
            "DIM @DATA[2]\nEVAL @DATA[1] 2",
            "EVAL @MISSING[1]",
            "EVAL @Z[1]",
            "EVAL @ABS[1]",
            "EVAL @LET[1]",
        ] {
            let (sources, source_id) = source(text, "program.tbx");
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(
                failure.class(),
                UserFacingFailureClass::UserProgram,
                "{text}"
            );
            assert!(
                failure.diagnostic().primary().is_some(),
                "{text}: {:?}",
                failure
            );
        }
    }

    #[test]
    fn putchr_accepts_character_hex_and_triple_quote_literals_from_source() {
        let text = "PUTCHR 'A'\nPUTCHR $41\nPUTCHR '''\nPUTCHR $27\nPUTCHR $0A";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), []);
        assert_eq!(writer.text(), "AA''\n");
    }

    #[test]
    fn putchr_reports_ascii_range_errors_after_hex_literal_evaluation() {
        for text in ["PUTCHR -1", "PUTCHR 128", "PUTCHR $F1"] {
            let (sources, source_id) = source(text, "program.tbx");
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(
                failure.class(),
                UserFacingFailureClass::UserProgram,
                "{text}"
            );
            assert_eq!(writer.text(), "", "{text}");
        }
    }

    #[test]
    fn abs_is_available_as_a_runtime_word_in_ordinary_expressions() {
        let (sources, source_id) =
            source("EVAL ABS(0)\nEVAL ABS(42)\nEVAL ABS(-42)", "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            result.data_stack(),
            [Value::integer(0), Value::integer(42), Value::integer(42)]
        );
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn abs_minimum_integer_reports_a_runtime_failure_from_source() {
        let (sources, source_id) = source("EVAL ABS(-32768)", "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn print_lowers_fixed_text_and_integer_expressions_in_source_order() {
        let text = "LET A = 42\nLET B = 5\nPRINT \"Im \", A, \" years old. TOTAL = \", A + B";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(writer.text(), "Im 42 years old. TOTAL = 47");
    }

    #[test]
    fn print_does_not_emit_for_commas_or_add_a_newline() {
        let (sources, source_id) = source("PRINT \"A\", \"B\"", "program.tbx");
        let mut writer = RecordingWriter::default();

        success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(writer.text(), "AB");
    }

    #[test]
    fn print_rejects_missing_items_and_invalid_item_separators() {
        for text in [
            "PRINT",
            "PRINT , 1",
            "PRINT 1,",
            "PRINT 1,,2",
            "PRINT \"A\" + 1",
        ] {
            let (sources, source_id) = source(text, "program.tbx");
            let mut writer = RecordingWriter::default();

            assert!(
                matches!(
                    execute_registered_source(&sources, source_id, &mut writer),
                    BatchExecutionResult::Failure(_)
                ),
                "{text} should be rejected"
            );
        }
    }

    #[test]
    fn print_does_not_execute_items_after_expression_failure() {
        let (sources, source_id) = source("PRINT \"before\", 1 / 0, \"after\"", "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(writer.text(), "before");
    }

    #[test]
    fn print_does_not_execute_items_after_output_failure() {
        let (sources, source_id) = source("PRINT \"before\", 7, \"after\"", "program.tbx");
        let mut writer = RecordingWriter::failing_after(1);

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert_eq!(writer.text(), "before");
    }

    #[test]
    fn print_lowers_local_references_inside_a_definition() {
        let text = "DEF SHOW value\nPRINT \"value=\", value\nEND\nEVAL SHOW(7)";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(writer.text(), "value=7");
    }

    #[test]
    fn print_syntax_diagnostics_point_at_the_invalid_separator_or_item() {
        for (text, expected_column) in [("PRINT 1,", 8), ("PRINT 1,,2", 9), ("PRINT 1 2", 9)] {
            let (sources, source_id) = source(text, "program.tbx");
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
            let primary = failure.diagnostic().primary().unwrap_or_else(|| {
                panic!("PRINT syntax failure should have a source position: {text}")
            });
            assert_eq!(primary.line_number(), 1, "{text}");
            assert_eq!(primary.column_number(), expected_column, "{text}");
            assert_eq!(primary.source_line(), text, "{text}");
        }
    }

    #[test]
    fn invalid_extra_character_literal_quotes_keep_a_source_diagnostic_span() {
        let text = "EVAL ''''";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
        let primary = failure
            .diagnostic()
            .primary()
            .expect("diagnostic should have a span");

        assert_eq!(primary.line_number(), 1);
        assert_eq!(primary.column_number(), 6);
        assert_eq!(primary.source_line(), text);
    }

    #[test]
    fn print_preserves_expression_diagnostics_for_undefined_names() {
        let (sources, source_id) = source("PRINT A + MISSING", "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
        let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
            panic!("PRINT expression failure should be a source failure");
        };

        assert!(matches!(
            user_failure.original_error(),
            SourceProcessorError::SourceWord(crate::source_word::SourceWordError::Expression {
                source: crate::expression::ExpressionError::Variable(_),
            })
        ));
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
    fn user_syntax_can_resolve_and_emit_runtime_words_variables_and_integers() {
        let text = "SYNTAX EMIT
STATEMENT
READ_NAME AS variable
RESOLVE_VAR variable AS target
EMIT_LOAD target
READ_NAME AS word
RESOLVE_WORD word AS callable
EMIT_CALL callable
EMIT_INT 7
ENDS
LET A = 3
EMIT A DUP";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            result.data_stack(),
            [Value::integer(3), Value::integer(3), Value::integer(7)]
        );
    }

    #[test]
    fn user_syntax_emits_control_value_operations_and_executes_them_as_a_lifo() {
        let text = "SYNTAX CONTROL
STATEMENT
READ_EXPR AS value
EXPECT_END
EMIT_EXPR value
EMIT_CONTROL_PUSH
EMIT_CONTROL_COPY
EMIT_CONTROL_DROP
ENDS
CONTROL 42";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(42)]);
    }

    #[test]
    fn user_syntax_control_value_operations_restore_an_outer_lifo_value() {
        let text = "SYNTAX PUSH_CONTROL
STATEMENT
READ_EXPR AS value
EXPECT_END
EMIT_EXPR value
EMIT_CONTROL_PUSH
ENDS
SYNTAX COPY_CONTROL
STATEMENT
EXPECT_END
EMIT_CONTROL_COPY
ENDS
SYNTAX DROP_CONTROL
STATEMENT
EXPECT_END
EMIT_CONTROL_DROP
ENDS
PUSH_CONTROL 10
PUSH_CONTROL 20
COPY_CONTROL
DROP_CONTROL
COPY_CONTROL
DROP_CONTROL";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            result.data_stack(),
            [Value::integer(20), Value::integer(10)]
        );
    }

    #[test]
    fn user_syntax_control_value_underflow_reaches_the_runtime_error() {
        let text = "SYNTAX COPY_CONTROL
STATEMENT
EXPECT_END
EMIT_CONTROL_COPY
ENDS
COPY_CONTROL";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
            panic!("control-value underflow should be a source runtime failure");
        };
        let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
            panic!("control-value underflow should preserve the runtime error");
        };
        assert!(matches!(
            error.vm().kind(),
            crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
        ));
    }

    #[test]
    fn control_value_emit_operations_reject_operands() {
        for operation in [
            "EMIT_CONTROL_PUSH",
            "EMIT_CONTROL_COPY",
            "EMIT_CONTROL_DROP",
        ] {
            let text = format!("SYNTAX CONTROL\nSTATEMENT\n{operation} EXTRA\nENDS\nCONTROL");
            let (sources, source_id) = source(&text, "program.tbx");
            let mut writer = RecordingWriter::default();
            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

            assert!(matches!(
                failure.cause,
                BatchExecutionFailureCause::Source(_)
            ));
        }
    }

    #[test]
    fn user_syntax_can_resolve_a_fixed_runtime_word_at_use_time() {
        let text = "SYNTAX LATER_CALL\nSTATEMENT\nRESOLVE_WORD_LITERAL LATER AS word\nEMIT_CALL word\nENDS\nDEF LATER\nDUP\nEND\nEVAL 7\nLATER_CALL";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(7)]);
    }

    #[test]
    fn fixed_runtime_word_resolution_reports_the_literal_operand_span() {
        for (text, expected_line) in [
            (
                "SYNTAX FIXED_CALL\nSTATEMENT\nRESOLVE_WORD_LITERAL MISSING AS word\nEMIT_CALL word\nENDS\nFIXED_CALL",
                "RESOLVE_WORD_LITERAL MISSING AS word",
            ),
            (
                "SYNTAX FIXED_CALL\nSTATEMENT\nRESOLVE_WORD_LITERAL A AS word\nEMIT_CALL word\nENDS\nLET A = 1\nFIXED_CALL",
                "RESOLVE_WORD_LITERAL A AS word",
            ),
        ] {
            let (sources, source_id) = source(text, "program.tbx");
            let mut writer = RecordingWriter::default();
            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
            let primary = failure
                .diagnostic()
                .primary()
                .expect("fixed runtime word failure should retain a source span");

            assert_eq!(primary.source_line(), expected_line);
            assert!(primary.column_number() > "RESOLVE_WORD_LITERAL ".len());
        }
    }

    #[test]
    fn user_syntax_reports_runtime_emit_binding_and_literal_errors_at_source_spans() {
        for text in [
            "SYNTAX S\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_WORD name AS word\nENDS\nLET A = 1\nS A",
            "SYNTAX S\nSTATEMENT\nEMIT_INT 32768\nENDS\nS",
        ] {
            let (sources, source_id) = source(text, "program.tbx");
            let mut writer = RecordingWriter::default();
            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
            let primary = failure
                .diagnostic()
                .primary()
                .expect("invalid emit should retain a source span");

            assert!(!primary.source_line().is_empty());
        }
    }

    #[test]
    fn user_syntax_reports_undefined_runtime_word_at_name_span() {
        let text =
            "SYNTAX S\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_WORD name AS word\nENDS\nS MISSING";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
        let primary = failure
            .diagnostic()
            .primary()
            .expect("undefined runtime word should retain a source span");

        assert!(!primary.source_line().is_empty());
        assert!(primary.column_number() > 0);
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
        let (sources, source_id) = source("EVAL 7\nPUTDEC", "program.tbx");
        let mut writer = RecordingWriter::failing_after(0);

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure.diagnostic().primary().is_some());
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn successful_runtime_output_is_not_rolled_back_by_a_later_failure() {
        let (sources, source_id) = source("EVAL 7\nPUTDEC\nCR", "program.tbx");
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
            sources_with_standard_library("EVAL 99\nPUTDEC\nCR", "EVAL 1\nPUTDEC\nCR");
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
            sources_with_standard_library("UNKNOWN", "EVAL 7\nPUTDEC");
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
            sources_with_standard_library("?", "EVAL 7\nPUTDEC");
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
            sources_with_standard_library("SYNTAX A\nSTATEMENT\nENDS", "EVAL 7\nPUTDEC");
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
            "LET A = 0\nWHILE A < 3\nLET A = A + 1\nENDWH\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(3)]);

        for (source, expected) in [
            ("WHILE 0\nLET A = 1\nENDWH\nEVAL 0", 0),
            ("LET A = 0\nWHILE A < 1\nLET A = A + 1\nENDWH\nEVAL A", 1),
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
    fn embedded_standard_library_break_exits_while_do_and_for_without_running_terminators() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nWHILE 1\nLET A = A + 1\nBREAK\nLET A = A + 100\nENDWH\nEVAL A",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(1)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nDO\nLET A = A + 1\nBREAK\nLET A = A + 100\nUNTIL 1\nEVAL A",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(1)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = 1 TO 3\nLET A = A + I\nBREAK\nLET A = A + 100\nNEXT\nEVAL A\nEVAL I",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(1), Value::integer(1)]);
    }

    #[test]
    fn embedded_standard_library_break_transparently_exits_through_if_and_select() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nWHILE A < 1\nIF 1\nBREAK\nENDIF\nLET A = A + 100\nENDWH\nEVAL A",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(0)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "FOR I = 1 TO 1\nSELECT I\nCASE 1\nBREAK\nENDSEL\nNEXT\nEVAL I",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(1)]);
    }

    #[test]
    fn embedded_standard_library_break_cleans_nested_control_values() {
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_with_embedded_standard_library(
            "SYNTAX COPY_CONTROL\nSTATEMENT\nEXPECT_END\nEMIT_CONTROL_COPY\nENDS\nFOR I = 1 TO 1\nSELECT I\nCASE 1\nIF 1\nBREAK\nENDIF\nENDSEL\nNEXT\nCOPY_CONTROL",
            "program.tbx",
            &mut writer,
        ));
        let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
            panic!("control-value cleanup probe should fail in user runtime code");
        };
        let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
            panic!("cleanup probe should preserve the runtime error");
        };
        assert!(matches!(
            error.vm().kind(),
            crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
        ));

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nWHILE A < 2\nLET B = 0\nWHILE B < 1\nBREAK\nENDWH\nLET A = A + 1\nENDWH\nEVAL A",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(2)]);
    }

    #[test]
    fn embedded_standard_library_break_requires_a_loop_and_rejects_trailing_tokens() {
        for (source, expected_column) in [("BREAK", 1), ("BREAK X", 7)] {
            let mut writer = RecordingWriter::default();
            let failure = failure(execute_with_embedded_standard_library(
                source,
                "program.tbx",
                &mut writer,
            ));
            assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
            let primary = failure
                .diagnostic
                .primary()
                .expect("BREAK diagnostic should have a primary span");
            assert_eq!(primary.display_name(), "program.tbx");
            assert_eq!(primary.line_number(), 1);
            assert_eq!(primary.column_number(), expected_column);
        }

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "DEF LOOP_BREAK\nWHILE 1\nBREAK\nENDWH\nEND\nWHILE 1\nLOOP_BREAK\nBREAK\nENDWH\nEVAL 1",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(1)]);

        let mut writer = RecordingWriter::default();
        let failure = failure(execute_with_embedded_standard_library(
            "DEF INVALID_BREAK\nBREAK\nEND",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
    }

    #[test]
    fn embedded_standard_library_while_has_one_line_number_scope_across_body() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nWHILE A < 1\nBIF 0, 20\n10 LET A = A + 1\n20 LET A = A + 1\nENDWH\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(1)]);
    }

    #[test]
    fn line_number_on_structured_start_statement_belongs_to_enclosing_scope() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "BIF 0, 100\n100 WHILE 0\nLET A = 1\nENDWH\nEVAL 7",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(7)]);
    }

    #[test]
    fn structured_body_keeps_enclosing_publication_capability() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "WHILE 0\nVAR SCORE\nENDWH\nLET SCORE = 3\nEVAL SCORE",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(3)]);
    }

    #[test]
    fn structured_marker_cannot_become_a_line_number_target() {
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_with_embedded_standard_library(
            "WHILE 1\n10 ENDWH",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
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
    fn embedded_standard_library_for_repeats_with_a_fixed_end_value() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = 1 TO 3\nLET A = A + I\nNEXT\nEVAL A\nEVAL I",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(6), Value::integer(4)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nLET B = 3\nFOR I = 1 TO B\nLET A = A + 1\nLET B = 1\nNEXT\nEVAL A\nEVAL I",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(3), Value::integer(4)]);
    }

    #[test]
    fn embedded_standard_library_for_evaluates_bounds_in_order_once_and_can_skip_body() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = A + 1 TO A + 2\nLET A = A + 10\nNEXT\nEVAL A\nEVAL I",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(20), Value::integer(3)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = 3 TO 1\nLET A = A + 1\nNEXT\nEVAL A\nEVAL I",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(0), Value::integer(3)]);
    }

    #[test]
    fn embedded_standard_library_for_evaluates_side_effecting_bounds_once_in_order() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nDEF START_BOUND\nLET A = A + 1\nEVAL A\nEND\nDEF END_BOUND\nLET A = A + 10\nEVAL A\nEND\nFOR I = START_BOUND() TO END_BOUND()\nNEXT\nEVAL A\nEVAL I",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(
            result.data_stack(),
            [Value::integer(11), Value::integer(12)]
        );
    }

    #[test]
    fn embedded_standard_library_for_supports_nested_for_loops() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = 1 TO 2\nFOR J = 1 TO 3\nLET A = A + 1\nNEXT\nNEXT\nEVAL A\nEVAL I\nEVAL J",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(
            result.data_stack(),
            [Value::integer(6), Value::integer(3), Value::integer(4)]
        );
    }

    #[test]
    fn embedded_standard_library_for_uses_the_modified_counter_and_preserves_body_stack_values() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = 1 TO 3\nEVAL I\nLET I = I + 1\nNEXT",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(1), Value::integer(3)]);
    }

    #[test]
    fn embedded_standard_library_for_supports_nested_structured_blocks_and_case_insensitive_to() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nFOR I = 1 to 2\nIF 1\nWHILE A < 1\nDO\nLET A = A + 1\nUNTIL A >= 1\nENDWH\nENDIF\nSELECT I\nCASE 1\nLET A = A + 10\nCASE 2\nLET A = A + 100\nENDSEL\nNEXT\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(111)]);
    }

    #[test]
    fn embedded_standard_library_for_rejects_non_variable_bindings_and_cleans_up_control_value() {
        let mut writer = RecordingWriter::default();
        let binding_failure = failure(execute_with_embedded_standard_library(
            "FOR MISSING = 1 TO 2\nNEXT",
            "program.tbx",
            &mut writer,
        ));
        assert!(matches!(
            binding_failure.cause,
            BatchExecutionFailureCause::Source(_)
        ));

        let mut writer = RecordingWriter::default();
        let non_variable_failure = failure(execute_with_embedded_standard_library(
            "FOR ADD = 1 TO 2\nNEXT",
            "program.tbx",
            &mut writer,
        ));
        assert!(matches!(
            non_variable_failure.cause,
            BatchExecutionFailureCause::Source(_)
        ));

        let mut writer = RecordingWriter::default();
        let failure = failure(execute_with_embedded_standard_library(
            "SYNTAX COPY_CONTROL\nSTATEMENT\nEXPECT_END\nEMIT_CONTROL_COPY\nENDS\nDEF CHECK\nFOR I = 1 TO 1\nNEXT\nCOPY_CONTROL\nEND\nCHECK",
            "program.tbx",
            &mut writer,
        ));
        let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
            panic!("control-value cleanup probe should fail in user runtime code");
        };
        let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
            panic!("cleanup probe should preserve the runtime error");
        };
        assert!(matches!(
            error.vm().kind(),
            crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
        ));
    }

    #[test]
    fn embedded_standard_library_select_matches_cases_without_fallthrough() {
        for (selector, expected) in [(1, 10), (2, 20), (3, 30), (9, 0)] {
            let mut writer = RecordingWriter::default();
            let source = format!(
                "SELECT {selector}\nCASE 1\nEVAL 10\nCASE 2\nEVAL 20\nCASE 3\nEVAL 30\nENDSEL\nEVAL 0"
            );
            let result = success(execute_with_embedded_standard_library(
                &source,
                "program.tbx",
                &mut writer,
            ));

            let expected_stack = if selector == 9 {
                vec![Value::integer(0)]
            } else {
                vec![Value::integer(expected), Value::integer(0)]
            };
            assert_eq!(result.data_stack(), expected_stack);
        }

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "SELECT 9\nCASE 1\nEVAL 10\nCASE_ELSE\nEVAL 99\nENDSEL",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(99)]);
    }

    #[test]
    fn embedded_standard_library_select_evaluates_selector_once_and_supports_nesting() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "DEF INC\nLET A = A + 1\nEVAL A\nEND\nLET A = 0\nSELECT INC()\nCASE 1\nSELECT 2\nCASE 2\nEVAL 7\nENDSEL\nENDSEL\nEVAL A",
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(1)]);
    }

    #[test]
    fn embedded_standard_library_select_cleans_up_selector_before_following_runtime_code() {
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_with_embedded_standard_library(
            "SYNTAX COPY_CONTROL\nSTATEMENT\nEXPECT_END\nEMIT_CONTROL_COPY\nENDS\nDEF CHECK\nSELECT 1\nCASE 1\nEVAL 7\nENDSEL\nCOPY_CONTROL\nEND\nCHECK",
            "program.tbx",
            &mut writer,
        ));

        let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
            panic!("control-value cleanup probe should fail in user runtime code");
        };
        let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
            panic!("cleanup probe should preserve the runtime error");
        };
        assert!(matches!(
            error.vm().kind(),
            crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
        ));
    }

    #[test]
    fn embedded_standard_library_select_nests_inside_if_while_and_do() {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nIF 1\nSELECT 1\nCASE 1\nEVAL 10\nENDSEL\nENDIF\nWHILE A < 1\nSELECT 2\nCASE 2\nEVAL 20\nENDSEL\nLET A = A + 1\nENDWH\nDO\nSELECT 3\nCASE 3\nEVAL 30\nENDSEL\nUNTIL 1\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(
            result.data_stack(),
            [
                Value::integer(10),
                Value::integer(20),
                Value::integer(30),
                Value::integer(1)
            ]
        );
    }

    #[test]
    fn embedded_standard_library_select_contains_if_while_and_do_bodies() {
        for (selector, body, expected) in [
            (1, "IF 1\nEVAL 10\nENDIF", 10),
            (
                2,
                "LET A = 0\nWHILE A < 1\nEVAL 20\nLET A = A + 1\nENDWH",
                20,
            ),
            (3, "DO\nEVAL 30\nUNTIL 1", 30),
        ] {
            let mut writer = RecordingWriter::default();
            let source = format!(
                "SELECT {selector}\nCASE 1\n{}\nCASE 2\n{}\nCASE 3\n{}\nCASE_ELSE\nEVAL 96\nENDSEL",
                if selector == 1 { body } else { "EVAL 99" },
                if selector == 2 { body } else { "EVAL 98" },
                if selector == 3 { body } else { "EVAL 97" },
            );
            let result = success(execute_with_embedded_standard_library(
                &source,
                "program.tbx",
                &mut writer,
            ));

            assert_eq!(result.data_stack(), [Value::integer(expected)]);
        }
    }

    #[test]
    fn embedded_standard_library_select_requires_cases_and_orders_else_last() {
        for source in [
            "SELECT 1\nENDSEL",
            "SELECT 1\nCASE_ELSE\nENDSEL",
            "SELECT 1\nCASE_ELSE\nCASE 1\nENDSEL",
            "SELECT 1\nCASE 1\nCASE_ELSE\nCASE_ELSE\nENDSEL",
        ] {
            let mut writer = RecordingWriter::default();
            let failure = failure(execute_with_embedded_standard_library(
                source,
                "program.tbx",
                &mut writer,
            ));
            assert!(matches!(
                failure.cause,
                BatchExecutionFailureCause::Source(_)
            ));
        }
    }

    #[test]
    fn embedded_standard_library_control_structures_support_nested_and_native_if_blocks() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nLET B = 0\nIF 1\nWHILE A < 2\nDO\nLET B = B + 1\nUNTIL B >= 2\nLET A = A + 1\nENDWH\nENDIF\nEVAL A\nEVAL B",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(3)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nDO\nLET B = 0\nWHILE B < 2\nLET B = B + 1\nENDWH\nLET A = A + 1\nUNTIL A >= 2\nEVAL A",
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(2)]);

        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            "LET A = 0\nLET B = 0\nWHILE A < 2\nLET B = 0\nWHILE B < 2\nLET B = B + 1\nENDWH\nLET A = A + 1\nENDWH\nEVAL A\nEVAL B",
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
        let Some(Binding::SourceWord(if_id)) = environment.bindings.get(&name("IF")) else {
            panic!("IF should publish as a source word from the embedded stdlib");
        };
        for (source_name, expected_exit_target, expected_ownership) in [
            ("IF", false, 0),
            ("WHILE", true, 0),
            ("DO", true, 0),
            ("FOR", true, 1),
            ("SELECT", false, 1),
        ] {
            let Some(Binding::SourceWord(id)) =
                environment.bindings.get(&name(source_name)).copied()
            else {
                panic!("{source_name} should publish as a source word");
            };
            let crate::source_word::SourceWordDispatch::Structured {
                implementation:
                    crate::source_word::StructuredSourceWordDispatch::UserDefined(implementation),
                ..
            } = environment
                .source_words
                .lookup()
                .lookup_dispatch(id)
                .expect("stdlib source word should dispatch")
            else {
                panic!("{source_name} should use a user-defined structured implementation");
            };
            assert_eq!(
                implementation.exit_target(),
                expected_exit_target,
                "{source_name}"
            );
            assert_eq!(
                implementation.control_value_ownership(),
                expected_ownership,
                "{source_name}"
            );
        }
        assert_eq!(
            environment
                .bindings
                .syntax_marker_reservation(&name("ENDWH"))
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
        for marker_name in ["ELSIF", "ELSE", "ENDIF"] {
            assert_eq!(
                environment
                    .bindings
                    .syntax_marker_reservation(&name(marker_name))
                    .map(|reservation| reservation.owner()),
                Some(*if_id),
                "{marker_name} should be owned by IF"
            );
        }
        assert!(STDLIB_SOURCE.contains("SYNTAX WHILE"));
        assert!(STDLIB_SOURCE.contains("SYNTAX DO"));
        assert!(STDLIB_SOURCE.contains("SYNTAX IF"));
    }

    #[test]
    fn embedded_standard_library_control_structure_markers_reject_binding_and_owner_mismatch() {
        for source in [
            "DEF ENDWH\nEND",
            "WHILE 1\nWEND",
            "WHILE 1\nUNTIL 1",
            "DO\nENDWH",
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
    fn embedded_standard_library_allows_wend_as_a_regular_word_name() {
        let mut writer = RecordingWriter::default();

        let result = success(execute_with_embedded_standard_library(
            "DEF WEND\nEND",
            "program.tbx",
            &mut writer,
        ));

        assert!(result.data_stack().is_empty());
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
            .run_with_hook(&mut writer, &mut hook, None)
            .expect("nested source should complete");
        assert_eq!(writer.text(), "");
        assert_eq!(order_ids.len(), 2);
        assert_eq!(order_ids.first(), Some(&source_id));
        assert_ne!(order_ids.get(1), Some(&source_id));
        assert_eq!(session.sources().len(), 3);
    }

    #[test]
    fn builtin_use_source_word_requests_nested_source_with_literal_specification() {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("USE \"B\"\nNOOP", "main.tbx");
        let mut session = SourceProcessingSession::new(sources, source_id)
            .expect("processing session should build");
        crate::bootstrap::register_native_source_word(
            &mut session.environment.source_words,
            &mut session.environment.bindings,
            name("NOOP"),
            noop_source_word,
        )
        .expect("test no-op source word should register");
        let mut writer = RecordingWriter::default();
        let mut requested = Vec::new();
        let mut hook = |sources: &mut SourceTexts,
                        _states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            requested.push((request.specification, request.span.source_id()));
            Ok(Some(sources.register("NOOP", "nested.tbx")))
        };

        session
            .run_with_hook(&mut writer, &mut hook, None)
            .expect("USE should complete through the normal source-word path");

        assert_eq!(requested, vec![("B".into(), source_id)]);
        assert_eq!(session.sources().len(), 2);
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
            .run_with_hook(&mut writer, &mut hook, None)
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
    fn builtin_use_in_definition_body_cannot_request_additional_source() {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("DEF FOO\nUSE \"B\"\nEND", "main.tbx");
        let mut session = SourceProcessingSession::new(sources, source_id)
            .expect("processing session should build");
        let mut writer = RecordingWriter::default();
        let mut hook = |_sources: &mut SourceTexts,
                        _states: &mut SourceAcquisitionStates,
                        _request: AdditionalSourceRequest| {
            panic!("definition body must not receive additional source capability")
        };

        let error = session
            .run_with_hook(&mut writer, &mut hook, None)
            .expect_err("USE in a definition body should be unavailable");
        assert!(matches!(
            error,
            crate::source_processor::SourceProcessorError::SourceWord(
                crate::source_word::SourceWordError::DefBodyCompile { .. }
            )
        ));
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
            .run_with_hook(&mut writer, &mut hook, None)
            .expect("source should complete");
        assert_eq!(
            session.acquisition_states.states.get(&canonical_path),
            Some(&SourceIdentityState::Completed)
        );
    }

    fn register_request_word_for_session(session: &mut SourceProcessingSession) {
        crate::bootstrap::register_native_source_word(
            &mut session.environment.source_words,
            &mut session.environment.bindings,
            name("REQUEST"),
            request_source_word,
        )
        .expect("test source word should register");
    }

    #[test]
    fn filesystem_session_detects_indirect_cycle_before_registering_cycle_source() {
        let root = std::path::PathBuf::from(".tmp")
            .join(format!("issue-1658-cycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory should be created");
        let a_path = root.join("A");
        let b_path = root.join("B");
        std::fs::write(&a_path, "REQUEST B").expect("A fixture should be written");
        std::fs::write(&b_path, "REQUEST A").expect("B fixture should be written");
        let a_canonical = std::fs::canonicalize(&a_path).expect("A path should resolve");

        let mut sources = SourceTexts::new();
        let a_id = sources.register_with_acquisition(
            "REQUEST B",
            "A",
            SourceAcquisition::FileSystem {
                canonical_path: a_canonical.clone(),
            },
        );
        let mut session =
            SourceProcessingSession::new(sources, a_id).expect("processing session should build");
        register_request_word_for_session(&mut session);
        let mut writer = RecordingWriter::default();
        let mut hook = |sources: &mut SourceTexts,
                        states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            acquire_filesystem_source_with_states(sources, states, request)
        };

        let error = session
            .run_with_hook(&mut writer, &mut hook, None)
            .expect_err("A -> B -> A should be a cycle");
        assert!(matches!(
            error,
            SourceProcessorError::AdditionalSourceAcquisition {
                specification,
                kind: AdditionalSourceAcquisitionError::Cycle,
                ..
            } if specification.as_ref() == "A"
        ));
        assert_eq!(session.sources().len(), 2, "cycle source must not register");
        assert_eq!(
            session.acquisition_states.states.get(&a_canonical),
            Some(&SourceIdentityState::Processing)
        );
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    #[cfg(unix)]
    fn filesystem_session_does_not_reprocess_completed_canonical_alias() {
        let root = std::path::PathBuf::from(".tmp")
            .join(format!("issue-1658-alias-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory should be created");
        let a_path = root.join("A");
        let b_path = root.join("B");
        let alias_path = root.join("C");
        std::fs::write(&a_path, "REQUEST B\nREQUEST C\nFOO").expect("A fixture should be written");
        std::fs::write(&b_path, "DEF FOO\nEND").expect("B fixture should be written");
        std::os::unix::fs::symlink("B", &alias_path).expect("alias should be created");
        let a_canonical = std::fs::canonicalize(&a_path).expect("A path should resolve");

        let mut sources = SourceTexts::new();
        let a_id = sources.register_with_acquisition(
            "REQUEST B\nREQUEST C\nFOO",
            "A",
            SourceAcquisition::FileSystem {
                canonical_path: a_canonical,
            },
        );
        let mut session =
            SourceProcessingSession::new(sources, a_id).expect("processing session should build");
        register_request_word_for_session(&mut session);
        let mut writer = RecordingWriter::default();
        let mut hook = |sources: &mut SourceTexts,
                        states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            acquire_filesystem_source_with_states(sources, states, request)
        };

        session
            .run_with_hook(&mut writer, &mut hook, None)
            .expect("completed canonical alias should be a no-op");
        assert_eq!(session.sources().len(), 2, "alias must not register twice");
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    fn filesystem_session_leaves_failed_source_uncompleted_and_cannot_resume() {
        let root = std::path::PathBuf::from(".tmp")
            .join(format!("issue-1658-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory should be created");
        let a_path = root.join("A");
        let b_path = root.join("B");
        std::fs::write(&a_path, "REQUEST B").expect("A fixture should be written");
        std::fs::write(&b_path, "UNKNOWN").expect("B fixture should be written");
        let a_canonical = std::fs::canonicalize(&a_path).expect("A path should resolve");
        let b_canonical = std::fs::canonicalize(&b_path).expect("B path should resolve");

        let mut sources = SourceTexts::new();
        let a_id = sources.register_with_acquisition(
            "REQUEST B",
            "A",
            SourceAcquisition::FileSystem {
                canonical_path: a_canonical,
            },
        );
        let mut session =
            SourceProcessingSession::new(sources, a_id).expect("processing session should build");
        register_request_word_for_session(&mut session);
        let mut writer = RecordingWriter::default();
        let mut hook = |sources: &mut SourceTexts,
                        states: &mut SourceAcquisitionStates,
                        request: AdditionalSourceRequest| {
            acquire_filesystem_source_with_states(sources, states, request)
        };

        session
            .run_with_hook(&mut writer, &mut hook, None)
            .expect_err("B compilation should fail");
        assert_eq!(
            session.acquisition_states.states.get(&b_canonical),
            Some(&SourceIdentityState::Processing)
        );
        let resume_error = session
            .run_with_hook(&mut writer, &mut hook, None)
            .expect_err("failed session must not be resumed");
        assert_eq!(resume_error, SourceProcessorError::ProcessingSessionFailed);
        std::fs::remove_dir_all(root).expect("fixture directory should be removed");
    }

    #[test]
    fn seeded_processing_session_continues_rnd_across_top_level_forms() {
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, "PUTDEC RND(10)\nPUTDEC RND(10)");
        let mut first_output = RecordingWriter::default();
        let first = success(execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut first_output,
            None,
            123,
        ));

        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, "PUTDEC RND(10)\nPUTDEC RND(10)");
        let mut second_output = RecordingWriter::default();
        let second = success(execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut second_output,
            None,
            123,
        ));

        let mut expected_random = RandomState::seeded(123);
        let expected_output = format!(
            "{}{}",
            expected_random
                .next_inclusive(10)
                .expect("positive bound should succeed"),
            expected_random
                .next_inclusive(10)
                .expect("positive bound should succeed")
        );
        assert_eq!(first_output.text(), second_output.text());
        assert_eq!(first_output.text(), expected_output);
        assert_eq!(first.data_stack(), second.data_stack());
    }

    #[test]
    fn guess_example_covers_ordering_branches_with_one_generated_answer() {
        let source = std::fs::read_to_string(example_path("guess.tbx"))
            .expect("guess example should be readable");
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, &source);
        let mut expected_random = RandomState::seeded(123);
        let answer = expected_random
            .next_inclusive(100)
            .expect("the sample uses a positive random bound");
        assert!((2..=99).contains(&answer));
        let mut input = TestInput::new([
            Ok(Some("not a number".to_owned())),
            Ok(Some((answer - 1).to_string())),
            Ok(Some((answer + 1).to_string())),
            Ok(Some(answer.to_string())),
        ]);
        let mut writer = RecordingWriter::default();

        let result = execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            Some(&mut input),
            123,
        );

        let result = success(result);
        assert_eq!(
            writer.text(),
            "Guess a number from 1 to 100: Please enter a number.\n\
Guess a number from 1 to 100: Too low.\n\
Guess a number from 1 to 100: Too high.\n\
Guess a number from 1 to 100: Correct!\n"
        );
        assert_eq!(result.data_stack(), []);
    }

    #[test]
    fn guess_example_keeps_answer_after_invalid_input() {
        let source = std::fs::read_to_string(example_path("guess.tbx"))
            .expect("guess example should be readable");
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, &source);
        let mut expected_random = RandomState::seeded(456);
        let answer = expected_random
            .next_inclusive(100)
            .expect("the sample uses a positive random bound");
        let mut input = TestInput::new([
            Ok(Some("not a number".to_owned())),
            Ok(Some(answer.to_string())),
        ]);
        let mut writer = RecordingWriter::default();

        let result = execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            Some(&mut input),
            456,
        );

        let result = success(result);
        assert_eq!(
            writer.text(),
            "Guess a number from 1 to 100: Please enter a number.\n\
Guess a number from 1 to 100: Correct!\n"
        );
        assert_eq!(result.data_stack(), []);
    }

    fn example_path(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("docs")
            .join("next")
            .join("examples")
            .join(name)
    }

    #[test]
    fn prime_example_leaves_the_data_stack_empty() {
        let source = std::fs::read_to_string(example_path("prime.tbx"))
            .expect("prime example should be readable");
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, &source);
        let mut writer = RecordingWriter::default();

        let result = execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            None,
            123,
        );

        let result = success(result);
        assert_eq!(result.data_stack(), []);
    }

    #[test]
    fn grades_example_leaves_the_data_stack_empty() {
        let source = std::fs::read_to_string(example_path("grades.tbx"))
            .expect("grades example should be readable");
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, &source);
        let mut writer = RecordingWriter::default();

        let result = execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            None,
            123,
        );

        let result = success(result);
        assert_eq!(result.data_stack(), []);
    }

    #[test]
    fn mandelbrot_example_leaves_the_data_stack_empty() {
        let source = std::fs::read_to_string(example_path("mandelbrot.tbx"))
            .expect("Mandelbrot example should be readable");
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, &source);
        let mut writer = RecordingWriter::default();

        let result = execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            None,
            123,
        );

        let result = success(result);
        assert_eq!(result.data_stack(), []);
    }

    #[test]
    fn squares_example_leaves_the_data_stack_empty() {
        let source = std::fs::read_to_string(example_path("squares.tbx"))
            .expect("squares example should be readable");
        let (sources, standard_library_id, source_id) =
            sources_with_standard_library(STDLIB_SOURCE, &source);
        let mut writer = RecordingWriter::default();

        let result = execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            None,
            123,
        );

        let result = success(result);
        assert_eq!(result.data_stack(), []);
    }
}
