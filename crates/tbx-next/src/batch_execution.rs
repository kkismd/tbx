use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::arithmetic_primitive::register_arithmetic_primitives;
use crate::binding::Bindings;
use crate::bootstrap::{
    register_builtin_source_words, PrimitiveBootstrapError, SourceWordBootstrapError,
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

        let globals = GlobalVariables::new();

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
mod tests;
