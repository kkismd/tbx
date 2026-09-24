use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::binding::Bindings;
use crate::block_code::BlockCodeBuilder;
use crate::expression::{
    parse_expression_with_locals, DefinitionLocalReferences, ExpressionCallErrorKind,
    ExpressionError, ExpressionLocalResolver, ExpressionStaging, ExpressionSyntaxErrorKind,
    ExpressionVariableErrorKind,
};
use crate::global_array::GlobalArrays;
use crate::global_variable::{GlobalVariableView, GlobalVariables};
use crate::instruction::{
    CodeLocation, CodeSpaceLookup, CodeSpaceLookupError, Instruction, InstructionAddress,
    InstructionView,
};
use crate::instruction_builder::{InstructionBuildError, InstructionBuildTarget};
#[cfg(test)]
use crate::lexer::Lexer;
use crate::lexer::{LexError, Token, TokenKind};
use crate::line_number::{LineNumberError, LocalLineNumber, LocalLineNumberTable};
use crate::operator::OperatorLookup;
use crate::primitive::PrimitiveLookup;
use crate::published_code::{
    NewWordPublicationError, PublishedCode, PublishedWordBuilder, WordBodyBuildError,
};
use crate::random::RandomState;
use crate::runtime_input::RuntimeInput;
use crate::runtime_output::RuntimeOutput;
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::source_mapping::{
    InstructionSourceMappingView, SourceMappedCode, SourceMappingLookup, SourceMappingLookupError,
};
use crate::source_word::{
    AdditionalSourceRequest, NativeSourceWordBindingAccess, NativeSourceWordContext,
    NativeSourceWordContextParts, NativeSourceWordHandler, NativeStructuredSourceWordContext,
    NativeStructuredSourceWordContextParts, OneShotSourceWordDispatch, RuntimeDefinitionPublisher,
    SourceBlockMarker, SourceBlockReader, SourceBlockStatement, SourceWordDispatch,
    SourceWordError, SourceWordId, SourceWordLookup, SourceWordLookupError, SourceWordRegistry,
    SourceWordSyntaxMarker, StructuredBodyCapabilities, StructuredSourceWordDispatch,
    StructuredSourceWordInstance,
};
use crate::source_word_evaluator::{
    evaluate_source_word, evaluate_source_word_with_state, UserDefinedSourceWordContext,
    UserDefinedSourceWordContextParts,
};
use crate::source_word_ir::SourceProcessingCapabilities;
use crate::static_quotation::{StaticQuotation, StaticQuotationBuildError};
use crate::structured_grammar::{GrammarAccept, MarkerIdentity, StructuredGrammar};
use crate::value::Value;
use crate::vm::{ExecutionView, RunOutcome, Vm, VmError};
use crate::word::{PublishedWords, WordId};
use crate::word_lookup::PublishedWordLookup;
use crate::word_resolution::{
    resolve_binding_name, resolve_word_name, ResolvedBinding, WordResolutionError,
};

mod segmentation;
mod structured_frame;

use segmentation::{LogicalStatementCursor, LogicalStatementView, SegmentedSource, Terminal};
use structured_frame::{
    BuildTargetHandle, SharedOwnerLocalBuildTarget, StructuredExitMetadata, StructuredSourceFrame,
};

#[cfg(test)]
use crate::source_word::{SourceBlockRead, SourceBlockTerminal};

#[derive(Debug)]
pub(crate) struct TemporaryExecutionUnit {
    code: SourceMappedCode,
    entry: CodeLocation,
}

#[derive(Debug)]
pub(crate) struct CompiledTopLevelForm {
    pub(crate) unit: TemporaryExecutionUnit,
    pub(crate) additional_source: Option<AdditionalSourceRequest>,
}

/// Owns the tokenized source while allowing the processing session to return
/// between complete top-level forms. It intentionally stores no `SourceView`:
/// registering another source must be possible between calls without keeping a
/// borrow into `SourceTexts` alive.
pub(crate) struct SourceFormCursor {
    source_id: SourceId,
    segmented: SegmentedSource,
    position: usize,
}

impl SourceFormCursor {
    pub(crate) fn new(
        view: SourceView<'_>,
        source_id: SourceId,
    ) -> Result<Self, SourceProcessorError> {
        Ok(Self {
            source_id,
            segmented: SegmentedSource::collect(view, source_id)?,
            position: 0,
        })
    }

    /// Compiles exactly one complete top-level form, or returns `None` when
    /// the source has been consumed. A form owns its temporary code unit; the
    /// caller can execute it and then release all source borrows before adding
    /// a nested source to the same processing session.
    pub(crate) fn compile_next_form(
        &mut self,
        view: SourceView<'_>,
        mut context: SourceCompileContext<'_>,
    ) -> Result<Option<CompiledTopLevelForm>, SourceProcessorError> {
        if self.position >= self.segmented.completed_statements().len() {
            return match self.segmented.terminal() {
                Terminal::Eof { .. } => Ok(None),
                Terminal::LexError(error) => Err(error.into()),
            };
        }

        let mut code = SourceMappedCode::new();
        let mut additional_source = None;
        let consumed = {
            let statements = self.segmented.completed_statements();
            let mut cursor = LogicalStatementCursor::new(
                view,
                self.source_id,
                &statements[self.position..],
                self.segmented.terminal(),
            );
            let mut structured_frames = Vec::new();
            let root_line_numbers = Rc::new(RefCell::new(LocalLineNumberTable::new()));
            let mut builder = BlockCodeBuilder::new(&mut code);

            let Some(mut statement) = cursor.next_completed_statement() else {
                return Ok(None);
            };
            loop {
                if dispatch_current_owner_marker(
                    view,
                    self.source_id,
                    &context,
                    statement,
                    &mut builder,
                    &mut structured_frames,
                )? {
                    // A marker can terminate the current structured form.
                } else {
                    let (target_handle, line_numbers, capabilities) =
                        current_processing_context(&structured_frames, &root_line_numbers);
                    let mut owner_target;
                    let statement_code = match &target_handle {
                        BuildTargetHandle::Parent => {
                            &mut builder as &mut dyn InstructionBuildTarget
                        }
                        BuildTargetHandle::OwnerLocal(target) => {
                            owner_target = SharedOwnerLocalBuildTarget {
                                target: target.clone(),
                            };
                            &mut owner_target as &mut dyn InstructionBuildTarget
                        }
                    };
                    additional_source = compile_statement(
                        statement.tokens(),
                        &mut context,
                        &mut StatementCompileState {
                            code: statement_code,
                            line_numbers,
                            capabilities,
                            target: target_handle,
                        },
                        &mut StatementTraversal {
                            view,
                            source_id: self.source_id,
                            cursor: &mut cursor,
                            structured_frames: &mut structured_frames,
                        },
                    )?;
                }

                if structured_frames.is_empty() {
                    break;
                }
                let Some(next) = cursor.next_completed_statement() else {
                    let span = match self.segmented.terminal() {
                        Terminal::Eof { span } => span,
                        Terminal::LexError(error) => return Err(error.into()),
                    };
                    return Err(SourceWordError::StructuredMissingTerminator { span }.into());
                };
                statement = next;
            }

            root_line_numbers
                .borrow_mut()
                .resolve(&mut builder)
                .map_err(|source| SourceProcessorError::from(line_number_compile_error(source)))?;
            let eof_span = match self.segmented.terminal() {
                Terminal::Eof { span } => span,
                Terminal::LexError(error) => return Err(error.into()),
            };
            InstructionBuildTarget::append_mapped(&mut builder, Instruction::Halt, eof_span)?;
            builder.finish().map_err(InstructionBuildError::from)?;
            cursor.position
        };

        self.position += consumed;
        let entry = code
            .instruction_view()
            .location(InstructionAddress::from_index(0));
        Ok(Some(CompiledTopLevelForm {
            unit: TemporaryExecutionUnit { code, entry },
            additional_source,
        }))
    }
}

pub(crate) struct SourceCompileContext<'a> {
    bindings: BindingAccess<'a>,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordAccess<'a>>,
    globals: Option<&'a mut GlobalVariables>,
    arrays: Option<&'a mut GlobalArrays>,
    runtime_definitions: Option<RuntimeDefinitionPublicationAccess<'a>>,
    additional_source_capability: bool,
    local_references: Option<&'a DefinitionLocalReferences>,
    // #1936: only a compiled runtime word body has a return frame to target.
    return_allowed: bool,
}

pub(crate) struct DefinitionBodyCompileContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
    local_references: Option<&'a DefinitionLocalReferences>,
}

pub(crate) struct QuotationBodyCompileContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
    local_references: Option<&'a DefinitionLocalReferences>,
}

pub(crate) struct DefinitionBodyStatements<'a> {
    statements: &'a [SourceBlockStatement<'a>],
    terminal: Terminal,
}

pub(crate) struct QuotationBodyStatements<'a> {
    statements: &'a [SourceBlockStatement<'a>],
    terminal: Terminal,
}

enum BindingAccess<'a> {
    Read(&'a Bindings),
    Write(&'a mut Bindings),
}

enum SourceWordAccess<'a> {
    Read(SourceWordLookup<'a>),
    Write(&'a SourceWordRegistry),
}

struct RuntimeDefinitionPublicationAccess<'a> {
    code: &'a mut PublishedCode,
    words: &'a mut PublishedWords,
}

pub(crate) struct SourceExecutionContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
    code_spaces: &'a [InstructionView<'a>],
    source_mappings: &'a [InstructionSourceMappingView<'a>],
    globals: Option<SourceGlobalAccess<'a>>,
    arrays: Option<crate::global_array::GlobalArrayViewMut<'a>>,
    words: PublishedWordLookup<'a>,
    primitives: PrimitiveLookup<'a>,
    output: Option<&'a mut dyn RuntimeOutput>,
    input: Option<&'a mut dyn RuntimeInput>,
    random: Option<&'a mut RandomState>,
}

#[derive(Debug)]
enum SourceGlobalAccess<'a> {
    Read(GlobalVariableView<'a>),
    Write(crate::global_variable::GlobalVariableViewMut<'a>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRunResult {
    outcome: RunOutcome,
    data_stack: Vec<Value>,
    instruction_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeError {
    vm: VmError,
    source_span: Result<Option<SourceSpan>, SourceMappingLookupError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceProcessorError {
    ProcessingSessionFailed,
    Source(SourceError),
    Lex(LexError),
    Compile(CompileError),
    CodeSpaceLookup(CodeSpaceLookupError),
    InstructionBuild(InstructionBuildError),
    SourceMappingLookup(SourceMappingLookupError),
    SourceWordContextUnavailable {
        id: SourceWordId,
    },
    SourceWordLookup(SourceWordLookupError),
    SourceWord(SourceWordError),
    AdditionalSourceAcquisition {
        span: SourceSpan,
        specification: Box<str>,
        kind: AdditionalSourceAcquisitionError,
    },
    Runtime(RuntimeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdditionalSourceAcquisitionError {
    RelativePathRequiresFileSource,
    Cycle,
    Canonicalize { path: PathBuf, message: Box<str> },
    Read { path: PathBuf, message: Box<str> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompileError {
    span: SourceSpan,
    kind: CompileErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompileErrorKind {
    UnsupportedToken { kind: TokenKind },
    BareExpression,
    BifSyntax { source: BifSyntaxErrorKind },
    IntegerLiteralOutOfRange,
    IntegerLiteralConversion,
    LineNumberLiteralOutOfRange,
    LineNumberLiteralConversion,
    LineNumber { source: Box<LineNumberError> },
    WordResolution { source: WordResolutionError },
    Expression { source: ExpressionSyntaxErrorKind },
    ExpressionVariable { source: ExpressionVariableErrorKind },
    ExpressionCall { source: ExpressionCallErrorKind },
    StructuredExitTargetUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BifSyntaxErrorKind {
    MissingCondition,
    MissingComma,
    MissingTarget,
    TrailingToken { kind: TokenKind },
}

type OptionalLineNumberPrefix = Option<(LocalLineNumber, SourceSpan)>;

struct StatementCompileState<'a> {
    code: &'a mut dyn InstructionBuildTarget,
    line_numbers: Rc<RefCell<LocalLineNumberTable>>,
    capabilities: StructuredBodyCapabilities,
    target: BuildTargetHandle,
}

struct StatementTraversal<'source, 'cursor, S> {
    view: SourceView<'source>,
    source_id: SourceId,
    cursor: &'cursor mut LogicalStatementCursor<'source, 'source, S>,
    structured_frames: &'cursor mut Vec<StructuredSourceFrame>,
}

enum StatementSourceWordDispatch {
    Native(NativeSourceWordHandler),
    UserDefined(crate::source_word_ir::SourceWordImplementation),
    Structured {
        implementation: StatementStructuredSourceWordDispatch,
        grammar: StructuredGrammar,
    },
}

enum StatementStructuredSourceWordDispatch {
    Native(crate::source_word::NativeStructuredSourceWordStartHandler),
    UserDefined(crate::source_word::UserDefinedStructuredSourceWordImplementation),
}

fn current_processing_context(
    structured_frames: &[StructuredSourceFrame],
    root_line_numbers: &Rc<RefCell<LocalLineNumberTable>>,
) -> (
    BuildTargetHandle,
    Rc<RefCell<LocalLineNumberTable>>,
    StructuredBodyCapabilities,
) {
    structured_frames.last().map_or(
        (
            BuildTargetHandle::Parent,
            root_line_numbers.clone(),
            StructuredBodyCapabilities::inherit(),
        ),
        |frame| {
            (
                frame.body_target.clone(),
                frame.current_line_numbers.clone(),
                frame.body_capabilities,
            )
        },
    )
}

fn dispatch_current_owner_marker<'source, S>(
    view: SourceView<'source>,
    source_id: SourceId,
    context: &SourceCompileContext<'_>,
    statement: &'source S,
    code: &mut dyn InstructionBuildTarget,
    structured_frames: &mut Vec<StructuredSourceFrame>,
) -> Result<bool, SourceProcessorError>
where
    S: LogicalStatementView,
{
    let Some(frame) = structured_frames.last_mut() else {
        return Ok(false);
    };
    let span = statement.span(view, source_id)?;
    let block_statement = SourceBlockStatement::new(statement.tokens(), span);
    let Some(marker) = classify_current_owner_marker(view, block_statement, &frame.syntax_markers)?
    else {
        return Ok(false);
    };

    let identity = MarkerIdentity::new(marker.name().clone());
    let accept =
        frame
            .progress
            .accept(&identity)
            .map_err(|source| SourceWordError::StructuredGrammar {
                span: marker.span(),
                source,
            })?;

    if matches!(accept, GrammarAccept::Intermediate { .. })
        && frame.owner.resolve_line_numbers_at_intermediate_marker()
    {
        frame.resolve_owner_line_numbers(code)?;
    }
    if matches!(accept, GrammarAccept::Terminator)
        && !frame.owner.resolve_line_numbers_at_terminator()
    {
        frame.resolve_owner_line_numbers(code)?;
    }

    let callback_target = match accept {
        GrammarAccept::Intermediate { .. } => frame.body_target.clone(),
        GrammarAccept::Terminator => frame.enclosing_target.clone(),
    };
    let callback_line_numbers = match accept {
        GrammarAccept::Intermediate { .. } => frame.current_line_numbers.clone(),
        GrammarAccept::Terminator => frame.enclosing_line_numbers.clone(),
    };
    let owner_local_targets = frame.owner_local_target_snapshots()?;
    let mut owner_target;
    let callback_code = match &callback_target {
        BuildTargetHandle::Parent => &mut *code,
        BuildTargetHandle::OwnerLocal(target) => {
            owner_target = SharedOwnerLocalBuildTarget {
                target: target.clone(),
            };
            &mut owner_target as &mut dyn InstructionBuildTarget
        }
    };
    let mut callback_line_numbers = callback_line_numbers.borrow_mut();
    let mut owner_context =
        NativeStructuredSourceWordContext::new(NativeStructuredSourceWordContextParts {
            view,
            source_id,
            bindings: context.bindings(),
            operators: context.operators(),
            local_references: context.local_references,
            code: callback_code,
            line_numbers: &mut callback_line_numbers,
            capabilities: SourceProcessingCapabilities::structured_runtime(),
            return_allowed: context.return_allowed,
            owner_local_targets,
        });
    match accept {
        GrammarAccept::Intermediate { .. } => {
            frame
                .owner
                .accept_marker(&mut owner_context, marker, accept)?;
            frame.apply_owner_context();
        }
        GrammarAccept::Terminator => {
            frame.owner.complete(&mut owner_context, marker)?;
            drop(owner_context);
            drop(callback_line_numbers);
            if frame.owner.resolve_line_numbers_at_terminator() {
                frame.resolve_owner_line_numbers(code)?;
            }
            let mut patch_target;
            let completion_code = match &callback_target {
                BuildTargetHandle::Parent => &mut *code,
                BuildTargetHandle::OwnerLocal(target) => {
                    patch_target = SharedOwnerLocalBuildTarget {
                        target: target.clone(),
                    };
                    &mut patch_target as &mut dyn InstructionBuildTarget
                }
            };
            frame.patch_pending_exit_branches(completion_code)?;
        }
    }

    if matches!(accept, GrammarAccept::Terminator) {
        structured_frames
            .pop()
            .expect("terminator handling requires current frame");
    }

    Ok(true)
}

fn classify_current_owner_marker<'source>(
    view: SourceView<'source>,
    statement: SourceBlockStatement<'source>,
    syntax_markers: &[SourceWordSyntaxMarker],
) -> Result<Option<SourceBlockMarker<'source>>, SourceProcessorError> {
    // #1544/#1545: only the innermost current owner's marker declarations are
    // considered before ordinary binding dispatch. Ancestor marker spelling is
    // intentionally invisible while a child owner is active.
    let Some(token) = statement.leading_name() else {
        return Ok(None);
    };
    let source_name = view.slice(token.span())?;
    let Ok(name) = crate::name::NormalizedName::new(source_name) else {
        return Ok(None);
    };

    Ok(syntax_markers
        .iter()
        .find(|marker| marker.name() == &name)
        .map(|marker| SourceBlockMarker::new(statement, token, name, marker.role())))
}

impl From<SourceError> for SourceProcessorError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<LexError> for SourceProcessorError {
    fn from(error: LexError) -> Self {
        Self::Lex(error)
    }
}

impl From<CompileError> for SourceProcessorError {
    fn from(error: CompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<CodeSpaceLookupError> for SourceProcessorError {
    fn from(error: CodeSpaceLookupError) -> Self {
        Self::CodeSpaceLookup(error)
    }
}

impl From<InstructionBuildError> for SourceProcessorError {
    fn from(error: InstructionBuildError) -> Self {
        Self::InstructionBuild(error)
    }
}

impl From<WordBodyBuildError> for SourceProcessorError {
    fn from(error: WordBodyBuildError) -> Self {
        Self::InstructionBuild(InstructionBuildError::WordBodyBuild { source: error })
    }
}

impl From<StaticQuotationBuildError> for SourceProcessorError {
    fn from(error: StaticQuotationBuildError) -> Self {
        match error {
            StaticQuotationBuildError::Build { source } => {
                Self::InstructionBuild(InstructionBuildError::BlockCodeBuild { source })
            }
        }
    }
}

impl From<SourceMappingLookupError> for SourceProcessorError {
    fn from(error: SourceMappingLookupError) -> Self {
        Self::SourceMappingLookup(error)
    }
}

impl From<SourceWordLookupError> for SourceProcessorError {
    fn from(error: SourceWordLookupError) -> Self {
        Self::SourceWordLookup(error)
    }
}

impl From<SourceWordError> for SourceProcessorError {
    fn from(error: SourceWordError) -> Self {
        Self::SourceWord(error)
    }
}

pub(crate) fn compile_source(
    view: SourceView<'_>,
    source_id: SourceId,
    context: SourceCompileContext<'_>,
) -> Result<TemporaryExecutionUnit, SourceProcessorError> {
    let segmented = SegmentedSource::collect(view, source_id)?;

    let mut code = SourceMappedCode::new();

    {
        let mut builder = BlockCodeBuilder::new(&mut code);
        compile_statements(
            view,
            source_id,
            segmented.completed_statements(),
            segmented.terminal(),
            context,
            &mut builder,
        )?;

        let eof_span = match segmented.terminal() {
            Terminal::Eof { span } => span,
            Terminal::LexError(error) => return Err(error.into()),
        };
        InstructionBuildTarget::append_mapped(&mut builder, Instruction::Halt, eof_span)?;
        builder.finish().map_err(InstructionBuildError::from)?;
    }

    let entry = code
        .instruction_view()
        .location(InstructionAddress::from_index(0));
    Ok(TemporaryExecutionUnit { code, entry })
}

pub(crate) fn compile_definition_body<'source>(
    view: SourceView<'source>,
    source_id: SourceId,
    body: DefinitionBodyStatements<'source>,
    context: DefinitionBodyCompileContext<'_>,
    builder: &mut PublishedWordBuilder<'_>,
) -> Result<(), SourceProcessorError> {
    let context = SourceCompileContext {
        bindings: BindingAccess::Read(context.bindings),
        operators: context.operators,
        source_words: context.source_words.map(SourceWordAccess::Read),
        globals: None,
        arrays: None,
        runtime_definitions: None,
        additional_source_capability: false,
        local_references: context.local_references,
        return_allowed: true,
    };

    compile_statements(
        view,
        source_id,
        body.statements,
        body.terminal,
        context,
        builder,
    )
}

pub(crate) fn compile_quotation_body<'source>(
    view: SourceView<'source>,
    source_id: SourceId,
    body: QuotationBodyStatements<'source>,
    context: QuotationBodyCompileContext<'_>,
) -> Result<StaticQuotation, SourceProcessorError> {
    let context = SourceCompileContext {
        bindings: BindingAccess::Read(context.bindings),
        operators: context.operators,
        source_words: context.source_words.map(SourceWordAccess::Read),
        // #1516/#1500: quotation bodies reuse statement lowering but have no
        // capability to publish bindings, globals, or runtime definitions.
        globals: None,
        arrays: None,
        runtime_definitions: None,
        additional_source_capability: false,
        local_references: context.local_references,
        return_allowed: false,
    };

    StaticQuotation::try_build(|builder| {
        compile_statements(
            view,
            source_id,
            body.statements,
            body.terminal,
            context,
            builder,
        )
    })
}

fn compile_statements<'source, S>(
    view: SourceView<'source>,
    source_id: SourceId,
    statements: &'source [S],
    terminal: Terminal,
    mut context: SourceCompileContext<'_>,
    code: &mut dyn InstructionBuildTarget,
) -> Result<(), SourceProcessorError>
where
    S: LogicalStatementView,
{
    let root_line_numbers = Rc::new(RefCell::new(LocalLineNumberTable::new()));
    let mut cursor = LogicalStatementCursor::new(view, source_id, statements, terminal);
    let mut structured_frames = Vec::new();

    while let Some(statement) = cursor.next_completed_statement() {
        if dispatch_current_owner_marker(
            view,
            source_id,
            &context,
            statement,
            code,
            &mut structured_frames,
        )? {
            continue;
        }

        let (target_handle, line_numbers, capabilities) =
            current_processing_context(&structured_frames, &root_line_numbers);
        let mut owner_target;
        let statement_code = match &target_handle {
            BuildTargetHandle::Parent => &mut *code,
            BuildTargetHandle::OwnerLocal(target) => {
                owner_target = SharedOwnerLocalBuildTarget {
                    target: target.clone(),
                };
                &mut owner_target as &mut dyn InstructionBuildTarget
            }
        };
        compile_statement(
            statement.tokens(),
            &mut context,
            &mut StatementCompileState {
                code: statement_code,
                line_numbers,
                capabilities,
                target: target_handle,
            },
            &mut StatementTraversal {
                view,
                source_id,
                cursor: &mut cursor,
                structured_frames: &mut structured_frames,
            },
        )?;
    }

    match terminal {
        Terminal::Eof { span } if !structured_frames.is_empty() => {
            return Err(SourceWordError::StructuredMissingTerminator { span }.into());
        }
        Terminal::LexError(error) => return Err(error.into()),
        Terminal::Eof { .. } => {}
    }

    let result = root_line_numbers
        .borrow_mut()
        .resolve(code)
        .map_err(|source| SourceProcessorError::from(line_number_compile_error(source)));
    result
}

fn compile_statement<'source, S>(
    statement: &'source [Token],
    context: &mut SourceCompileContext<'_>,
    state: &mut StatementCompileState<'_>,
    traversal: &mut StatementTraversal<'source, '_, S>,
) -> Result<Option<AdditionalSourceRequest>, SourceProcessorError>
where
    S: LogicalStatementView,
{
    if statement.is_empty() {
        return Ok(None);
    }

    let (line_number, body) = split_statement_line_number(traversal.view, statement)?;
    let start = state.code.current_address();
    let local_line_number_prefix = line_number.map(|(_, span)| span);
    let additional_source =
        compile_statement_body(body, context, local_line_number_prefix, state, traversal)?;

    if let Some((line_number, span)) = line_number {
        state
            .line_numbers
            .borrow_mut()
            .define(state.code, line_number, start, span)
            .map_err(|source| SourceProcessorError::from(line_number_compile_error(source)))?;
    }
    Ok(additional_source)
}

fn split_statement_line_number<'a>(
    view: SourceView<'_>,
    statement: &'a [Token],
) -> Result<(OptionalLineNumberPrefix, &'a [Token]), SourceProcessorError> {
    let Some((&first, rest)) = statement.split_first() else {
        return Ok((None, statement));
    };

    if first.kind() != TokenKind::IntegerLiteral {
        return Ok((None, statement));
    }

    // #1535: line-number definition recognition is local to the complete
    // logical statement. Do not consult later branch references or binding
    // resolution before deciding whether the leading integer is a prefix.
    let Some(next) = rest.first().copied() else {
        return Err(CompileError {
            span: first.span(),
            kind: CompileErrorKind::BareExpression,
        }
        .into());
    };
    if next.kind() != TokenKind::Name {
        return Err(CompileError {
            span: first.span(),
            kind: CompileErrorKind::BareExpression,
        }
        .into());
    }

    let line_number = compile_line_number_literal(view, first)?;
    Ok((Some((line_number, first.span())), rest))
}

fn compile_statement_body<'source, S>(
    tokens: &'source [Token],
    context: &mut SourceCompileContext<'_>,
    local_line_number_prefix: Option<SourceSpan>,
    state: &mut StatementCompileState<'_>,
    traversal: &mut StatementTraversal<'source, '_, S>,
) -> Result<Option<AdditionalSourceRequest>, SourceProcessorError>
where
    S: LogicalStatementView,
{
    let Some((&first, _)) = tokens.split_first() else {
        return Ok(None);
    };

    if is_bif_keyword(traversal.view, first)? {
        compile_bif(
            traversal.view,
            traversal.source_id,
            tokens,
            context,
            state.code,
            &mut state.line_numbers.borrow_mut(),
        )?;
        return Ok(None);
    }

    if let Some(additional_source) = compile_statement_leading_source_word(
        tokens,
        context,
        local_line_number_prefix,
        state,
        traversal,
    )? {
        return Ok(additional_source);
    }

    if compile_statement_leading_runtime_word(
        traversal.view,
        traversal.source_id,
        tokens,
        context,
        state,
    )? {
        return Ok(None);
    }

    if contains_expression_syntax(tokens) {
        return Err(CompileError {
            span: first.span(),
            kind: CompileErrorKind::BareExpression,
        }
        .into());
    }

    if first.kind() == TokenKind::Name {
        compile_word_reference(traversal.view, first, context).map(|_| None)
    } else {
        Err(CompileError {
            span: first.span(),
            kind: CompileErrorKind::BareExpression,
        }
        .into())
    }
}

fn compile_statement_leading_runtime_word(
    view: SourceView<'_>,
    source_id: SourceId,
    tokens: &[Token],
    context: &SourceCompileContext<'_>,
    state: &mut StatementCompileState<'_>,
) -> Result<bool, SourceProcessorError> {
    let Some(head) = tokens.first().copied() else {
        return Ok(false);
    };
    if head.kind() != TokenKind::Name {
        return Ok(false);
    }

    let source_name = view.slice(head.span())?;
    let binding = match resolve_binding_name(context.bindings(), source_name) {
        Ok(binding) => binding,
        Err(WordResolutionError::InvalidWordName | WordResolutionError::UndefinedName) => {
            return Ok(false);
        }
        Err(WordResolutionError::TargetIsNotWord) => unreachable!(),
    };
    let ResolvedBinding::RuntimeWord(word) = binding else {
        return Ok(false);
    };

    let trailing = &tokens[1..];
    let has_expression = trailing
        .iter()
        .any(|token| !matches!(token.kind(), TokenKind::LineBoundary | TokenKind::Eof));
    if !has_expression {
        state
            .code
            .append_mapped(Instruction::Call(word), head.span())?;
        return Ok(true);
    }

    // ADR #1631: a runtime-word statement owns its trailing ordinary
    // expression. Stage the expression and its call together so a failure
    // cannot commit a partial statement.
    let Some(operators) = context.operators() else {
        return Err(CompileError {
            span: head.span(),
            kind: CompileErrorKind::UnsupportedToken { kind: head.kind() },
        }
        .into());
    };
    let mut staging = parse_expression_staging(
        view,
        source_id,
        trailing,
        context.bindings(),
        operators,
        context.local_references,
    )?;
    staging.append_mapped_instruction(Instruction::Call(word), head.span());
    staging
        .commit_to(state.code)
        .map_err(SourceProcessorError::from_expression_error)?;
    Ok(true)
}

fn compile_statement_leading_source_word<'source, S>(
    tokens: &'source [Token],
    context: &mut SourceCompileContext<'_>,
    local_line_number_prefix: Option<SourceSpan>,
    state: &mut StatementCompileState<'_>,
    traversal: &mut StatementTraversal<'source, '_, S>,
) -> Result<Option<Option<AdditionalSourceRequest>>, SourceProcessorError>
where
    S: LogicalStatementView,
{
    let Some(first) = tokens.first().copied() else {
        return Ok(None);
    };
    if first.kind() != TokenKind::Name {
        return Ok(None);
    }

    let source_name = traversal.view.slice(first.span())?;
    let binding = match resolve_binding_name(context.bindings(), source_name) {
        Ok(binding) => binding,
        Err(WordResolutionError::InvalidWordName | WordResolutionError::UndefinedName) => {
            return Ok(None);
        }
        Err(WordResolutionError::TargetIsNotWord) => {
            unreachable!("binding-kind resolution does not classify published bindings as non-word")
        }
    };

    let ResolvedBinding::SourceWord(id) = binding else {
        return Ok(None);
    };
    let Some(source_word_access) = &context.source_words else {
        return Err(SourceProcessorError::SourceWordContextUnavailable { id });
    };
    let (dispatch, syntax_markers, runtime_definition_source_words) = {
        let source_words = match source_word_access {
            SourceWordAccess::Read(lookup) => *lookup,
            SourceWordAccess::Write(registry) => registry.lookup(),
        };
        let dispatch = match source_words.lookup_dispatch(id)? {
            SourceWordDispatch::OneShot(OneShotSourceWordDispatch::Native(handler)) => {
                StatementSourceWordDispatch::Native(handler)
            }
            SourceWordDispatch::OneShot(OneShotSourceWordDispatch::UserDefined(implementation)) => {
                StatementSourceWordDispatch::UserDefined(implementation.clone())
            }
            SourceWordDispatch::Structured {
                implementation,
                grammar,
            } => StatementSourceWordDispatch::Structured {
                implementation: match implementation {
                    StructuredSourceWordDispatch::Native(start) => {
                        StatementStructuredSourceWordDispatch::Native(start)
                    }
                    StructuredSourceWordDispatch::UserDefined(implementation) => {
                        StatementStructuredSourceWordDispatch::UserDefined(implementation.clone())
                    }
                },
                grammar: grammar.clone(),
            },
        };
        let syntax_markers = source_words.syntax_markers(id)?;
        let runtime_definition_source_words = Some(match source_word_access {
            SourceWordAccess::Read(lookup) => *lookup,
            SourceWordAccess::Write(registry) => registry.lookup(),
        });
        (dispatch, syntax_markers, runtime_definition_source_words)
    };
    let operators = context.operators();
    let globals = context
        .globals
        .as_deref_mut()
        .filter(|_| state.capabilities.allows_publication());
    let arrays = context
        .arrays
        .as_deref_mut()
        .filter(|_| state.capabilities.allows_publication());
    let mut runtime_publisher = context
        .runtime_definitions
        .as_mut()
        .filter(|_| state.capabilities.allows_publication())
        .filter(|_| runtime_definition_source_words.is_some())
        .map(|publication| RuntimeDefinitionPublisherAdapter {
            view: traversal.view,
            source_id: traversal.source_id,
            operators,
            source_words: runtime_definition_source_words
                .expect("source word lookup should be available for runtime definition"),
            code: &mut *publication.code,
            words: &mut *publication.words,
        });
    let runtime_definitions = runtime_publisher
        .as_mut()
        .map(|publisher| publisher as &mut dyn RuntimeDefinitionPublisher<'source>);
    let binding_access = if state.capabilities.allows_publication() {
        match &mut context.bindings {
            BindingAccess::Read(bindings) => NativeSourceWordBindingAccess::Read(bindings),
            BindingAccess::Write(bindings) => NativeSourceWordBindingAccess::Write(bindings),
        }
    } else {
        match &mut context.bindings {
            BindingAccess::Read(bindings) => NativeSourceWordBindingAccess::Read(bindings),
            BindingAccess::Write(bindings) => NativeSourceWordBindingAccess::Read(bindings),
        }
    };
    match dispatch {
        StatementSourceWordDispatch::Native(handler) => {
            let source_word_publication = if state.capabilities.allows_publication() {
                match &mut context.source_words {
                    Some(SourceWordAccess::Write(registry)) => Some(*registry),
                    Some(SourceWordAccess::Read(_)) | None => None,
                }
            } else {
                None
            };
            let mut source_word_context =
                NativeSourceWordContext::new(NativeSourceWordContextParts {
                    view: traversal.view,
                    source_id: traversal.source_id,
                    tokens,
                    block_reader: Some(SourceBlockReader::new(
                        traversal.view,
                        traversal.cursor,
                        &syntax_markers,
                    )),
                    bindings: binding_access,
                    operators,
                    local_references: context.local_references,
                    code: state.code,
                    local_line_number_prefix,
                    globals,
                    arrays,
                    runtime_definitions,
                    source_word_publication,
                    additional_source_capability: context.additional_source_capability,
                });
            handler(&mut source_word_context)?;
            return Ok(Some(source_word_context.take_additional_source_request()));
        }
        StatementSourceWordDispatch::UserDefined(implementation) => {
            let exit_requested = {
                let mut line_numbers = state.line_numbers.borrow_mut();
                let mut source_word_context =
                    UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                        view: traversal.view,
                        source_id: traversal.source_id,
                        tokens,
                        bindings: context.bindings(),
                        operators,
                        local_references: context.local_references,
                        code: state.code,
                        line_numbers: &mut line_numbers,
                        capabilities: SourceProcessingCapabilities::statement_runtime(),
                        return_allowed: context.return_allowed,
                    });
                evaluate_source_word(&implementation, &mut source_word_context)
                    .map_err(|source| SourceWordError::UserDefinedEvaluation { source })?;
                source_word_context.take_exit_request()
            };
            if exit_requested {
                emit_structured_exit(first.span(), state, traversal.structured_frames)?;
            }
        }
        StatementSourceWordDispatch::Structured {
            implementation,
            grammar,
        } => {
            let exit_metadata = match &implementation {
                StatementStructuredSourceWordDispatch::Native(_) => StructuredExitMetadata {
                    exit_target: false,
                    control_value_ownership: 0,
                },
                StatementStructuredSourceWordDispatch::UserDefined(implementation) => {
                    StructuredExitMetadata {
                        exit_target: implementation.exit_target(),
                        control_value_ownership: implementation.control_value_ownership(),
                    }
                }
            };
            let instance = match implementation {
                StatementStructuredSourceWordDispatch::Native(start) => {
                    let mut source_word_context =
                        NativeSourceWordContext::new(NativeSourceWordContextParts {
                            view: traversal.view,
                            source_id: traversal.source_id,
                            tokens,
                            block_reader: None,
                            bindings: binding_access,
                            operators,
                            local_references: context.local_references,
                            code: state.code,
                            local_line_number_prefix,
                            globals,
                            arrays: None,
                            runtime_definitions,
                            source_word_publication: None,
                            additional_source_capability: false,
                        });
                    start(&mut source_word_context)?
                }
                StatementStructuredSourceWordDispatch::UserDefined(implementation) => {
                    let mut evaluation_state =
                        crate::source_word_evaluator::SourceWordEvaluationState::new();
                    {
                        let mut line_numbers = state.line_numbers.borrow_mut();
                        let mut source_word_context =
                            UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                                view: traversal.view,
                                source_id: traversal.source_id,
                                tokens,
                                bindings: context.bindings(),
                                operators,
                                local_references: context.local_references,
                                code: state.code,
                                line_numbers: &mut line_numbers,
                                capabilities: SourceProcessingCapabilities::structured_runtime(),
                                return_allowed: context.return_allowed,
                            });
                        evaluate_source_word_with_state(
                            implementation.start(),
                            &mut source_word_context,
                            &mut evaluation_state,
                        )
                        .map_err(|source| SourceWordError::UserDefinedEvaluation { source })?;
                    }
                    StructuredSourceWordInstance::new(Box::new(
                        crate::source_word::UserDefinedStructuredSourceWordOwner::new(
                            implementation,
                            evaluation_state,
                        ),
                    ))
                }
            };
            let mut frame = StructuredSourceFrame::new(
                syntax_markers,
                grammar.start(),
                instance.into_owner(),
                state.target.clone(),
                state.line_numbers.clone(),
                state.capabilities,
                exit_metadata,
            );
            frame.apply_owner_context();
            traversal.structured_frames.push(frame);
        }
    }
    Ok(Some(None))
}

fn emit_structured_exit(
    span: SourceSpan,
    state: &mut StatementCompileState<'_>,
    frames: &mut [StructuredSourceFrame],
) -> Result<(), SourceProcessorError> {
    let Some(target_index) = frames.iter().rposition(|frame| frame.exit_target) else {
        return Err(CompileError {
            span,
            kind: CompileErrorKind::StructuredExitTargetUnavailable,
        }
        .into());
    };
    let drops = frames[target_index..]
        .iter()
        .map(|frame| frame.control_value_ownership)
        .sum::<usize>();
    for _ in 0..drops {
        state
            .code
            .append_mapped(Instruction::DropControlValue, span)?;
    }
    let branch = state.code.append_mapped_jump_placeholder(span)?;
    frames[target_index].pending_exit_branches.push(branch);
    Ok(())
}

fn compile_bif(
    view: SourceView<'_>,
    source_id: SourceId,
    tokens: &[Token],
    context: &SourceCompileContext<'_>,
    code: &mut dyn InstructionBuildTarget,
    line_numbers: &mut LocalLineNumberTable,
) -> Result<(), SourceProcessorError> {
    let bif = tokens
        .first()
        .copied()
        .expect("BIF compiler requires the keyword token");
    let Some(operators) = context.operators() else {
        return Err(CompileError {
            span: bif.span(),
            kind: CompileErrorKind::UnsupportedToken { kind: bif.kind() },
        }
        .into());
    };
    let Some(comma_index) = find_top_level_comma(&tokens[1..]).map(|index| index + 1) else {
        return Err(bif_syntax(bif.span(), BifSyntaxErrorKind::MissingComma).into());
    };
    if comma_index == 1 {
        return Err(bif_syntax(bif.span(), BifSyntaxErrorKind::MissingCondition).into());
    }

    compile_expression_tokens(
        view,
        source_id,
        &tokens[1..comma_index],
        context.bindings(),
        operators,
        context.local_references,
        code,
    )?;

    let target_tokens = &tokens[comma_index + 1..];
    let Some((&target, rest)) = target_tokens.split_first() else {
        return Err(bif_syntax(
            tokens[comma_index].span(),
            BifSyntaxErrorKind::MissingTarget,
        )
        .into());
    };
    if target.kind() != TokenKind::IntegerLiteral {
        return Err(bif_syntax(target.span(), BifSyntaxErrorKind::MissingTarget).into());
    }
    if let Some(trailing) = rest.first().copied() {
        return Err(bif_syntax(
            trailing.span(),
            BifSyntaxErrorKind::TrailingToken {
                kind: trailing.kind(),
            },
        )
        .into());
    }

    let line_number = compile_line_number_literal(view, target)?;
    let branch = code.append_mapped_jump_if_zero_placeholder(bif.span())?;
    line_numbers.add_patch(line_number, branch, target.span());
    Ok(())
}

fn compile_expression_tokens(
    view: SourceView<'_>,
    source_id: SourceId,
    tokens: &[Token],
    bindings: &Bindings,
    operators: OperatorLookup,
    local_references: Option<&DefinitionLocalReferences>,
    code: &mut dyn InstructionBuildTarget,
) -> Result<(), SourceProcessorError> {
    parse_expression_staging(
        view,
        source_id,
        tokens,
        bindings,
        operators,
        local_references,
    )?
    .commit_to(code)
    .map_err(SourceProcessorError::from_expression_error)
}

fn parse_expression_staging(
    view: SourceView<'_>,
    source_id: SourceId,
    tokens: &[Token],
    bindings: &Bindings,
    operators: OperatorLookup,
    local_references: Option<&DefinitionLocalReferences>,
) -> Result<ExpressionStaging, SourceProcessorError> {
    let mut expression_tokens = tokens
        .iter()
        .copied()
        .filter(|token| token.kind() != TokenKind::LineBoundary)
        .collect::<Vec<_>>();
    let end = expression_tokens
        .last()
        .map_or(0, |token| token.span().end());
    expression_tokens.push(Token::new(TokenKind::Eof, view.span(source_id, end, end)?));

    let resolver = |source_name: &str| resolve_variable_name(bindings, source_name);
    let array_resolver =
        |source_name: &str| crate::source_word::resolve_array_name(bindings, source_name);
    let runtime_word_resolver =
        |source_name: &str| resolve_runtime_word_name(bindings, source_name);
    let local_resolver =
        local_references.map(|references| references as &dyn ExpressionLocalResolver);
    parse_expression_with_locals(
        view,
        &expression_tokens,
        operators,
        &resolver,
        &runtime_word_resolver,
        &array_resolver,
        local_resolver,
    )
    .map_err(SourceProcessorError::from_expression_error)
}

pub(crate) fn run_source(
    view: SourceView<'_>,
    source_id: SourceId,
    context: SourceExecutionContext<'_>,
) -> Result<SourceRunResult, SourceProcessorError> {
    let unit = compile_source(view, source_id, context.compile_context())?;
    run_unit(&unit, context)
}

pub(crate) fn run_unit(
    unit: &TemporaryExecutionUnit,
    context: SourceExecutionContext<'_>,
) -> Result<SourceRunResult, SourceProcessorError> {
    run_unit_with_data_stack(unit, context, &[])
}

pub(crate) fn run_unit_with_data_stack(
    unit: &TemporaryExecutionUnit,
    context: SourceExecutionContext<'_>,
    initial_data_stack: &[Value],
) -> Result<SourceRunResult, SourceProcessorError> {
    let mut code_spaces = Vec::with_capacity(context.code_spaces.len() + 1);
    code_spaces.push(unit.code.instruction_view());
    code_spaces.extend_from_slice(context.code_spaces);
    let mut execution = ExecutionView::with_code_spaces(
        CodeSpaceLookup::new(&code_spaces)?,
        context.words,
        context.primitives,
    );
    if let Some(globals) = context.globals {
        execution = match globals {
            SourceGlobalAccess::Read(globals) => execution.with_global_reader(globals),
            SourceGlobalAccess::Write(globals) => execution.with_globals(globals),
        };
    }
    if let Some(arrays) = context.arrays {
        execution = execution.with_arrays(arrays);
    }
    if let Some(output) = context.output {
        execution = execution.with_output(output);
    }
    if let Some(input) = context.input {
        execution = execution.with_input(input);
    }
    if let Some(random) = context.random {
        execution = execution.with_random(random);
    }
    let mut vm = Vm::new_at_location_in(&mut execution, unit.entry)
        .map_err(|error| map_runtime_error(error, unit, context.source_mappings))?;
    for value in initial_data_stack {
        vm.push_data(*value);
    }
    let outcome = vm
        .run(&mut execution)
        .map_err(|error| map_runtime_error(error, unit, context.source_mappings))?;
    let data_stack = drain_data_stack(&mut vm);

    Ok(SourceRunResult {
        outcome,
        data_stack,
        instruction_count: unit.code.len(),
    })
}

fn map_runtime_error(
    error: VmError,
    unit: &TemporaryExecutionUnit,
    source_mappings: &[InstructionSourceMappingView<'_>],
) -> SourceProcessorError {
    let mut mapping_views = Vec::with_capacity(source_mappings.len() + 1);
    mapping_views.push(unit.source_mapping());
    mapping_views.extend_from_slice(source_mappings);
    let source_span = SourceMappingLookup::new(&mapping_views)
        .and_then(|lookup| lookup.source_span(error.location()));

    SourceProcessorError::Runtime(RuntimeError {
        vm: error,
        source_span,
    })
}

fn resolve_variable_name(
    bindings: &Bindings,
    source_name: &str,
) -> Result<crate::global_variable::GlobalVarId, ExpressionVariableErrorKind> {
    match resolve_binding_name(bindings, source_name) {
        Ok(ResolvedBinding::Variable(id)) => Ok(id),
        Ok(
            ResolvedBinding::RuntimeWord(_)
            | ResolvedBinding::SourceWord(_)
            | ResolvedBinding::Array(_),
        ) => Err(ExpressionVariableErrorKind::TargetIsNotVariable),
        Err(WordResolutionError::InvalidWordName) => Err(ExpressionVariableErrorKind::InvalidName),
        Err(WordResolutionError::UndefinedName) => Err(ExpressionVariableErrorKind::UndefinedName),
        Err(WordResolutionError::TargetIsNotWord) => {
            unreachable!("binding lookup does not require a runtime word target")
        }
    }
}

fn resolve_runtime_word_name(
    bindings: &Bindings,
    source_name: &str,
) -> Result<crate::word::WordId, ExpressionCallErrorKind> {
    match resolve_binding_name(bindings, source_name) {
        Ok(ResolvedBinding::RuntimeWord(id)) => Ok(id),
        Ok(
            ResolvedBinding::Variable(_)
            | ResolvedBinding::SourceWord(_)
            | ResolvedBinding::Array(_),
        ) => Err(ExpressionCallErrorKind::TargetIsNotRuntimeWord),
        Err(WordResolutionError::InvalidWordName) => Err(ExpressionCallErrorKind::InvalidName),
        Err(WordResolutionError::UndefinedName) => Err(ExpressionCallErrorKind::UndefinedName),
        Err(WordResolutionError::TargetIsNotWord) => {
            unreachable!("binding lookup does not require a runtime word target")
        }
    }
}

fn compile_word_reference(
    view: SourceView<'_>,
    token: Token,
    context: &SourceCompileContext<'_>,
) -> Result<crate::word::WordId, SourceProcessorError> {
    let source_name = view.slice(token.span())?;
    resolve_word_name(context.bindings(), source_name)
        .map_err(|source| CompileError {
            span: token.span(),
            kind: CompileErrorKind::WordResolution { source },
        })
        .map_err(SourceProcessorError::Compile)
}

fn compile_integer_literal(
    view: SourceView<'_>,
    token: Token,
) -> Result<i16, SourceProcessorError> {
    let source = view.slice(token.span())?;
    parse_unsigned_i16(source, token.span()).map_err(SourceProcessorError::Compile)
}

fn compile_line_number_literal(
    view: SourceView<'_>,
    token: Token,
) -> Result<LocalLineNumber, SourceProcessorError> {
    let source = view.slice(token.span())?;
    parse_local_line_number(source, token.span()).map_err(SourceProcessorError::Compile)
}

fn parse_local_line_number(
    source: &str,
    span: SourceSpan,
) -> Result<LocalLineNumber, CompileError> {
    let mut value: u64 = 0;
    let mut saw_digit = false;

    for byte in source.bytes() {
        let Some(digit) = byte.checked_sub(b'0').filter(|digit| *digit <= 9) else {
            return Err(CompileError {
                span,
                kind: CompileErrorKind::LineNumberLiteralConversion,
            });
        };

        saw_digit = true;
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(digit)))
            .ok_or(CompileError {
                span,
                kind: CompileErrorKind::LineNumberLiteralOutOfRange,
            })?;
    }

    if !saw_digit {
        return Err(CompileError {
            span,
            kind: CompileErrorKind::LineNumberLiteralConversion,
        });
    }

    Ok(LocalLineNumber::new(value))
}

fn parse_unsigned_i16(source: &str, span: SourceSpan) -> Result<i16, CompileError> {
    let mut value: i32 = 0;
    let mut saw_digit = false;

    for byte in source.bytes() {
        let Some(digit) = byte.checked_sub(b'0').filter(|digit| *digit <= 9) else {
            return Err(CompileError {
                span,
                kind: CompileErrorKind::IntegerLiteralConversion,
            });
        };

        saw_digit = true;
        value = value * 10 + i32::from(digit);
        if value > i32::from(i16::MAX) {
            return Err(CompileError {
                span,
                kind: CompileErrorKind::IntegerLiteralOutOfRange,
            });
        }
    }

    if !saw_digit {
        return Err(CompileError {
            span,
            kind: CompileErrorKind::IntegerLiteralConversion,
        });
    }

    i16::try_from(value).map_err(|_| CompileError {
        span,
        kind: CompileErrorKind::IntegerLiteralOutOfRange,
    })
}

fn contains_expression_syntax(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .any(|token| is_expression_syntax_token(token.kind()))
}

const fn is_expression_syntax_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::Equal
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual
            | TokenKind::Comma
    )
}

fn is_bif_keyword(view: SourceView<'_>, token: Token) -> Result<bool, SourceProcessorError> {
    if token.kind() != TokenKind::Name {
        return Ok(false);
    }

    Ok(view.slice(token.span())?.eq_ignore_ascii_case("BIF"))
}

fn find_top_level_comma(tokens: &[Token]) -> Option<usize> {
    let mut depth = 0usize;

    for (index, token) in tokens.iter().copied().enumerate() {
        match token.kind() {
            TokenKind::LParen => depth = depth.saturating_add(1),
            TokenKind::RParen => depth = depth.saturating_sub(1),
            TokenKind::Comma if depth == 0 => return Some(index),
            _ => {}
        }
    }

    None
}

fn bif_syntax(span: SourceSpan, source: BifSyntaxErrorKind) -> CompileError {
    CompileError {
        span,
        kind: CompileErrorKind::BifSyntax { source },
    }
}

fn line_number_compile_error(source: LineNumberError) -> CompileError {
    CompileError {
        span: source.primary_span(),
        kind: CompileErrorKind::LineNumber {
            source: Box::new(source),
        },
    }
}

fn drain_data_stack(vm: &mut Vm) -> Vec<Value> {
    let mut values = Vec::with_capacity(vm.data_stack_depth());

    while let Ok(value) = vm.pop_data() {
        values.push(value);
    }

    values.reverse();
    values
}

impl TemporaryExecutionUnit {
    pub(crate) fn entry(&self) -> InstructionAddress {
        self.entry.address()
    }

    pub(crate) fn entry_location(&self) -> CodeLocation {
        self.entry
    }

    pub(crate) fn instructions(&self) -> crate::instruction::InstructionView<'_> {
        self.code.instruction_view()
    }

    pub(crate) fn source_mapping(&self) -> InstructionSourceMappingView<'_> {
        self.code.source_mapping()
    }

    pub(crate) fn len(&self) -> usize {
        self.code.len()
    }

    pub(crate) fn source_span(
        &self,
        location: CodeLocation,
    ) -> Result<Option<SourceSpan>, SourceMappingLookupError> {
        self.code.source_mapping().source_span(location)
    }
}

impl<'a> SourceCompileContext<'a> {
    pub(crate) const fn new(bindings: &'a Bindings) -> Self {
        Self {
            bindings: BindingAccess::Read(bindings),
            operators: None,
            source_words: None,
            globals: None,
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings: BindingAccess::Read(bindings),
            operators: Some(operators),
            source_words: None,
            globals: None,
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) const fn with_source_words(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
    ) -> Self {
        Self {
            bindings: BindingAccess::Read(bindings),
            operators: None,
            source_words: Some(SourceWordAccess::Read(source_words)),
            globals: None,
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) const fn with_source_words_and_operators(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
    ) -> Self {
        Self {
            bindings: BindingAccess::Read(bindings),
            operators: Some(operators),
            source_words: Some(SourceWordAccess::Read(source_words)),
            globals: None,
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) fn with_source_word_publication(
        bindings: &'a mut Bindings,
        source_words: SourceWordLookup<'a>,
        globals: &'a mut GlobalVariables,
    ) -> Self {
        Self {
            bindings: BindingAccess::Write(bindings),
            operators: None,
            source_words: Some(SourceWordAccess::Read(source_words)),
            globals: Some(globals),
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) fn with_source_word_publication_and_operators(
        bindings: &'a mut Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
        globals: &'a mut GlobalVariables,
    ) -> Self {
        Self {
            bindings: BindingAccess::Write(bindings),
            operators: Some(operators),
            source_words: Some(SourceWordAccess::Read(source_words)),
            globals: Some(globals),
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) fn with_user_source_word_publication_and_operators(
        bindings: &'a mut Bindings,
        source_words: &'a SourceWordRegistry,
        operators: OperatorLookup,
        globals: &'a mut GlobalVariables,
    ) -> Self {
        Self {
            bindings: BindingAccess::Write(bindings),
            operators: Some(operators),
            source_words: Some(SourceWordAccess::Write(source_words)),
            globals: Some(globals),
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) fn with_runtime_definition_publication_and_operators(
        bindings: &'a mut Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
        globals: &'a mut GlobalVariables,
        code: &'a mut PublishedCode,
        words: &'a mut PublishedWords,
    ) -> Self {
        Self {
            bindings: BindingAccess::Write(bindings),
            operators: Some(operators),
            source_words: Some(SourceWordAccess::Read(source_words)),
            globals: Some(globals),
            arrays: None,
            runtime_definitions: Some(RuntimeDefinitionPublicationAccess { code, words }),
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) fn with_source_word_and_runtime_publication_and_operators(
        bindings: &'a mut Bindings,
        source_words: &'a mut SourceWordRegistry,
        operators: OperatorLookup,
        globals: &'a mut GlobalVariables,
        code: &'a mut PublishedCode,
        words: &'a mut PublishedWords,
    ) -> Self {
        Self {
            bindings: BindingAccess::Write(bindings),
            operators: Some(operators),
            source_words: Some(SourceWordAccess::Write(source_words)),
            globals: Some(globals),
            arrays: None,
            runtime_definitions: Some(RuntimeDefinitionPublicationAccess { code, words }),
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) fn bindings(&self) -> &Bindings {
        match &self.bindings {
            BindingAccess::Read(bindings) => bindings,
            BindingAccess::Write(bindings) => bindings,
        }
    }

    pub(crate) fn with_additional_source_capability(mut self) -> Self {
        self.additional_source_capability = true;
        self
    }

    pub(crate) fn with_global_arrays(mut self, arrays: &'a mut GlobalArrays) -> Self {
        self.arrays = Some(arrays);
        self
    }

    pub(crate) const fn operators(&self) -> Option<OperatorLookup> {
        self.operators
    }

    pub(crate) fn source_words(&self) -> Option<SourceWordLookup<'_>> {
        match &self.source_words {
            Some(SourceWordAccess::Read(lookup)) => Some(*lookup),
            Some(SourceWordAccess::Write(registry)) => Some(registry.lookup()),
            None => None,
        }
    }

    fn publication_context(&mut self) -> Option<(&mut Bindings, &mut GlobalVariables)> {
        let globals = self.globals.as_deref_mut()?;
        let bindings = match &mut self.bindings {
            BindingAccess::Read(_) => return None,
            BindingAccess::Write(bindings) => &mut **bindings,
        };

        Some((bindings, globals))
    }
}

impl<'a> DefinitionBodyCompileContext<'a> {
    pub(crate) const fn new(bindings: &'a Bindings) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: None,
            local_references: None,
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: None,
            local_references: None,
        }
    }

    pub(crate) const fn with_source_words_and_operators(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: Some(source_words),
            local_references: None,
        }
    }

    pub(crate) const fn with_local_references(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
        local_references: &'a DefinitionLocalReferences,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: Some(source_words),
            local_references: Some(local_references),
        }
    }
}

impl<'a> QuotationBodyCompileContext<'a> {
    pub(crate) const fn new(bindings: &'a Bindings) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: None,
            local_references: None,
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: None,
            local_references: None,
        }
    }

    pub(crate) const fn with_source_words_and_operators(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: Some(source_words),
            local_references: None,
        }
    }

    pub(crate) const fn with_local_references(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
        local_references: &'a DefinitionLocalReferences,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: Some(source_words),
            local_references: Some(local_references),
        }
    }
}

impl<'a> DefinitionBodyStatements<'a> {
    const fn new(statements: &'a [SourceBlockStatement<'a>], terminal: Terminal) -> Self {
        Self {
            statements,
            terminal,
        }
    }
}

impl<'a> QuotationBodyStatements<'a> {
    const fn new(statements: &'a [SourceBlockStatement<'a>], terminal: Terminal) -> Self {
        Self {
            statements,
            terminal,
        }
    }
}

struct RuntimeDefinitionPublisherAdapter<'a, 'source> {
    view: SourceView<'source>,
    source_id: SourceId,
    operators: Option<OperatorLookup>,
    source_words: SourceWordLookup<'a>,
    code: &'a mut PublishedCode,
    words: &'a mut PublishedWords,
}

impl<'source> RuntimeDefinitionPublisher<'source>
    for RuntimeDefinitionPublisherAdapter<'_, 'source>
{
    fn publish_runtime_definition(
        &mut self,
        bindings: &mut Bindings,
        name: crate::name::NormalizedName,
        name_span: SourceSpan,
        local_references: &DefinitionLocalReferences,
        body: &[SourceBlockStatement<'source>],
        end_span: SourceSpan,
    ) -> Result<WordId, SourceWordError> {
        let mut body_error = None;
        let published = self
            .code
            .publish_new_word(self.words, bindings, name, |body_bindings, builder| {
                let result = compile_definition_body(
                    self.view,
                    self.source_id,
                    DefinitionBodyStatements::new(body, Terminal::Eof { span: end_span }),
                    DefinitionBodyCompileContext::with_local_references(
                        body_bindings,
                        self.source_words,
                        self.operators
                            .expect("runtime definition publication requires operators"),
                        local_references,
                    ),
                    builder,
                );
                if let Err(error) = result {
                    body_error = Some(error);
                    return Err(WordBodyBuildError::DefinitionBodyCompileRejected);
                }
                builder
                    .append_mapped(Instruction::Return, end_span)
                    .map(|_| ())
            })
            .map_err(|error| match error {
                NewWordPublicationError::NameConflict => {
                    SourceWordError::DefNameConflict { span: name_span }
                }
                NewWordPublicationError::ReservedName => {
                    SourceWordError::DefReservedName { span: name_span }
                }
                NewWordPublicationError::Build {
                    source: WordBodyBuildError::DefinitionBodyCompileRejected,
                } => SourceWordError::DefBodyCompile {
                    span: body_error
                        .as_ref()
                        .and_then(SourceProcessorError::primary_span)
                        .unwrap_or(name_span),
                },
                NewWordPublicationError::Build { .. } => {
                    SourceWordError::DefBodyBuild { span: end_span }
                }
                NewWordPublicationError::Definition { .. } => {
                    SourceWordError::DefDefinition { span: end_span }
                }
                NewWordPublicationError::BindingCommitInvariantViolated => {
                    SourceWordError::DefBindingCommitInvariantViolated { span: name_span }
                }
            })?;

        Ok(published.id())
    }
}

impl<'a> SourceExecutionContext<'a> {
    pub(crate) const fn with_runtime_environment(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
        code_spaces: &'a [InstructionView<'a>],
        source_mappings: &'a [InstructionSourceMappingView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: Some(source_words),
            code_spaces,
            source_mappings,
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn new(
        bindings: &'a Bindings,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: None,
            code_spaces: &[],
            source_mappings: &[],
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_operators(
        bindings: &'a Bindings,
        operators: OperatorLookup,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: None,
            code_spaces: &[],
            source_mappings: &[],
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_source_words(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: Some(source_words),
            code_spaces: &[],
            source_mappings: &[],
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_source_words_and_operators(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
        operators: OperatorLookup,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: Some(source_words),
            code_spaces: &[],
            source_mappings: &[],
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_code_spaces(
        bindings: &'a Bindings,
        code_spaces: &'a [InstructionView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: None,
            code_spaces,
            source_mappings: &[],
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_code_spaces_and_operators(
        bindings: &'a Bindings,
        operators: OperatorLookup,
        code_spaces: &'a [InstructionView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: None,
            code_spaces,
            source_mappings: &[],
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_code_spaces_and_mappings(
        bindings: &'a Bindings,
        code_spaces: &'a [InstructionView<'a>],
        source_mappings: &'a [InstructionSourceMappingView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: None,
            code_spaces,
            source_mappings,
            globals: None,
            arrays: None,
            words,
            primitives,
            output: None,
            input: None,
            random: None,
        }
    }

    pub(crate) const fn with_globals(mut self, globals: GlobalVariableView<'a>) -> Self {
        self.globals = Some(SourceGlobalAccess::Read(globals));
        self
    }

    pub(crate) fn with_mut_globals(
        mut self,
        globals: crate::global_variable::GlobalVariableViewMut<'a>,
    ) -> Self {
        self.globals = Some(SourceGlobalAccess::Write(globals));
        self
    }

    pub(crate) fn with_mut_arrays(
        mut self,
        arrays: crate::global_array::GlobalArrayViewMut<'a>,
    ) -> Self {
        self.arrays = Some(arrays);
        self
    }

    pub(crate) fn with_output(mut self, output: &'a mut dyn RuntimeOutput) -> Self {
        self.output = Some(output);
        self
    }

    pub(crate) fn with_input(mut self, input: &'a mut dyn RuntimeInput) -> Self {
        self.input = Some(input);
        self
    }

    pub(crate) fn with_random(mut self, random: &'a mut RandomState) -> Self {
        self.random = Some(random);
        self
    }

    fn compile_context(&self) -> SourceCompileContext<'_> {
        SourceCompileContext {
            bindings: BindingAccess::Read(self.bindings),
            operators: self.operators,
            source_words: self.source_words.map(SourceWordAccess::Read),
            globals: None,
            arrays: None,
            runtime_definitions: None,
            additional_source_capability: false,
            local_references: None,
            return_allowed: false,
        }
    }

    pub(crate) const fn code_spaces(&self) -> &'a [InstructionView<'a>] {
        self.code_spaces
    }

    pub(crate) const fn source_mappings(&self) -> &'a [InstructionSourceMappingView<'a>] {
        self.source_mappings
    }

    pub(crate) const fn words(&self) -> PublishedWordLookup<'a> {
        self.words
    }

    pub(crate) const fn primitives(&self) -> PrimitiveLookup<'a> {
        self.primitives
    }
}

impl SourceProcessorError {
    pub(crate) fn primary_span(&self) -> Option<SourceSpan> {
        match self {
            Self::ProcessingSessionFailed
            | Self::Source(_)
            | Self::CodeSpaceLookup(_)
            | Self::SourceMappingLookup(_) => None,
            Self::Lex(error) => match error {
                LexError::Source(_) => None,
                LexError::InvalidCharacter { span, .. } | LexError::InvalidLiteral { span, .. } => {
                    Some(*span)
                }
            },
            Self::Compile(error) => Some(error.span()),
            Self::InstructionBuild(_) => None,
            Self::SourceWordContextUnavailable { .. } | Self::SourceWordLookup(_) => None,
            Self::SourceWord(error) => error.primary_span(),
            Self::AdditionalSourceAcquisition { span, .. } => Some(*span),
            Self::Runtime(error) => error.source_span().ok().flatten(),
        }
    }

    fn from_expression_error(error: ExpressionError) -> Self {
        match error {
            ExpressionError::Source(error) => Self::Source(error),
            ExpressionError::Syntax(error) => Self::Compile(CompileError {
                span: error.span(),
                kind: CompileErrorKind::Expression {
                    source: error.kind(),
                },
            }),
            ExpressionError::Variable(error) => Self::Compile(CompileError {
                span: error.span(),
                kind: CompileErrorKind::ExpressionVariable {
                    source: error.kind(),
                },
            }),
            ExpressionError::Call(error) => Self::Compile(CompileError {
                span: error.span(),
                kind: CompileErrorKind::ExpressionCall {
                    source: error.kind(),
                },
            }),
            ExpressionError::InstructionBuild(error) => Self::InstructionBuild(error),
        }
    }
}

impl SourceRunResult {
    pub(crate) fn outcome(&self) -> RunOutcome {
        self.outcome
    }

    pub(crate) fn data_stack(&self) -> &[Value] {
        &self.data_stack
    }

    pub(crate) fn instruction_count(&self) -> usize {
        self.instruction_count
    }
}

impl RuntimeError {
    pub(crate) const fn vm(self) -> VmError {
        self.vm
    }

    pub(crate) const fn source_span(self) -> Result<Option<SourceSpan>, SourceMappingLookupError> {
        self.source_span
    }
}

impl CompileError {
    pub(crate) const fn span(&self) -> SourceSpan {
        self.span
    }

    pub(crate) fn kind(&self) -> CompileErrorKind {
        self.kind.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::bootstrap::{
        register_builtin_global_variables, register_builtin_source_words,
        register_native_source_word, register_native_source_word_with_markers, register_primitive,
    };
    use crate::global_array::GlobalArrays;
    use crate::global_variable::{GlobalVarId, GlobalVariables};
    use crate::instruction::InstructionSequence;
    use crate::lexer::InvalidCharacterReason;
    use crate::name::NormalizedName;
    use crate::operator::{
        register_named_operator_primitives, register_operator_primitives, OperatorSemantic,
        OperatorWords,
    };
    use crate::output_primitive::register_output_primitives;
    use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
    use crate::published_code::PublishedCode;
    use crate::redefinition::redefine_word;
    use crate::runtime_output::{RuntimeOutput, TestOutput};
    use crate::source::SourceTexts;
    use crate::source_mapping::{
        InstructionSourceMapping, SourceMappingLookup, SourceMappingLookupError,
    };
    use crate::source_word::SyntaxDefinitionErrorKind;
    use crate::source_word::{
        DefSyntaxErrorKind, DimSyntaxErrorKind, EvalSyntaxErrorKind, LetSyntaxErrorKind,
        NativeSourceWordContext, NativeStructuredSourceWordContext,
        NativeStructuredSourceWordOwner, SourceBlockItem, SourceBlockMarker, SourceWordRegistry,
        SourceWordSyntaxMarker, SourceWordSyntaxMarkerRole, StructuredBodyCapabilities,
        StructuredBodyContext, StructuredBuildTargetScope, StructuredLineNumberScope,
        StructuredSourceWordInstance, VarSyntaxErrorKind,
    };
    use crate::stack_primitive::register_stack_primitives;
    use crate::structured_grammar::{
        MarkerCardinality, MarkerGroup, MarkerIdentity, StructuredGrammar,
    };
    use crate::word::{CompletedWordDefinition, PrimitiveId, PublishedWords, WordId};
    use crate::word_lookup::PublishedWordLookup;
    use std::rc::Rc;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text, "test.tbx");
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn compile(text: &str) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let bindings = Bindings::new();
        let unit = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect("source should compile");
        (sources, id, unit)
    }

    fn compile_with_bindings(
        text: &str,
        bindings: &Bindings,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let unit = compile_source(sources.view(), id, SourceCompileContext::new(bindings))
            .expect("source should compile");
        (sources, id, unit)
    }

    fn compile_with_bindings_and_operators(
        text: &str,
        bindings: &Bindings,
        operators: OperatorLookup,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(bindings, operators),
        )
        .expect("source should compile with operators");
        (sources, id, unit)
    }

    fn run(text: &str) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let words = PublishedWords::new();
        let bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();
        let result = run_source(
            sources.view(),
            id,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("source should run");
        (sources, id, result)
    }

    fn compile_error(text: &str) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let bindings = Bindings::new();
        let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect_err("source should fail");
        (sources, id, error)
    }

    fn segment(text: &str) -> (SourceTexts, SourceId, SegmentedSource) {
        let (sources, id) = source(text);
        let segmented =
            SegmentedSource::collect(sources.view(), id).expect("source should segment");
        (sources, id, segmented)
    }

    fn compile_with_operators_error(text: &str) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let mut words = PublishedWords::new();
        let bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect_err("source should fail");
        (sources, id, error)
    }

    fn run_with_bindings_and_operators(
        text: &str,
        bindings: &Bindings,
        words: &PublishedWords,
        primitives: &PrimitiveRegistry,
        operators: OperatorLookup,
    ) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let result = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_operators(
                bindings,
                operators,
                PublishedWordLookup::new(words),
                primitives.lookup(),
            ),
        )
        .expect("source should run with operators");
        (sources, id, result)
    }

    fn run_with_bindings_operators_and_globals(
        text: &str,
        bindings: &Bindings,
        globals: &GlobalVariables,
        words: &PublishedWords,
        primitives: &PrimitiveRegistry,
        operators: OperatorLookup,
    ) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let result = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_operators(
                bindings,
                operators,
                PublishedWordLookup::new(words),
                primitives.lookup(),
            )
            .with_globals(globals.view()),
        )
        .expect("source should run with operators and globals");
        (sources, id, result)
    }

    fn run_with_source_words_operators_and_mut_globals(
        text: &str,
        bindings: &Bindings,
        globals: &mut GlobalVariables,
        source_words: &SourceWordRegistry,
        words: &PublishedWords,
        primitives: &PrimitiveRegistry,
        operators: OperatorLookup,
    ) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let result = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_source_words_and_operators(
                bindings,
                source_words.lookup(),
                operators,
                PublishedWordLookup::new(words),
                primitives.lookup(),
            )
            .with_mut_globals(globals.view_mut()),
        )
        .expect("LET source should run with mutable globals");

        (sources, id, result)
    }

    fn publish_user_source_word(
        text: &str,
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &mut SourceWordRegistry,
        operators: OperatorLookup,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_user_source_word_publication_and_operators(
                bindings,
                source_words,
                operators,
                globals,
            ),
        )
        .expect("user-defined source word should publish");
        (sources, id, unit)
    }

    fn publish_user_source_word_error(
        text: &str,
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &mut SourceWordRegistry,
        operators: OperatorLookup,
    ) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_user_source_word_publication_and_operators(
                bindings,
                source_words,
                operators,
                globals,
            ),
        )
        .expect_err("user-defined source word should fail");
        (sources, id, error)
    }

    fn publish_test_if(
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &mut SourceWordRegistry,
        operators: OperatorLookup,
    ) {
        publish_user_source_word(
            "SYNTAX IF\nBLOCK\nSTART\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_ANY ELSIF\nEMIT_BRANCH_COMPLETE\nPATCH_FOLLOWING\nREAD_EXPR AS elsif_condition\nEMIT_EXPR elsif_condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_OPTIONAL ELSE\nEMIT_BRANCH_COMPLETE\nPATCH_FOLLOWING\nEXPECT_END\nLAST ENDIF\nEXPECT_END\nPATCH_FOLLOWING\nPATCH_COMPLETE\nENDS",
            bindings,
            globals,
            source_words,
            operators,
        );
    }

    fn emit_source_word_marker(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let first = context.source_word_token();
        context.append_mapped(Instruction::Push(value(99)), first.span())
    }

    fn request_additional_source_for_test(
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

    fn start_requesting_structured_source_word(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        request_additional_source_for_test(context)?;
        unreachable!("a structured source request should return an error in its body")
    }

    fn consume_one_following_statement(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let read = context
            .block_reader_mut()
            .expect("block reader should be available to source words")
            .next_statement()?;
        let SourceBlockRead::Statement(statement) = read else {
            let span = match read {
                SourceBlockRead::Terminal(terminal) => terminal
                    .eof_span()
                    .unwrap_or_else(|| context.source_word_token().span()),
                SourceBlockRead::Statement(_) => unreachable!(),
            };
            return Err(SourceWordError::UnsupportedSourceWord { span });
        };

        context.append_mapped(
            Instruction::Push(value(statement.tokens().len() as i16)),
            statement.span(),
        )
    }

    fn consume_two_following_statements(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        for _ in 0..2 {
            consume_one_following_statement(context)?;
        }
        Ok(())
    }

    fn consume_standalone_marker(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let read = context
            .block_reader_mut()
            .expect("block reader should be available to source words")
            .next_statement()?;
        let SourceBlockRead::Statement(statement) = read else {
            return Err(SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            });
        };
        if let Some(token) = statement.standalone_name() {
            let source_name = context
                .view()
                .slice(token.span())
                .map_err(|source| SourceWordError::Source { source })?;
            if source_name.eq_ignore_ascii_case("END") {
                return context.append_mapped(Instruction::Push(value(0)), statement.span());
            }
        }

        context.append_mapped(
            Instruction::Push(value(statement.tokens().len() as i16)),
            statement.span(),
        )
    }

    fn classify_one_declared_block_item(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let item = context
            .block_reader_mut()
            .expect("block reader should be available to source words")
            .next_item()?;

        let (emitted, span) = match item {
            SourceBlockItem::Statement(statement) => {
                (statement.tokens().len() as i16, statement.span())
            }
            SourceBlockItem::Marker(marker) => {
                let value = match marker.role() {
                    SourceWordSyntaxMarkerRole::BlockContinuation => 10,
                    SourceWordSyntaxMarkerRole::BlockTerminator => 20,
                };
                assert_eq!(marker.statement().span(), marker.span());
                assert_eq!(marker.span().source_id(), marker.token().span().source_id());
                assert!(marker.span().start() <= marker.token().span().start());
                assert!(marker.token().span().end() <= marker.span().end());
                assert!(!marker.name().as_str().is_empty());
                (value, marker.span())
            }
            SourceBlockItem::Terminal(terminal) => {
                let span = terminal
                    .eof_span()
                    .unwrap_or_else(|| context.source_word_token().span());
                (30, span)
            }
        };

        context.append_mapped(Instruction::Push(value(emitted)), span)
    }

    fn consume_until_declared_terminator(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        loop {
            let item = context
                .block_reader_mut()
                .expect("block reader should be available to source words")
                .next_item()?;
            match item {
                SourceBlockItem::Statement(statement) => {
                    context.append_mapped(
                        Instruction::Push(value(statement.tokens().len() as i16)),
                        statement.span(),
                    )?;
                }
                SourceBlockItem::Marker(marker)
                    if marker.role() == SourceWordSyntaxMarkerRole::BlockTerminator =>
                {
                    context.append_mapped(Instruction::Push(value(20)), marker.span())?;
                    return Ok(());
                }
                SourceBlockItem::Marker(marker) => {
                    context.append_mapped(Instruction::Push(value(10)), marker.span())?;
                }
                SourceBlockItem::Terminal(terminal) => {
                    let span = terminal
                        .eof_span()
                        .unwrap_or_else(|| context.source_word_token().span());
                    return Err(SourceWordError::UnsupportedSourceWord { span });
                }
            }
        }
    }

    fn read_eof_terminal_as_missing_terminator(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let read = context
            .block_reader_mut()
            .expect("block reader should be available to source words")
            .next_statement()?;
        let SourceBlockRead::Terminal(SourceBlockTerminal::Eof { span }) = read else {
            return Err(SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            });
        };

        Err(SourceWordError::UnsupportedSourceWord { span })
    }

    fn observe_lex_terminal_without_converting_to_eof(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let read = context
            .block_reader_mut()
            .expect("block reader should be available to source words")
            .next_statement()?;
        let SourceBlockRead::Terminal(terminal) = read else {
            return Err(SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            });
        };
        assert!(terminal.lex_error().is_some());
        Ok(())
    }

    fn nested_reader_fixture(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        fn consume_inner<'source>(
            reader: &mut SourceBlockReader<'source, '_>,
        ) -> Result<SourceBlockRead<'source>, SourceWordError> {
            reader.next_statement()
        }

        let (first, second) = {
            let reader = context
                .block_reader_mut()
                .expect("block reader should be available to source words");
            let first = consume_inner(reader)?;
            let second = reader.next_statement()?;
            (first, second)
        };

        assert!(matches!(first, SourceBlockRead::Statement(_)));
        let SourceBlockRead::Statement(statement) = second else {
            return Err(SourceWordError::UnsupportedSourceWord {
                span: context.source_word_token().span(),
            });
        };
        context.append_mapped(Instruction::Push(value(2)), statement.span())
    }

    #[derive(Debug)]
    struct StructuredProbeOwner {
        body_contexts: Vec<StructuredBodyContext>,
        context_index: usize,
        marker_value: i16,
        complete_value: i16,
        commit_targets: Vec<usize>,
        fail_completion: bool,
    }

    impl StructuredProbeOwner {
        fn inherited() -> Self {
            Self {
                body_contexts: vec![StructuredBodyContext::inherited()],
                context_index: 0,
                marker_value: 20,
                complete_value: 30,
                commit_targets: Vec::new(),
                fail_completion: false,
            }
        }

        fn without_publication() -> Self {
            Self {
                body_contexts: vec![StructuredBodyContext::new(
                    StructuredBuildTargetScope::Enclosing,
                    StructuredLineNumberScope::Enclosing,
                    StructuredBodyCapabilities::without_publication(),
                )],
                context_index: 0,
                marker_value: 20,
                complete_value: 30,
                commit_targets: Vec::new(),
                fail_completion: false,
            }
        }

        fn split_line_number_scopes() -> Self {
            Self {
                body_contexts: vec![
                    StructuredBodyContext::new(
                        StructuredBuildTargetScope::Enclosing,
                        StructuredLineNumberScope::OwnerLocal(0),
                        StructuredBodyCapabilities::inherit(),
                    ),
                    StructuredBodyContext::new(
                        StructuredBuildTargetScope::Enclosing,
                        StructuredLineNumberScope::OwnerLocal(1),
                        StructuredBodyCapabilities::inherit(),
                    ),
                ],
                context_index: 0,
                marker_value: 20,
                complete_value: 30,
                commit_targets: Vec::new(),
                fail_completion: false,
            }
        }

        fn owner_local_target() -> Self {
            Self {
                body_contexts: vec![StructuredBodyContext::new(
                    StructuredBuildTargetScope::OwnerLocal(0),
                    StructuredLineNumberScope::OwnerLocal(0),
                    StructuredBodyCapabilities::inherit(),
                )],
                context_index: 0,
                marker_value: 20,
                complete_value: 30,
                commit_targets: vec![0],
                fail_completion: false,
            }
        }

        fn split_owner_local_targets() -> Self {
            Self {
                body_contexts: vec![
                    StructuredBodyContext::new(
                        StructuredBuildTargetScope::OwnerLocal(0),
                        StructuredLineNumberScope::OwnerLocal(0),
                        StructuredBodyCapabilities::inherit(),
                    ),
                    StructuredBodyContext::new(
                        StructuredBuildTargetScope::OwnerLocal(1),
                        StructuredLineNumberScope::OwnerLocal(1),
                        StructuredBodyCapabilities::inherit(),
                    ),
                ],
                context_index: 0,
                marker_value: 20,
                complete_value: 30,
                commit_targets: vec![0, 1],
                fail_completion: false,
            }
        }

        fn failing_owner_local_target() -> Self {
            Self {
                fail_completion: true,
                ..Self::owner_local_target()
            }
        }
    }

    impl NativeStructuredSourceWordOwner for StructuredProbeOwner {
        fn current_body_context(&self) -> StructuredBodyContext {
            self.body_contexts[self.context_index]
        }

        fn accept_marker<'source>(
            &mut self,
            context: &mut NativeStructuredSourceWordContext<'source, '_>,
            marker: SourceBlockMarker<'source>,
            _accept: GrammarAccept,
        ) -> Result<(), SourceWordError> {
            context.append_mapped(Instruction::Push(value(self.marker_value)), marker.span())?;
            self.context_index = (self.context_index + 1).min(self.body_contexts.len() - 1);
            Ok(())
        }

        fn complete<'source>(
            &mut self,
            context: &mut NativeStructuredSourceWordContext<'source, '_>,
            marker: SourceBlockMarker<'source>,
        ) -> Result<(), SourceWordError> {
            if self.fail_completion {
                return Err(SourceWordError::UnsupportedSourceWord {
                    span: marker.span(),
                });
            }
            if self.commit_targets.is_empty() {
                return context
                    .append_mapped(Instruction::Push(value(self.complete_value)), marker.span());
            }
            for target in &self.commit_targets {
                context.append_owner_local_target(*target, marker.span())?;
            }
            Ok(())
        }
    }

    fn start_structured_probe(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        assert!(context.block_reader_mut().is_none());
        context.append_mapped(
            Instruction::Push(value(10)),
            context.source_word_token().span(),
        )?;
        Ok(StructuredSourceWordInstance::new(Box::new(
            StructuredProbeOwner::inherited(),
        )))
    }

    fn start_no_publication_probe(
        _context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        Ok(StructuredSourceWordInstance::new(Box::new(
            StructuredProbeOwner::without_publication(),
        )))
    }

    fn start_split_scope_probe(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        context.append_mapped(
            Instruction::Push(value(10)),
            context.source_word_token().span(),
        )?;
        Ok(StructuredSourceWordInstance::new(Box::new(
            StructuredProbeOwner::split_line_number_scopes(),
        )))
    }

    fn start_owner_local_target_probe(
        _context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        Ok(StructuredSourceWordInstance::new(Box::new(
            StructuredProbeOwner::owner_local_target(),
        )))
    }

    fn start_split_owner_local_targets_probe(
        _context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        Ok(StructuredSourceWordInstance::new(Box::new(
            StructuredProbeOwner::split_owner_local_targets(),
        )))
    }

    fn start_failing_owner_local_target_probe(
        _context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError> {
        Ok(StructuredSourceWordInstance::new(Box::new(
            StructuredProbeOwner::failing_owner_local_target(),
        )))
    }

    fn structured_grammar(
        groups: Vec<(&str, MarkerCardinality)>,
        terminator: &str,
    ) -> StructuredGrammar {
        StructuredGrammar::new(
            groups
                .into_iter()
                .map(|(marker_name, cardinality)| {
                    MarkerGroup::new(MarkerIdentity::new(name(marker_name)), cardinality)
                })
                .collect(),
            Some(MarkerIdentity::new(name(terminator))),
        )
        .expect("test grammar should be valid")
    }

    fn register_structured_probe(
        source_words: &mut SourceWordRegistry,
        bindings: &mut Bindings,
        word_name: &str,
        start: crate::source_word::NativeStructuredSourceWordStartHandler,
        markers: Vec<SourceWordSyntaxMarker>,
        grammar: StructuredGrammar,
    ) -> SourceWordId {
        let id = source_words.register_structured(start, grammar, markers);
        bindings
            .insert_new(name(word_name), Binding::SourceWord(id))
            .expect("structured source word binding should register");
        id
    }

    fn compile_with_var(
        text: &str,
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &SourceWordRegistry,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_word_publication(
                bindings,
                source_words.lookup(),
                globals,
            ),
        )
        .expect("VAR source should compile");
        (sources, id, unit)
    }

    fn compile_with_dim(
        text: &str,
        bindings: &mut Bindings,
        arrays: &mut GlobalArrays,
        source_words: &SourceWordRegistry,
    ) -> Result<(SourceTexts, SourceId, TemporaryExecutionUnit), SourceProcessorError> {
        let (sources, id) = source(text);
        let mut globals = GlobalVariables::new();
        let context = SourceCompileContext::with_source_word_publication(
            bindings,
            source_words.lookup(),
            &mut globals,
        )
        .with_global_arrays(arrays);
        compile_source(sources.view(), id, context).map(|unit| (sources, id, unit))
    }

    fn compile_with_var_error(
        text: &str,
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &SourceWordRegistry,
    ) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_word_publication(
                bindings,
                source_words.lookup(),
                globals,
            ),
        )
        .expect_err("VAR source should fail");
        (sources, id, error)
    }

    fn compile_with_def(
        text: &str,
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &SourceWordRegistry,
        operators: OperatorLookup,
        code: &mut PublishedCode,
        words: &mut PublishedWords,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_runtime_definition_publication_and_operators(
                bindings,
                source_words.lookup(),
                operators,
                globals,
                code,
                words,
            ),
        )
        .expect("DEF source should compile");
        (sources, id, unit)
    }

    fn compile_with_def_error(
        text: &str,
        bindings: &mut Bindings,
        globals: &mut GlobalVariables,
        source_words: &SourceWordRegistry,
        operators: OperatorLookup,
        code: &mut PublishedCode,
        words: &mut PublishedWords,
    ) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_runtime_definition_publication_and_operators(
                bindings,
                source_words.lookup(),
                operators,
                globals,
                code,
                words,
            ),
        )
        .expect_err("DEF source should fail");
        (sources, id, error)
    }

    fn compile_body(
        text: &str,
        context: DefinitionBodyCompileContext<'_>,
    ) -> (SourceTexts, SourceId, PublishedCode) {
        let (sources, id, segmented) = segment(text);
        let mut code = PublishedCode::new();
        compile_body_into(&mut code, sources.view(), id, &segmented, context)
            .expect("definition body should compile");
        (sources, id, code)
    }

    fn compile_body_error(
        text: &str,
        context: DefinitionBodyCompileContext<'_>,
    ) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id, segmented) = segment(text);
        let mut code = PublishedCode::new();
        let error = compile_body_into(&mut code, sources.view(), id, &segmented, context)
            .expect_err("definition body should fail");
        (sources, id, error)
    }

    fn compile_body_into(
        code: &mut PublishedCode,
        view: SourceView<'_>,
        source_id: SourceId,
        segmented: &SegmentedSource,
        context: DefinitionBodyCompileContext<'_>,
    ) -> Result<(), SourceProcessorError> {
        let statements = segmented
            .completed_statements()
            .iter()
            .map(|statement| {
                Ok(SourceBlockStatement::new(
                    statement.tokens(),
                    statement.span(view, source_id)?,
                ))
            })
            .collect::<Result<Vec<_>, SourceError>>()?;
        code.test_build_word_body(|builder| {
            compile_definition_body(
                view,
                source_id,
                DefinitionBodyStatements::new(&statements, segmented.terminal()),
                context,
                builder,
            )
        })
    }

    fn compile_quotation(
        text: &str,
        context: QuotationBodyCompileContext<'_>,
    ) -> (SourceTexts, SourceId, StaticQuotation) {
        let (sources, id, segmented) = segment(text);
        let quotation = compile_quotation_from_segmented(sources.view(), id, &segmented, context)
            .expect("quotation body should compile");
        (sources, id, quotation)
    }

    fn compile_quotation_error(
        text: &str,
        context: QuotationBodyCompileContext<'_>,
    ) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id, segmented) = segment(text);
        let error = compile_quotation_from_segmented(sources.view(), id, &segmented, context)
            .expect_err("quotation body should fail");
        (sources, id, error)
    }

    fn compile_quotation_from_segmented(
        view: SourceView<'_>,
        source_id: SourceId,
        segmented: &SegmentedSource,
        context: QuotationBodyCompileContext<'_>,
    ) -> Result<StaticQuotation, SourceProcessorError> {
        let statements = segmented
            .completed_statements()
            .iter()
            .map(|statement| {
                Ok(SourceBlockStatement::new(
                    statement.tokens(),
                    statement.span(view, source_id)?,
                ))
            })
            .collect::<Result<Vec<_>, SourceError>>()?;
        compile_quotation_body(
            view,
            source_id,
            QuotationBodyStatements::new(&statements, segmented.terminal()),
            context,
        )
    }

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn token_kinds(tokens: &[Token]) -> Vec<TokenKind> {
        tokens.iter().map(|token| token.kind()).collect()
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    fn location(unit: &TemporaryExecutionUnit, index: usize) -> CodeLocation {
        unit.instructions().location(address(index))
    }

    fn quotation_location(quotation: &StaticQuotation, index: usize) -> CodeLocation {
        quotation.instruction_view().location(address(index))
    }

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn marker(input: &str, role: SourceWordSyntaxMarkerRole) -> SourceWordSyntaxMarker {
        SourceWordSyntaxMarker::new(name(input), role)
    }

    fn completed_primitive(slot: usize) -> CompletedWordDefinition {
        CompletedWordDefinition::primitive(PrimitiveId::from_slot(slot))
    }

    fn operator_fixture() -> (PublishedWords, PrimitiveRegistry, OperatorWords) {
        let mut words = PublishedWords::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        (words, primitives, operators)
    }

    fn global_source_fixture() -> (
        PublishedWords,
        PrimitiveRegistry,
        OperatorWords,
        SourceWordRegistry,
        Bindings,
        GlobalVariables,
        Vec<GlobalVarId>,
    ) {
        let mut words = PublishedWords::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut globals = GlobalVariables::new();
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        publish_user_source_word(
            "SYNTAX IF\nBLOCK\nSTART\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_ANY ELSIF\nEMIT_BRANCH_COMPLETE\nPATCH_FOLLOWING\nREAD_EXPR AS elsif_condition\nEMIT_EXPR elsif_condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_OPTIONAL ELSE\nEMIT_BRANCH_COMPLETE\nPATCH_FOLLOWING\nEXPECT_END\nLAST ENDIF\nEXPECT_END\nPATCH_FOLLOWING\nPATCH_COMPLETE\nENDS",
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );

        (
            words,
            primitives,
            operators,
            source_words,
            bindings,
            globals,
            variables,
        )
    }

    struct RuntimeDefinitionSession {
        words: PublishedWords,
        primitives: PrimitiveRegistry,
        operators: OperatorWords,
        source_words: SourceWordRegistry,
        bindings: Bindings,
        globals: GlobalVariables,
        code: PublishedCode,
    }

    impl RuntimeDefinitionSession {
        fn new() -> Self {
            let (words, primitives, operators) = operator_fixture();
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();
            register_builtin_source_words(&mut source_words, &mut bindings)
                .expect("built-in source words should bootstrap");
            publish_user_source_word(
                "SYNTAX IF\nBLOCK\nSTART\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_ANY ELSIF\nEMIT_BRANCH_COMPLETE\nPATCH_FOLLOWING\nREAD_EXPR AS elsif_condition\nEMIT_EXPR elsif_condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_OPTIONAL ELSE\nEMIT_BRANCH_COMPLETE\nPATCH_FOLLOWING\nEXPECT_END\nLAST ENDIF\nEXPECT_END\nPATCH_FOLLOWING\nPATCH_COMPLETE\nENDS",
                &mut bindings,
                &mut GlobalVariables::new(),
                &mut source_words,
                operators.lookup(),
            );

            Self {
                words,
                primitives,
                operators,
                source_words,
                bindings,
                globals: GlobalVariables::new(),
                code: PublishedCode::new(),
            }
        }

        fn new_with_named_operators() -> Self {
            let mut words = PublishedWords::new();
            let mut primitives = PrimitiveRegistry::new();
            let mut bindings = Bindings::new();
            let operators =
                register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                    .expect("named operators should bootstrap");
            let mut source_words = SourceWordRegistry::new();
            register_builtin_source_words(&mut source_words, &mut bindings)
                .expect("built-in source words should bootstrap");

            Self {
                words,
                primitives,
                operators,
                source_words,
                bindings,
                globals: GlobalVariables::new(),
                code: PublishedCode::new(),
            }
        }

        fn new_with_named_operators_and_stack_primitives() -> Self {
            let mut session = Self::new_with_named_operators();
            register_stack_primitives(
                &mut session.primitives,
                &mut session.words,
                &mut session.bindings,
            )
            .expect("stack primitives should bootstrap");
            session
        }

        fn new_with_named_operators_and_output_primitives() -> Self {
            let mut session = Self::new_with_named_operators();
            register_output_primitives(
                &mut session.primitives,
                &mut session.words,
                &mut session.bindings,
            )
            .expect("output primitives should bootstrap");
            session
        }

        fn publish_def(&mut self, text: &str) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
            compile_with_def(
                text,
                &mut self.bindings,
                &mut self.globals,
                &self.source_words,
                self.operators.lookup(),
                &mut self.code,
                &mut self.words,
            )
        }

        fn publish_syntax(
            &mut self,
            text: &str,
        ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
            publish_user_source_word(
                text,
                &mut self.bindings,
                &mut self.globals,
                &mut self.source_words,
                self.operators.lookup(),
            )
        }

        fn publish_def_error(
            &mut self,
            text: &str,
        ) -> (SourceTexts, SourceId, SourceProcessorError) {
            compile_with_def_error(
                text,
                &mut self.bindings,
                &mut self.globals,
                &self.source_words,
                self.operators.lookup(),
                &mut self.code,
                &mut self.words,
            )
        }

        fn compile_caller(&self, text: &str) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
            compile_with_bindings(text, &self.bindings)
        }

        fn run_unit_with_published_code(
            &self,
            unit: &TemporaryExecutionUnit,
        ) -> Result<SourceRunResult, SourceProcessorError> {
            let code_spaces = [self.code.instruction_view()];
            let source_mappings = [self.code.source_mapping()];
            run_unit(
                unit,
                SourceExecutionContext::with_code_spaces_and_mappings(
                    &self.bindings,
                    &code_spaces,
                    &source_mappings,
                    PublishedWordLookup::new(&self.words),
                    self.primitives.lookup(),
                ),
            )
        }

        fn run_unit_with_output(
            &self,
            unit: &TemporaryExecutionUnit,
            output: &mut dyn RuntimeOutput,
        ) -> Result<SourceRunResult, SourceProcessorError> {
            run_unit(
                unit,
                SourceExecutionContext::with_source_words_and_operators(
                    &self.bindings,
                    self.source_words.lookup(),
                    self.operators.lookup(),
                    PublishedWordLookup::new(&self.words),
                    self.primitives.lookup(),
                )
                .with_output(output),
            )
        }

        fn run_unit_with_published_code_and_output(
            &self,
            unit: &TemporaryExecutionUnit,
            output: &mut dyn RuntimeOutput,
        ) -> Result<SourceRunResult, SourceProcessorError> {
            let code_spaces = [self.code.instruction_view()];
            let source_mappings = [self.code.source_mapping()];
            run_unit(
                unit,
                SourceExecutionContext::with_code_spaces_and_mappings(
                    &self.bindings,
                    &code_spaces,
                    &source_mappings,
                    PublishedWordLookup::new(&self.words),
                    self.primitives.lookup(),
                )
                .with_output(output),
            )
        }

        fn run_caller(&self, text: &str) -> (SourceTexts, SourceId, SourceRunResult) {
            let (sources, id, unit) = self.compile_caller(text);
            let result = self
                .run_unit_with_published_code(&unit)
                .expect("caller should run against published code");

            (sources, id, result)
        }

        fn register_primitive(
            &mut self,
            source_name: &str,
            primitive: fn(&mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError>,
        ) -> WordId {
            let primitive = self.primitives.register(primitive);
            register_primitive(
                &mut self.words,
                &mut self.bindings,
                name(source_name),
                primitive,
            )
            .expect("primitive should register")
        }
    }

    fn completed_compiled(code: &mut InstructionSequence, value: i16) -> CompletedWordDefinition {
        let entry = code.append(Instruction::Push(Value::integer(value)));
        CompletedWordDefinition::compiled(code.view().location(entry), code.view())
            .expect("test compiled entry should be valid")
    }

    fn completed_compiled_at(
        code: &InstructionSequence,
        entry: InstructionAddress,
    ) -> CompletedWordDefinition {
        CompletedWordDefinition::compiled(code.view().location(entry), code.view())
            .expect("test compiled entry should be valid")
    }

    fn mapping_for(
        code: &InstructionSequence,
        entries: &[(InstructionAddress, Option<SourceSpan>)],
    ) -> InstructionSourceMapping {
        let mut mapping = InstructionSourceMapping::new(code.code_space());
        for (address, span) in entries {
            match span {
                Some(span) => mapping
                    .append_mapped(*address, *span)
                    .expect("mapped instruction should append"),
                None => mapping
                    .append_unmapped(*address)
                    .expect("unmapped instruction should append"),
            }
        }
        mapping
    }

    fn assert_runtime_error(
        error: SourceProcessorError,
        expected_vm_location: CodeLocation,
        expected_span: Result<Option<SourceSpan>, SourceMappingLookupError>,
    ) -> RuntimeError {
        let SourceProcessorError::Runtime(error) = error else {
            panic!("expected runtime error");
        };

        assert_eq!(error.vm().location(), expected_vm_location);
        assert_eq!(error.source_span(), expected_span);
        error
    }

    fn publish_initial(
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        input: &str,
        definition: CompletedWordDefinition,
    ) -> WordId {
        let id = words.add(definition);
        bindings
            .insert_new(name(input), Binding::Word(id))
            .expect("initial test binding should register");
        id
    }

    fn push_7(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(7));
        Ok(())
    }

    fn push_1(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(1));
        Ok(())
    }

    fn push_2(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(2));
        Ok(())
    }

    fn push_3(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(3));
        Ok(())
    }

    fn push_4(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(4));
        Ok(())
    }

    fn push_5(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(5));
        Ok(())
    }

    fn push_41(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        context.push(value(41));
        Ok(())
    }

    fn add_top_two(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
        let (lhs, rhs) = context.pop2()?;
        context.push(value(lhs.as_integer() + rhs.as_integer()));
        Ok(())
    }

    fn fail_after_partial_stack_update(
        context: &mut PrimitiveContext<'_, '_>,
    ) -> Result<(), PrimitiveError> {
        context.pop()?;
        context.push(value(99));
        Err(PrimitiveError::Failed)
    }

    #[path = "basic_compile_tests.rs"]
    mod basic_compile_tests;

    #[path = "user_defined_source_word_tests.rs"]
    mod user_defined_source_word_tests;

    #[path = "execution_tests.rs"]
    mod execution_tests;

    #[path = "eval_dispatch_tests.rs"]
    mod eval_dispatch_tests;

    #[path = "mapping_diagnostic_tests.rs"]
    mod mapping_diagnostic_tests;

    #[path = "published_runtime_tests.rs"]
    mod published_runtime_tests;

    #[path = "runtime_mapping_tests.rs"]
    mod runtime_mapping_tests;
}
