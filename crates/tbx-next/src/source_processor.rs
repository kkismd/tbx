use crate::binding::Bindings;
use crate::block_code::BlockCodeBuilder;
use crate::expression::{
    parse_expression, ExpressionError, ExpressionSyntaxErrorKind, ExpressionVariableErrorKind,
};
use crate::global_variable::{GlobalVariableView, GlobalVariables};
use crate::instruction::{
    CodeLocation, CodeSpaceLookup, CodeSpaceLookupError, Instruction, InstructionAddress,
    InstructionView,
};
use crate::instruction_builder::{InstructionBuildError, InstructionBuildTarget};
use crate::lexer::{LexError, Lexer, Token, TokenKind};
use crate::line_number::{LineNumberError, LocalLineNumber, LocalLineNumberTable};
use crate::operator::OperatorLookup;
use crate::primitive::PrimitiveLookup;
use crate::published_code::{
    NewWordPublicationError, PublishedCode, PublishedWordBuilder, WordBodyBuildError,
};
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::source_mapping::{
    InstructionSourceMappingView, SourceMappedCode, SourceMappingLookup, SourceMappingLookupError,
};
use crate::source_word::{
    NativeSourceWordBindingAccess, NativeSourceWordContext, NativeSourceWordContextParts,
    RuntimeDefinitionPublisher, SourceBlockCursor, SourceBlockRead, SourceBlockReader,
    SourceBlockStatement, SourceBlockTerminal, SourceWordError, SourceWordId, SourceWordLookup,
    SourceWordLookupError,
};
use crate::static_quotation::{StaticQuotation, StaticQuotationBuildError};
use crate::value::Value;
use crate::vm::{ExecutionView, RunOutcome, Vm, VmError};
use crate::word::{PublishedWords, WordId};
use crate::word_lookup::PublishedWordLookup;
use crate::word_resolution::{
    resolve_binding_name, resolve_word_name, ResolvedBinding, WordResolutionError,
};

#[derive(Debug)]
pub(crate) struct TemporaryExecutionUnit {
    code: SourceMappedCode,
    entry: CodeLocation,
}

pub(crate) struct SourceCompileContext<'a> {
    bindings: BindingAccess<'a>,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
    globals: Option<&'a mut GlobalVariables>,
    runtime_definitions: Option<RuntimeDefinitionPublicationAccess<'a>>,
}

pub(crate) struct DefinitionBodyCompileContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
}

pub(crate) struct QuotationBodyCompileContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
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

struct RuntimeDefinitionPublicationAccess<'a> {
    code: &'a mut PublishedCode,
    words: &'a mut PublishedWords,
}

#[derive(Debug)]
pub(crate) struct SourceExecutionContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
    source_words: Option<SourceWordLookup<'a>>,
    code_spaces: &'a [InstructionView<'a>],
    source_mappings: &'a [InstructionSourceMappingView<'a>],
    globals: Option<SourceGlobalAccess<'a>>,
    words: PublishedWordLookup<'a>,
    primitives: PrimitiveLookup<'a>,
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
    Source(SourceError),
    Lex(LexError),
    Compile(CompileError),
    CodeSpaceLookup(CodeSpaceLookupError),
    InstructionBuild(InstructionBuildError),
    SourceMappingLookup(SourceMappingLookupError),
    SourceWordContextUnavailable { id: SourceWordId },
    SourceWordLookup(SourceWordLookupError),
    SourceWord(SourceWordError),
    Runtime(RuntimeError),
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
    line_numbers: &'a mut LocalLineNumberTable,
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
        source_words: context.source_words,
        globals: None,
        runtime_definitions: None,
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
        source_words: context.source_words,
        // #1516/#1500: quotation bodies reuse statement lowering but have no
        // capability to publish bindings, globals, or runtime definitions.
        globals: None,
        runtime_definitions: None,
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
    let mut line_numbers = LocalLineNumberTable::new();
    let mut cursor = LogicalStatementCursor::new(view, source_id, statements, terminal);

    while let Some(statement) = cursor.next_completed_statement() {
        compile_statement(
            view,
            source_id,
            statement.tokens(),
            &mut context,
            &mut StatementCompileState {
                code,
                line_numbers: &mut line_numbers,
            },
            &mut cursor,
        )?;
    }

    line_numbers
        .resolve(code)
        .map_err(|source| line_number_compile_error(source).into())
}

fn compile_statement<'source, S>(
    view: SourceView<'source>,
    source_id: SourceId,
    statement: &'source [Token],
    context: &mut SourceCompileContext<'_>,
    state: &mut StatementCompileState<'_>,
    cursor: &mut LogicalStatementCursor<'source, 'source, S>,
) -> Result<(), SourceProcessorError>
where
    S: LogicalStatementView,
{
    if statement.is_empty() {
        return Ok(());
    }

    let (line_number, body) = split_statement_line_number(view, statement)?;
    let start = state.code.current_address();
    let local_line_number_prefix = line_number.map(|(_, span)| span);
    compile_statement_body(
        view,
        source_id,
        body,
        context,
        local_line_number_prefix,
        state,
        cursor,
    )?;

    if let Some((line_number, span)) = line_number {
        state
            .line_numbers
            .define(state.code, line_number, start, span)
            .map_err(|source| line_number_compile_error(source).into())
    } else {
        Ok(())
    }
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
    view: SourceView<'source>,
    source_id: SourceId,
    tokens: &'source [Token],
    context: &mut SourceCompileContext<'_>,
    local_line_number_prefix: Option<SourceSpan>,
    state: &mut StatementCompileState<'_>,
    cursor: &mut LogicalStatementCursor<'source, 'source, S>,
) -> Result<(), SourceProcessorError>
where
    S: LogicalStatementView,
{
    let Some((&first, _)) = tokens.split_first() else {
        return Ok(());
    };

    if is_bif_keyword(view, first)? {
        return compile_bif(
            view,
            source_id,
            tokens,
            context,
            state.code,
            state.line_numbers,
        );
    }

    if compile_statement_leading_source_word(
        view,
        source_id,
        tokens,
        context,
        local_line_number_prefix,
        state.code,
        cursor,
    )? {
        return Ok(());
    }

    if contains_expression_syntax(tokens) {
        return Err(CompileError {
            span: first.span(),
            kind: CompileErrorKind::BareExpression,
        }
        .into());
    }

    compile_simple_tokens(view, tokens, context, state.code)
}

fn compile_statement_leading_source_word<'source, S>(
    view: SourceView<'source>,
    source_id: SourceId,
    tokens: &'source [Token],
    context: &mut SourceCompileContext<'_>,
    local_line_number_prefix: Option<SourceSpan>,
    code: &mut dyn InstructionBuildTarget,
    cursor: &mut LogicalStatementCursor<'source, 'source, S>,
) -> Result<bool, SourceProcessorError>
where
    S: LogicalStatementView,
{
    let Some(first) = tokens.first().copied() else {
        return Ok(false);
    };
    if first.kind() != TokenKind::Name {
        return Ok(false);
    }

    let source_name = view.slice(first.span())?;
    let binding = match resolve_binding_name(context.bindings(), source_name) {
        Ok(binding) => binding,
        Err(WordResolutionError::InvalidWordName | WordResolutionError::UndefinedName) => {
            return Ok(false);
        }
        Err(WordResolutionError::TargetIsNotWord) => {
            unreachable!("binding-kind resolution does not classify published bindings as non-word")
        }
    };

    let ResolvedBinding::SourceWord(id) = binding else {
        return Ok(false);
    };
    let Some(source_words) = context.source_words() else {
        return Err(SourceProcessorError::SourceWordContextUnavailable { id });
    };
    let handler = source_words.lookup_handler(id)?;
    let syntax_markers = source_words.syntax_markers(id)?;
    let operators = context.operators();
    let globals = context.globals.as_deref_mut();
    let mut runtime_publisher =
        context
            .runtime_definitions
            .as_mut()
            .map(|publication| RuntimeDefinitionPublisherAdapter {
                view,
                source_id,
                operators,
                source_words,
                code: &mut *publication.code,
                words: &mut *publication.words,
            });
    let runtime_definitions = runtime_publisher
        .as_mut()
        .map(|publisher| publisher as &mut dyn RuntimeDefinitionPublisher<'source>);
    let binding_access = match &mut context.bindings {
        BindingAccess::Read(bindings) => NativeSourceWordBindingAccess::Read(bindings),
        BindingAccess::Write(bindings) => NativeSourceWordBindingAccess::Write(bindings),
    };
    let mut source_word_context = NativeSourceWordContext::new(NativeSourceWordContextParts {
        view,
        source_id,
        tokens,
        block_reader: Some(SourceBlockReader::new(view, cursor, syntax_markers)),
        bindings: binding_access,
        operators,
        code,
        local_line_number_prefix,
        globals,
        runtime_definitions,
    });
    handler(&mut source_word_context)?;
    Ok(true)
}

fn compile_simple_tokens(
    view: SourceView<'_>,
    tokens: &[Token],
    context: &SourceCompileContext<'_>,
    code: &mut dyn InstructionBuildTarget,
) -> Result<(), SourceProcessorError> {
    for token in tokens {
        match token.kind() {
            TokenKind::IntegerLiteral => {
                let value = compile_integer_literal(view, *token)?;
                code.append_mapped(Instruction::Push(Value::integer(value)), token.span())?;
            }
            TokenKind::Name => {
                let id = compile_word_reference(view, *token, context)?;
                code.append_mapped(Instruction::Call(id), token.span())?;
            }
            TokenKind::LineBoundary | TokenKind::Eof => {}
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Comma
            | TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::Equal
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual => {
                return Err(CompileError {
                    span: token.span(),
                    kind: CompileErrorKind::UnsupportedToken { kind: token.kind() },
                }
                .into());
            }
        }
    }

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
    code: &mut dyn InstructionBuildTarget,
) -> Result<(), SourceProcessorError> {
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
    parse_expression(view, &expression_tokens, operators, &resolver)
        .map_err(SourceProcessorError::from_expression_error)?
        .commit_to(code)
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

fn run_unit(
    unit: &TemporaryExecutionUnit,
    context: SourceExecutionContext<'_>,
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
    let mut vm = Vm::new_at_location_in(&mut execution, unit.entry)
        .map_err(|error| map_runtime_error(error, unit, context.source_mappings))?;
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
        Ok(ResolvedBinding::RuntimeWord(_) | ResolvedBinding::SourceWord(_)) => {
            Err(ExpressionVariableErrorKind::TargetIsNotVariable)
        }
        Err(WordResolutionError::InvalidWordName) => Err(ExpressionVariableErrorKind::InvalidName),
        Err(WordResolutionError::UndefinedName) => Err(ExpressionVariableErrorKind::UndefinedName),
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

// Segmentation is the source of truth for top-level statement boundaries.
// Lexical failure keeps already completed statements visible while leaving the
// unbounded tail unavailable to semantic compilation and source-wide analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentedSource {
    completed_statements: Vec<LogicalStatement>,
    incomplete_tail: Vec<Token>,
    terminal: Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogicalStatement {
    tokens: Vec<Token>,
}

trait LogicalStatementView {
    fn tokens(&self) -> &[Token];
    fn span(&self, view: SourceView<'_>, source_id: SourceId) -> Result<SourceSpan, SourceError>;
}

#[derive(Debug)]
struct LogicalStatementCursor<'source, 'statements, S> {
    view: SourceView<'source>,
    source_id: SourceId,
    statements: &'statements [S],
    terminal: Terminal,
    position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminal {
    Eof { span: SourceSpan },
    LexError(LexError),
}

impl SegmentedSource {
    fn collect(view: SourceView<'_>, source_id: SourceId) -> Result<Self, SourceProcessorError> {
        let mut collector = LogicalStatementCollector::new();
        let mut lexer = Lexer::new(view, source_id)?;

        loop {
            match lexer.next_token() {
                Ok(token) if token.kind() == TokenKind::Eof => {
                    return Ok(collector.finish(Terminal::Eof { span: token.span() }));
                }
                Ok(token) => collector.push_token(token),
                Err(error) => return Ok(collector.finish(Terminal::LexError(error))),
            }
        }
    }

    fn completed_statements(&self) -> &[LogicalStatement] {
        &self.completed_statements
    }

    fn terminal(&self) -> Terminal {
        self.terminal
    }

    #[cfg(test)]
    fn incomplete_tail(&self) -> &[Token] {
        &self.incomplete_tail
    }
}

impl LogicalStatement {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens }
    }

    fn tokens(&self) -> &[Token] {
        &self.tokens
    }
}

impl LogicalStatementView for LogicalStatement {
    fn tokens(&self) -> &[Token] {
        self.tokens()
    }

    fn span(&self, view: SourceView<'_>, source_id: SourceId) -> Result<SourceSpan, SourceError> {
        let first = self
            .tokens
            .first()
            .expect("logical statements are never empty");
        let last = self
            .tokens
            .last()
            .expect("logical statements are never empty");
        view.span(source_id, first.span().start(), last.span().end())
    }
}

impl LogicalStatementView for SourceBlockStatement<'_> {
    fn tokens(&self) -> &[Token] {
        SourceBlockStatement::tokens(*self)
    }

    fn span(&self, _view: SourceView<'_>, _source_id: SourceId) -> Result<SourceSpan, SourceError> {
        Ok(SourceBlockStatement::span(*self))
    }
}

impl<'source, 'statements, S> LogicalStatementCursor<'source, 'statements, S> {
    fn new(
        view: SourceView<'source>,
        source_id: SourceId,
        statements: &'statements [S],
        terminal: Terminal,
    ) -> Self {
        Self {
            view,
            source_id,
            statements,
            terminal,
            position: 0,
        }
    }

    fn next_completed_statement(&mut self) -> Option<&'statements S> {
        let statement = self.statements.get(self.position)?;
        self.position += 1;
        Some(statement)
    }
}

impl<'statements, S> SourceBlockCursor<'statements> for LogicalStatementCursor<'_, 'statements, S>
where
    S: LogicalStatementView + 'statements,
{
    fn read_next_block_statement(
        &mut self,
    ) -> Result<SourceBlockRead<'statements>, SourceWordError> {
        let Some(statement) = self.next_completed_statement() else {
            return Ok(SourceBlockRead::Terminal(match self.terminal {
                Terminal::Eof { span } => SourceBlockTerminal::Eof { span },
                Terminal::LexError(error) => SourceBlockTerminal::LexError { error },
            }));
        };

        let span = statement
            .span(self.view, self.source_id)
            .map_err(|source| SourceWordError::Source { source })?;
        Ok(SourceBlockRead::Statement(SourceBlockStatement::new(
            statement.tokens(),
            span,
        )))
    }
}

#[derive(Debug, Default)]
struct LogicalStatementCollector {
    completed_statements: Vec<LogicalStatement>,
    current_tokens: Vec<Token>,
    depth: usize,
}

impl LogicalStatementCollector {
    fn new() -> Self {
        Self {
            completed_statements: Vec::new(),
            current_tokens: Vec::new(),
            depth: 0,
        }
    }

    fn push_token(&mut self, token: Token) {
        match token.kind() {
            TokenKind::LParen => {
                self.depth = self.depth.saturating_add(1);
                self.current_tokens.push(token);
            }
            TokenKind::RParen => {
                self.depth = self.depth.saturating_sub(1);
                self.current_tokens.push(token);
            }
            TokenKind::LineBoundary if self.depth == 0 => self.finish_current_statement(),
            _ => self.current_tokens.push(token),
        }
    }

    fn finish(mut self, terminal: Terminal) -> SegmentedSource {
        if matches!(terminal, Terminal::Eof { .. }) {
            self.finish_current_statement();
        }

        SegmentedSource {
            completed_statements: self.completed_statements,
            incomplete_tail: self.current_tokens,
            terminal,
        }
    }

    fn finish_current_statement(&mut self) {
        if self.current_tokens.is_empty() {
            return;
        }

        let tokens = std::mem::take(&mut self.current_tokens);
        self.completed_statements
            .push(LogicalStatement::new(tokens));
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
            runtime_definitions: None,
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings: BindingAccess::Read(bindings),
            operators: Some(operators),
            source_words: None,
            globals: None,
            runtime_definitions: None,
        }
    }

    pub(crate) const fn with_source_words(
        bindings: &'a Bindings,
        source_words: SourceWordLookup<'a>,
    ) -> Self {
        Self {
            bindings: BindingAccess::Read(bindings),
            operators: None,
            source_words: Some(source_words),
            globals: None,
            runtime_definitions: None,
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
            source_words: Some(source_words),
            globals: None,
            runtime_definitions: None,
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
            source_words: Some(source_words),
            globals: Some(globals),
            runtime_definitions: None,
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
            source_words: Some(source_words),
            globals: Some(globals),
            runtime_definitions: None,
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
            source_words: Some(source_words),
            globals: Some(globals),
            runtime_definitions: Some(RuntimeDefinitionPublicationAccess { code, words }),
        }
    }

    pub(crate) fn bindings(&self) -> &Bindings {
        match &self.bindings {
            BindingAccess::Read(bindings) => bindings,
            BindingAccess::Write(bindings) => bindings,
        }
    }

    pub(crate) const fn operators(&self) -> Option<OperatorLookup> {
        self.operators
    }

    pub(crate) const fn source_words(&self) -> Option<SourceWordLookup<'a>> {
        self.source_words
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
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: None,
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
        }
    }
}

impl<'a> QuotationBodyCompileContext<'a> {
    pub(crate) const fn new(bindings: &'a Bindings) -> Self {
        Self {
            bindings,
            operators: None,
            source_words: None,
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings,
            operators: Some(operators),
            source_words: None,
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
                    DefinitionBodyCompileContext::with_source_words_and_operators(
                        body_bindings,
                        self.source_words,
                        self.operators
                            .expect("runtime definition publication requires operators"),
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
            words,
            primitives,
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
            words,
            primitives,
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
            words,
            primitives,
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
            words,
            primitives,
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
            words,
            primitives,
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
            words,
            primitives,
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
            words,
            primitives,
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

    fn compile_context(&self) -> SourceCompileContext<'_> {
        SourceCompileContext {
            bindings: BindingAccess::Read(self.bindings),
            operators: self.operators,
            source_words: self.source_words,
            globals: None,
            runtime_definitions: None,
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
    fn primary_span(&self) -> Option<SourceSpan> {
        match self {
            Self::Source(_) | Self::CodeSpaceLookup(_) | Self::SourceMappingLookup(_) => None,
            Self::Lex(error) => match error {
                LexError::Source(_) => None,
                LexError::InvalidCharacter { span, .. } => Some(*span),
            },
            Self::Compile(error) => Some(error.span()),
            Self::InstructionBuild(_) => None,
            Self::SourceWordContextUnavailable { .. } | Self::SourceWordLookup(_) => None,
            Self::SourceWord(error) => error.primary_span(),
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
    use crate::global_variable::{GlobalVarId, GlobalVariables};
    use crate::instruction::InstructionSequence;
    use crate::lexer::InvalidCharacterReason;
    use crate::name::NormalizedName;
    use crate::operator::{register_operator_primitives, OperatorSemantic, OperatorWords};
    use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
    use crate::published_code::PublishedCode;
    use crate::redefinition::redefine_word;
    use crate::source::SourceTexts;
    use crate::source_mapping::{
        InstructionSourceMapping, SourceMappingLookup, SourceMappingLookupError,
    };
    use crate::source_word::{
        DefSyntaxErrorKind, LetSyntaxErrorKind, NativeSourceWordContext, SourceBlockItem,
        SourceWordRegistry, SourceWordSyntaxMarker, SourceWordSyntaxMarkerRole, VarSyntaxErrorKind,
    };
    use crate::word::{CompletedWordDefinition, PrimitiveId, PublishedWords, WordId};
    use crate::word_lookup::PublishedWordLookup;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
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

    fn emit_source_word_marker(
        context: &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<(), SourceWordError> {
        let first = context.source_word_token();
        context.append_mapped(Instruction::Push(value(99)), first.span())
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
                assert_eq!(marker.token().span(), marker.span());
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
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");

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
            primitive: fn(&mut PrimitiveContext<'_>) -> Result<(), PrimitiveError>,
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

    fn push_7(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(7));
        Ok(())
    }

    fn push_1(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(1));
        Ok(())
    }

    fn push_2(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(2));
        Ok(())
    }

    fn push_3(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(3));
        Ok(())
    }

    fn push_4(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(4));
        Ok(())
    }

    fn push_5(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(5));
        Ok(())
    }

    fn push_41(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(41));
        Ok(())
    }

    fn add_top_two(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        let (lhs, rhs) = context.pop2()?;
        context.push(value(lhs.as_integer() + rhs.as_integer()));
        Ok(())
    }

    fn fail_after_partial_stack_update(
        context: &mut PrimitiveContext<'_>,
    ) -> Result<(), PrimitiveError> {
        context.pop()?;
        context.push(value(99));
        Err(PrimitiveError::Failed)
    }

    #[test]
    fn empty_source_compiles_to_halt_only_and_runs() {
        let (sources, id, unit) = compile("");
        let view = sources.view();

        assert_eq!(unit.entry(), address(0));
        assert_eq!(unit.entry_location(), location(&unit, 0));
        assert_eq!(unit.len(), 1);
        assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(view, id, 0, 0)))
        );

        let words = PublishedWords::new();
        let bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();
        let result = run_unit(
            &unit,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("halt-only unit should run");
        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), []);
        assert_eq!(result.instruction_count(), 1);
    }

    #[test]
    fn line_boundary_only_source_runs_as_halt_only() {
        for source in ["\n", "\r", "\r\n", "\n\r\n\r"] {
            let (sources, id, unit) = compile(source);
            let eof = source.len();

            assert_eq!(unit.len(), 1);
            assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
            assert_eq!(
                unit.source_span(location(&unit, 0)),
                Ok(Some(span(sources.view(), id, eof, eof))),
                "{source:?} should map Halt to EOF"
            );
        }
    }

    #[test]
    fn segmentation_collects_completed_statements_without_top_level_boundaries() {
        let (_sources, _id, segmented) = segment("1\n\n2 3\r\n");

        assert_eq!(segmented.completed_statements().len(), 2);
        assert_eq!(
            token_kinds(segmented.completed_statements()[0].tokens()),
            [TokenKind::IntegerLiteral]
        );
        assert_eq!(
            token_kinds(segmented.completed_statements()[1].tokens()),
            [TokenKind::IntegerLiteral, TokenKind::IntegerLiteral]
        );
        assert_eq!(segmented.incomplete_tail(), []);
        assert!(matches!(segmented.terminal(), Terminal::Eof { .. }));
    }

    #[test]
    fn segmentation_completes_final_non_empty_statement_at_eof() {
        let (_sources, _id, segmented) = segment("1 2");

        assert_eq!(segmented.completed_statements().len(), 1);
        assert_eq!(
            token_kinds(segmented.completed_statements()[0].tokens()),
            [TokenKind::IntegerLiteral, TokenKind::IntegerLiteral]
        );
        assert_eq!(segmented.incomplete_tail(), []);
    }

    #[test]
    fn segmentation_preserves_parenthesized_line_boundary_inside_statement() {
        let (_sources, _id, segmented) = segment("1 + (\n2)\n3");

        assert_eq!(segmented.completed_statements().len(), 2);
        assert_eq!(
            token_kinds(segmented.completed_statements()[0].tokens()),
            [
                TokenKind::IntegerLiteral,
                TokenKind::Plus,
                TokenKind::LParen,
                TokenKind::LineBoundary,
                TokenKind::IntegerLiteral,
                TokenKind::RParen
            ]
        );
        assert_eq!(
            token_kinds(segmented.completed_statements()[1].tokens()),
            [TokenKind::IntegerLiteral]
        );
    }

    #[test]
    fn segmentation_distinguishes_completed_prefix_from_lexical_failure() {
        let (sources, id, segmented) = segment("VAR SCORE\n@");

        assert_eq!(segmented.completed_statements().len(), 1);
        assert_eq!(
            token_kinds(segmented.completed_statements()[0].tokens()),
            [TokenKind::Name, TokenKind::Name]
        );
        assert_eq!(segmented.incomplete_tail(), []);
        assert_eq!(
            segmented.terminal(),
            Terminal::LexError(LexError::InvalidCharacter {
                span: span(sources.view(), id, 10, 11),
                character: '@',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn segmentation_keeps_unbounded_lexical_failure_prefix_as_incomplete_tail() {
        let (sources, id, segmented) = segment("VAR SCORE @");

        assert_eq!(segmented.completed_statements(), []);
        assert_eq!(
            token_kinds(segmented.incomplete_tail()),
            [TokenKind::Name, TokenKind::Name]
        );
        assert_eq!(
            segmented.terminal(),
            Terminal::LexError(LexError::InvalidCharacter {
                span: span(sources.view(), id, 10, 11),
                character: '@',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn standalone_integer_statement_is_rejected() {
        for source_text in ["100", "100 + 1"] {
            let (sources, id, error) = compile_with_operators_error(source_text);

            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, 0, 3),
                    kind: CompileErrorKind::BareExpression,
                }),
                "{source_text:?} should not compile as a statement"
            );
        }
    }

    #[test]
    fn top_level_bare_expression_is_rejected_before_expression_parsing() {
        let cases = [
            ("1 + 2 * 3", 0, 1),
            ("(1 + 2)", 0, 1),
            ("1 < 2", 0, 1),
            ("-1", 0, 1),
        ];

        for (source, start, end) in cases {
            let (sources, id, error) = compile_with_operators_error(source);
            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, start, end),
                    kind: CompileErrorKind::BareExpression,
                }),
                "{source:?} should not compile as a top-level statement"
            );
        }
    }

    #[test]
    fn unresolved_and_variable_leading_expression_inputs_are_not_rescued() {
        let mut globals = crate::global_variable::GlobalVariables::new();
        let variable = globals.allocate();
        let mut bindings = Bindings::new();
        bindings
            .insert_new(name("A"), Binding::Variable(variable))
            .expect("variable should register");

        let cases = [("MISSING + 1", 0, 7), ("A + 1", 0, 1)];

        for (input, start, end) in cases {
            let (sources, id) = source(input);
            let mut words = PublishedWords::new();
            let mut primitives = PrimitiveRegistry::new();
            let operators = register_operator_primitives(&mut primitives, &mut words);
            let error = compile_source(
                sources.view(),
                id,
                SourceCompileContext::with_operators(&bindings, operators.lookup()),
            )
            .expect_err("name-leading bare expression should fail");

            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, start, end),
                    kind: CompileErrorKind::BareExpression,
                }),
                "{input:?} should not be rescued as a top-level expression"
            );
        }
    }

    #[test]
    fn local_line_number_prefixed_bare_expression_is_rejected() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (sources, id) = source("100 MISSING + 2\nBIF 1, 100");
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect_err("line-number-prefixed bare expression should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 4, 11),
                kind: CompileErrorKind::BareExpression,
            })
        );
    }

    #[test]
    fn local_line_number_prefixed_missing_name_uses_word_resolution_error() {
        let (sources, id, error) = compile_error("100 MISSING");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 4, 11),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName,
                },
            })
        );
    }

    #[test]
    fn local_line_number_prefixed_runtime_word_still_compiles() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, result) = run_with_bindings_and_operators(
            "100 PUSH7\nBIF 1, 100",
            &bindings,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
    }

    #[test]
    fn bif_zero_condition_jumps_to_forward_line_number() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, result) = run_with_bindings_and_operators(
            "100 BIF 0, 200\nPUSH7\n200 push7",
            &bindings,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
    }

    #[test]
    fn bif_nonzero_condition_falls_through() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, result) = run_with_bindings_and_operators(
            "100 BIF 1, 200\nPUSH7\n200 push7",
            &bindings,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7), value(7)]);
    }

    #[test]
    fn bif_condition_uses_expression_precedence_and_comparison() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, result) = run_with_bindings_and_operators(
            "BIF 1 + 2 * 3 <> 7, 200\nPUSH7\n200 push7",
            &bindings,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
    }

    #[test]
    fn bif_resolves_backward_line_number_without_cross_space_lookup() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, unit) = compile_with_bindings_and_operators(
            "100 push7\nBIF 1, 100",
            &bindings,
            operators.lookup(),
        );

        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::JumpIfZero(address(0)))
        );
    }

    #[test]
    fn unused_local_line_number_definition_is_accepted() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, result) = run_with_bindings_and_operators(
            "100 push7",
            &bindings,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
    }

    #[test]
    fn physical_line_integer_inside_parenthesized_continuation_is_not_line_number() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let (_sources, _id, result) = run_with_bindings_and_operators(
            "BIF (1 +\n2) = 4, 200\nPUSH7\n200 push7",
            &bindings,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
    }

    #[test]
    fn bif_expression_operator_calls_map_to_operator_source_spans() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let push7_id = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(push7_id))
            .expect("primitive word should register");
        let (sources, id, unit) = compile_with_bindings_and_operators(
            "BIF 1 + 2 * 3, 100\n100 PUSH7",
            &bindings,
            operators.lookup(),
        );
        let view = sources.view();

        assert_eq!(
            unit.instructions().get(address(3)),
            Ok(&Instruction::Call(
                operators.lookup().resolve(OperatorSemantic::Multiply)
            ))
        );
        assert_eq!(
            unit.instructions().get(address(4)),
            Ok(&Instruction::Call(
                operators.lookup().resolve(OperatorSemantic::Add)
            ))
        );
        assert_eq!(
            unit.source_span(location(&unit, 3)),
            Ok(Some(span(view, id, 10, 11)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 4)),
            Ok(Some(span(view, id, 6, 7)))
        );
    }

    #[test]
    fn bif_condition_lowers_builtin_variable_names_to_load_var() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let push7_id = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(push7_id))
            .expect("primitive word should register");

        let (sources, id, unit) = compile_with_bindings_and_operators(
            "BIF a + B, 100\n100 PUSH7",
            &bindings,
            operators.lookup(),
        );
        let view = sources.view();

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::LoadVar(variables[0]))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::LoadVar(variables[1]))
        );
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(view, id, 4, 5)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 1)),
            Ok(Some(span(view, id, 8, 9)))
        );
    }

    #[test]
    fn bif_condition_lowers_user_published_variable_name_to_load_var() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("VAR source word should bootstrap");
        compile_with_var("VAR Score", &mut bindings, &mut globals, &source_words);
        let Some(Binding::Variable(score)) = bindings.get(&name("score")).copied() else {
            panic!("SCORE should be a published variable");
        };
        let push7_id = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(push7_id))
            .expect("primitive word should register");

        let (sources, id, unit) = compile_with_bindings_and_operators(
            "BIF score = 0, 100\n100 PUSH7",
            &bindings,
            operators.lookup(),
        );

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::LoadVar(score))
        );
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(sources.view(), id, 4, 9)))
        );
    }

    #[test]
    fn definition_body_empty_slice_appends_no_return_or_publication() {
        let bindings = Bindings::new();
        let (_sources, _id, code) = compile_body("", DefinitionBodyCompileContext::new(&bindings));

        assert_eq!(code.len(), 0);
    }

    #[test]
    fn definition_body_lowers_existing_runtime_word_call_with_source_mapping() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let call_target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("TARGET"), Binding::Word(call_target))
            .expect("runtime word should register");

        let (sources, id, code) =
            compile_body("target", DefinitionBodyCompileContext::new(&bindings));

        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Call(call_target))
        );
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(address(0))),
            Ok(Some(span(sources.view(), id, 0, 6)))
        );
    }

    #[test]
    fn definition_body_dispatches_source_word_through_binding_capability() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");

        let (_sources, _id, code) = compile_body(
            "source_marker",
            DefinitionBodyCompileContext {
                bindings: &bindings,
                operators: None,
                source_words: Some(source_words.lookup()),
            },
        );

        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn definition_body_let_lowers_expression_to_published_builder() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");

        let (_sources, _id, code) = compile_body(
            "LET A = 1 + 2",
            DefinitionBodyCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        );

        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            code.instruction_view().get(address(1)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            code.instruction_view().get(address(2)),
            Ok(&Instruction::Call(
                operators.lookup().resolve(OperatorSemantic::Add)
            ))
        );
        assert_eq!(
            code.instruction_view().get(address(3)),
            Ok(&Instruction::StoreVar(variables[0]))
        );
    }

    #[test]
    fn definition_body_var_fails_without_publication_capability() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let original_globals_len = globals.len();

        let (_sources, _id, error) = compile_body_error(
            "VAR SCORE",
            DefinitionBodyCompileContext {
                bindings: &bindings,
                operators: None,
                source_words: Some(source_words.lookup()),
            },
        );

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarPublicationContextUnavailable)
        );
        assert_eq!(bindings.get(&name("SCORE")), None);
        assert_eq!(globals.len(), original_globals_len);
    }

    #[test]
    fn definition_body_rejects_bare_expression() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let bindings = Bindings::new();
        let (sources, id, error) = compile_body_error(
            "1 + 2",
            DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 1),
                kind: CompileErrorKind::BareExpression,
            })
        );
    }

    #[test]
    fn definition_body_patches_forward_and_backward_local_line_numbers() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");

        let (_sources, _id, code) = compile_body(
            "100 BIF 0, 200\nPUSH7\n200 BIF 0, 100",
            DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );

        assert_eq!(
            code.instruction_view().get(address(1)),
            Ok(&Instruction::JumpIfZero(address(3)))
        );
        assert_eq!(
            code.instruction_view().get(address(4)),
            Ok(&Instruction::JumpIfZero(address(0)))
        );
    }

    #[test]
    fn definition_body_rejects_duplicate_and_undefined_local_line_numbers() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");
        let (duplicate_sources, duplicate_id, duplicate) = compile_body_error(
            "100 BIF 1, 100\n100 PUSH7",
            DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );
        let (undefined_sources, undefined_id, undefined) = compile_body_error(
            "BIF 1, 200",
            DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );

        assert_eq!(
            duplicate,
            SourceProcessorError::Compile(CompileError {
                span: span(duplicate_sources.view(), duplicate_id, 15, 18),
                kind: CompileErrorKind::LineNumber {
                    source: Box::new(LineNumberError::Duplicate {
                        line_number: LocalLineNumber::new(100),
                        original_span: span(duplicate_sources.view(), duplicate_id, 0, 3),
                        duplicate_span: span(duplicate_sources.view(), duplicate_id, 15, 18),
                    }),
                },
            })
        );
        assert_eq!(
            undefined,
            SourceProcessorError::Compile(CompileError {
                span: span(undefined_sources.view(), undefined_id, 7, 10),
                kind: CompileErrorKind::LineNumber {
                    source: Box::new(LineNumberError::Undefined {
                        line_number: LocalLineNumber::new(200),
                        span: span(undefined_sources.view(), undefined_id, 7, 10),
                    }),
                },
            })
        );
    }

    #[test]
    fn definition_body_line_number_preanalysis_is_owner_local() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");
        let (first_sources, first_id, first_segmented) = segment("BIF 0, 100\n100 PUSH7");
        let (second_sources, second_id, second_segmented) = segment("100 PUSH7");
        let mut code = PublishedCode::new();

        compile_body_into(
            &mut code,
            first_sources.view(),
            first_id,
            &first_segmented,
            DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect("first body should compile");
        let second_start = code.len();
        compile_body_into(
            &mut code,
            second_sources.view(),
            second_id,
            &second_segmented,
            DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect("second body should compile independently");

        assert_eq!(
            code.instruction_view().get(address(second_start)),
            Ok(&Instruction::Call(target))
        );
    }

    #[test]
    fn definition_body_unpublished_self_or_later_word_is_undefined() {
        let bindings = Bindings::new();
        for source_text in ["SELF", "LATER"] {
            let (sources, id, error) =
                compile_body_error(source_text, DefinitionBodyCompileContext::new(&bindings));
            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, 0, source_text.len()),
                    kind: CompileErrorKind::WordResolution {
                        source: WordResolutionError::UndefinedName,
                    },
                }),
                "{source_text:?} should not receive a temporary body binding"
            );
        }
    }

    #[test]
    fn quotation_body_empty_slice_completes_empty_static_quotation() {
        let bindings = Bindings::new();
        let (_sources, _id, quotation) =
            compile_quotation("", QuotationBodyCompileContext::new(&bindings));

        assert_eq!(quotation.len(), 0);
    }

    #[test]
    fn quotation_body_lowers_existing_runtime_word_call_with_source_mapping() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let call_target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("TARGET"), Binding::Word(call_target))
            .expect("runtime word should register");

        let (sources, id, quotation) =
            compile_quotation("target", QuotationBodyCompileContext::new(&bindings));

        assert_eq!(
            quotation.instruction_view().get(address(0)),
            Ok(&Instruction::Call(call_target))
        );
        assert_eq!(
            quotation
                .source_mapping()
                .source_span(quotation_location(&quotation, 0)),
            Ok(Some(span(sources.view(), id, 0, 6)))
        );
    }

    #[test]
    fn quotation_body_dispatches_source_word_through_binding_capability() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");

        let (_sources, _id, quotation) = compile_quotation(
            "source_marker",
            QuotationBodyCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        );

        assert_eq!(
            quotation.instruction_view().get(address(0)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn quotation_body_let_lowers_without_publication_capability() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");

        let (_sources, _id, quotation) = compile_quotation(
            "LET A = 1 + 2",
            QuotationBodyCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        );

        assert_eq!(
            quotation.instruction_view().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            quotation.instruction_view().get(address(1)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            quotation.instruction_view().get(address(2)),
            Ok(&Instruction::Call(
                operators.lookup().resolve(OperatorSemantic::Add)
            ))
        );
        assert_eq!(
            quotation.instruction_view().get(address(3)),
            Ok(&Instruction::StoreVar(variables[0]))
        );
    }

    #[test]
    fn quotation_body_var_fails_without_binding_or_global_publication() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let original_bindings_len = bindings.len();
        let original_globals_len = globals.len();

        let (_sources, _id, error) = compile_quotation_error(
            "VAR SCORE",
            QuotationBodyCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        );

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarPublicationContextUnavailable)
        );
        assert_eq!(bindings.len(), original_bindings_len);
        assert_eq!(bindings.get(&name("SCORE")), None);
        assert_eq!(globals.len(), original_globals_len);
    }

    #[test]
    fn quotation_body_def_fails_without_runtime_definition_publication() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let words = PublishedWords::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let original_bindings_len = bindings.len();

        let (sources, id, error) = compile_quotation_error(
            "DEF F\nEND",
            QuotationBodyCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        );

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefPublicationContextUnavailable {
                span: span(sources.view(), id, 0, 3),
            })
        );
        assert_eq!(bindings.len(), original_bindings_len);
        assert_eq!(bindings.get(&name("F")), None);
        assert_eq!(words.len(), 0);
    }

    #[test]
    fn quotation_body_patches_forward_and_backward_local_line_numbers() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");

        let (_sources, _id, quotation) = compile_quotation(
            "100 BIF 0, 200\nPUSH7\n200 BIF 0, 100",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );

        assert_eq!(
            quotation.instruction_view().get(address(1)),
            Ok(&Instruction::JumpIfZero(address(3)))
        );
        assert_eq!(
            quotation.instruction_view().get(address(4)),
            Ok(&Instruction::JumpIfZero(address(0)))
        );
    }

    #[test]
    fn quotation_body_rejects_duplicate_and_undefined_local_line_numbers() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");
        let (duplicate_sources, duplicate_id, duplicate) = compile_quotation_error(
            "100 BIF 1, 100\n100 PUSH7",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );
        let (undefined_sources, undefined_id, undefined) = compile_quotation_error(
            "BIF 1, 200",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );

        assert_eq!(
            duplicate,
            SourceProcessorError::Compile(CompileError {
                span: span(duplicate_sources.view(), duplicate_id, 15, 18),
                kind: CompileErrorKind::LineNumber {
                    source: Box::new(LineNumberError::Duplicate {
                        line_number: LocalLineNumber::new(100),
                        original_span: span(duplicate_sources.view(), duplicate_id, 0, 3),
                        duplicate_span: span(duplicate_sources.view(), duplicate_id, 15, 18),
                    }),
                },
            })
        );
        assert_eq!(
            undefined,
            SourceProcessorError::Compile(CompileError {
                span: span(undefined_sources.view(), undefined_id, 7, 10),
                kind: CompileErrorKind::LineNumber {
                    source: Box::new(LineNumberError::Undefined {
                        line_number: LocalLineNumber::new(200),
                        span: span(undefined_sources.view(), undefined_id, 7, 10),
                    }),
                },
            })
        );
    }

    #[test]
    fn quotation_body_line_number_scope_is_independent_per_quotation() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");

        let (_first_sources, _first_id, first) = compile_quotation(
            "100 PUSH7\nBIF 0, 100",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );
        let (_second_sources, _second_id, second) = compile_quotation(
            "100 PUSH7\nBIF 0, 100",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );
        let (_undefined_sources, _undefined_id, undefined) = compile_quotation_error(
            "BIF 0, 100",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );

        assert_eq!(
            first.instruction_view().get(address(2)),
            Ok(&Instruction::JumpIfZero(address(0)))
        );
        assert_eq!(
            second.instruction_view().get(address(2)),
            Ok(&Instruction::JumpIfZero(address(0)))
        );
        assert!(matches!(
            undefined,
            SourceProcessorError::Compile(CompileError {
                kind: CompileErrorKind::LineNumber { .. },
                ..
            })
        ));
    }

    #[test]
    fn quotation_body_failure_returns_no_completed_artifact_and_next_build_can_succeed() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let target = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(target))
            .expect("runtime word should register");
        let (sources, id, error) = compile_quotation_error(
            "BIF 1, 200",
            QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
        );
        let (_next_sources, _next_id, next) =
            compile_quotation("PUSH7", QuotationBodyCompileContext::new(&bindings));

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 7, 10),
                kind: CompileErrorKind::LineNumber {
                    source: Box::new(LineNumberError::Undefined {
                        line_number: LocalLineNumber::new(200),
                        span: span(sources.view(), id, 7, 10),
                    }),
                },
            })
        );
        assert_eq!(next.len(), 1);
        assert_eq!(
            next.instruction_view().get(address(0)),
            Ok(&Instruction::Call(target))
        );
    }

    #[test]
    fn let_lowers_rhs_expression_then_store_var_with_source_mapping() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let (sources, id) = source("LET A = 1 + 2 * 3");

        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect("LET should compile");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Push(value(3)))
        );
        assert_eq!(
            unit.instructions().get(address(3)),
            Ok(&Instruction::Call(
                operators.lookup().resolve(OperatorSemantic::Multiply)
            ))
        );
        assert_eq!(
            unit.instructions().get(address(4)),
            Ok(&Instruction::Call(
                operators.lookup().resolve(OperatorSemantic::Add)
            ))
        );
        assert_eq!(
            unit.instructions().get(address(5)),
            Ok(&Instruction::StoreVar(variables[0]))
        );
        assert_eq!(unit.instructions().get(address(6)), Ok(&Instruction::Halt));
        assert!(!matches!(
            unit.instructions().get(address(0)),
            Ok(Instruction::Call(_))
        ));
        assert_eq!(
            unit.source_span(location(&unit, 5)),
            Ok(Some(span(sources.view(), id, 4, 5)))
        );
    }

    #[test]
    fn let_updates_builtin_and_user_variables_with_case_insensitive_resolution() {
        let (words, primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        compile_with_var("VAR Score", &mut bindings, &mut globals, &source_words);
        let Some(Binding::Variable(score)) = bindings.get(&name("score")).copied() else {
            panic!("SCORE should be a published variable");
        };

        globals
            .view_mut()
            .write(variables[1], value(4))
            .expect("B should be writable");
        run_with_source_words_operators_and_mut_globals(
            "let a = b + 2 * 3\nLET score = A",
            &bindings,
            &mut globals,
            &source_words,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(globals.view().read(variables[0]), Ok(value(10)));
        assert_eq!(globals.view().read(score), Ok(value(10)));
    }

    #[test]
    fn source_to_vm_e2e_updates_builtin_variables_through_formal_expression_context() {
        let (words, primitives, operators, source_words, bindings, mut globals, variables) =
            global_source_fixture();

        let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
            "LET A = 40 + 2\nLET B = A + 1",
            &bindings,
            &mut globals,
            &source_words,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), []);
        assert_eq!(globals.view().read(variables[0]), Ok(value(42)));
        assert_eq!(globals.view().read(variables[1]), Ok(value(43)));
    }

    #[test]
    fn source_to_vm_e2e_publishes_user_var_for_later_let_in_same_source() {
        let (words, primitives, operators, source_words, mut bindings, mut globals, variables) =
            global_source_fixture();
        let (sources, id) = source("VAR SCORE\nLET SCORE = SCORE + 1\nLET A = SCORE");

        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_word_publication_and_operators(
                &mut bindings,
                source_words.lookup(),
                operators.lookup(),
                &mut globals,
            ),
        )
        .expect("same-source VAR and LET statements should compile");
        let Some(Binding::Variable(score)) = bindings.get(&name("SCORE")).copied() else {
            panic!("SCORE should be published by the VAR statement");
        };

        let result = run_unit(
            &unit,
            SourceExecutionContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            )
            .with_mut_globals(globals.view_mut()),
        )
        .expect("same-source VAR and LET unit should run");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), []);
        assert_eq!(globals.view().read(score), Ok(value(1)));
        assert_eq!(globals.view().read(variables[0]), Ok(value(1)));
    }

    #[test]
    fn source_to_vm_e2e_successful_var_survives_later_statement_failure() {
        let (_words, _primitives, operators, source_words, mut bindings, mut globals, _variables) =
            global_source_fixture();
        let original_globals_len = globals.len();
        let (sources, id) = source("VAR SCORE\nLET A = MISSING + 1");

        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_word_publication_and_operators(
                &mut bindings,
                source_words.lookup(),
                operators.lookup(),
                &mut globals,
            ),
        )
        .expect_err("later LET RHS failure should fail the source");

        let SourceProcessorError::SourceWord(SourceWordError::Expression {
            source: ExpressionError::Variable(error),
        }) = error
        else {
            panic!("expected later LET RHS variable error");
        };
        assert_eq!(error.span(), span(sources.view(), id, 18, 25));
        assert_eq!(error.kind(), ExpressionVariableErrorKind::UndefinedName);
        let Some(Binding::Variable(score)) = bindings.get(&name("SCORE")).copied() else {
            panic!("completed VAR should remain published after later failure");
        };
        assert_eq!(globals.len(), original_globals_len + 1);
        assert_eq!(globals.view().read(score), Ok(value(0)));
    }

    #[test]
    fn source_to_vm_e2e_failed_var_does_not_publish_binding_or_storage() {
        for source_text in ["VAR SCORE EXTRA", "VAR A"] {
            let (
                _words,
                _primitives,
                operators,
                source_words,
                mut bindings,
                mut globals,
                _variables,
            ) = global_source_fixture();
            let original_globals_len = globals.len();
            let (sources, id) = source(source_text);

            let error = compile_source(
                sources.view(),
                id,
                SourceCompileContext::with_source_word_publication_and_operators(
                    &mut bindings,
                    source_words.lookup(),
                    operators.lookup(),
                    &mut globals,
                ),
            )
            .expect_err("failed VAR statement should reject the source");

            match source_text {
                "VAR SCORE EXTRA" => {
                    assert_eq!(
                        error,
                        SourceProcessorError::SourceWord(SourceWordError::VarSyntax {
                            span: span(sources.view(), id, 10, 15),
                            kind: VarSyntaxErrorKind::TrailingToken {
                                kind: TokenKind::Name,
                            },
                        })
                    );
                    assert_eq!(bindings.get(&name("SCORE")), None);
                }
                "VAR A" => {
                    assert_eq!(
                        error,
                        SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                            span: span(sources.view(), id, 4, 5),
                        })
                    );
                    assert!(matches!(
                        bindings.get(&name("A")),
                        Some(Binding::Variable(_))
                    ));
                }
                _ => unreachable!(),
            }
            assert_eq!(
                globals.len(),
                original_globals_len,
                "{source_text:?} should not allocate storage"
            );
        }
    }

    #[test]
    fn source_to_vm_e2e_reuses_global_storage_across_fresh_executions_only() {
        let (
            mut words,
            mut primitives,
            operators,
            source_words,
            mut bindings,
            mut globals,
            variables,
        ) = global_source_fixture();
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("runtime word should register");

        let (_sources, _id, first_result) = run_with_source_words_operators_and_mut_globals(
            "PUSH7\nLET A = 10",
            &bindings,
            &mut globals,
            &source_words,
            &words,
            &primitives,
            operators.lookup(),
        );
        assert_eq!(first_result.data_stack(), [value(7)]);
        assert_eq!(globals.view().read(variables[0]), Ok(value(10)));

        let (_sources, _id, second_result) = run_with_source_words_operators_and_mut_globals(
            "LET B = A + 1",
            &bindings,
            &mut globals,
            &source_words,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(second_result.outcome(), RunOutcome::Halted);
        assert_eq!(second_result.data_stack(), []);
        assert_eq!(globals.view().read(variables[0]), Ok(value(10)));
        assert_eq!(globals.view().read(variables[1]), Ok(value(11)));
    }

    #[test]
    fn source_to_vm_e2e_rhs_runtime_failure_maps_operator_and_preserves_target() {
        let (words, primitives, operators, source_words, bindings, mut globals, variables) =
            global_source_fixture();
        let (sources, id) = source("LET A = 7\nLET A = 32767 + 1");

        let error = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            )
            .with_mut_globals(globals.view_mut()),
        )
        .expect_err("checked arithmetic overflow should fail before StoreVar");

        let SourceProcessorError::Runtime(error) = error else {
            panic!("expected runtime error");
        };
        assert_eq!(
            error.source_span(),
            Ok(Some(span(sources.view(), id, 24, 25)))
        );
        assert!(matches!(
            error.vm().kind(),
            crate::vm::VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::Failed,
                ..
            }
        ));
        assert_eq!(globals.view().read(variables[0]), Ok(value(7)));
    }

    #[test]
    fn let_rhs_variable_load_mapping_uses_reference_name_span() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let (sources, id) = source("LET A = B + 1");

        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect("LET should compile");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::LoadVar(variables[1]))
        );
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(sources.view(), id, 8, 9)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 3)),
            Ok(Some(span(sources.view(), id, 4, 5)))
        );
    }

    #[test]
    fn let_rejects_target_resolution_and_syntax_errors_at_primary_span() {
        let mut source_words = SourceWordRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("runtime word should register");

        let (sources, id) = source("LET MISSING = 1");
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect_err("LET unresolved target should fail");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::LetTarget {
                span: span(sources.view(), id, 4, 11),
                source: ExpressionVariableErrorKind::UndefinedName,
            })
        );

        for (source_text, start, end, source_kind) in [
            (
                "LET PUSH7 = 1",
                4,
                9,
                ExpressionVariableErrorKind::TargetIsNotVariable,
            ),
            (
                "LET VAR = 1",
                4,
                7,
                ExpressionVariableErrorKind::TargetIsNotVariable,
            ),
        ] {
            let (sources, id) = source(source_text);
            let error = compile_source(
                sources.view(),
                id,
                SourceCompileContext::with_source_words_and_operators(
                    &bindings,
                    source_words.lookup(),
                    operators.lookup(),
                ),
            )
            .expect_err("LET non-variable target should fail");

            assert_eq!(
                error,
                SourceProcessorError::SourceWord(SourceWordError::LetTarget {
                    span: span(sources.view(), id, start, end),
                    source: source_kind,
                }),
                "{source_text:?} should reject non-variable target"
            );
        }

        for (source_text, start, end, kind) in [
            ("LET", 0, 3, LetSyntaxErrorKind::Target),
            ("LET 123 = 1", 4, 7, LetSyntaxErrorKind::Target),
            ("LET A 1", 6, 7, LetSyntaxErrorKind::Equal),
            ("LET A =", 6, 7, LetSyntaxErrorKind::Rhs),
        ] {
            let (sources, id) = source(source_text);
            let error = compile_source(
                sources.view(),
                id,
                SourceCompileContext::with_source_words_and_operators(
                    &bindings,
                    source_words.lookup(),
                    operators.lookup(),
                ),
            )
            .expect_err("LET syntax should fail");

            assert_eq!(
                error,
                SourceProcessorError::SourceWord(SourceWordError::LetSyntax {
                    span: span(sources.view(), id, start, end),
                    kind,
                }),
                "{source_text:?} should report LET syntax error"
            );
        }
    }

    #[test]
    fn failed_let_expression_does_not_compile_prior_rhs_instructions() {
        let (_words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let (sources, id) = source("LET A = B + MISSING");

        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect_err("later unresolved RHS name should fail LET");

        let SourceProcessorError::SourceWord(SourceWordError::Expression {
            source: ExpressionError::Variable(error),
        }) = error
        else {
            panic!("expected LET RHS variable resolution error");
        };
        assert_eq!(error.span(), span(sources.view(), id, 12, 19));
        assert_eq!(error.kind(), ExpressionVariableErrorKind::UndefinedName);
        assert_eq!(globals.view().read(variables[0]), Ok(value(0)));
        assert_eq!(globals.view().read(variables[1]), Ok(value(0)));
    }

    #[test]
    fn line_number_prefixed_let_jumps_to_rhs_start_and_runs_store_var() {
        let (words, primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");

        globals
            .view_mut()
            .write(variables[0], value(5))
            .expect("A should be writable");
        run_with_source_words_operators_and_mut_globals(
            "BIF 0, 100\nLET A = 1\n100 LET A = A + 1",
            &bindings,
            &mut globals,
            &source_words,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(globals.view().read(variables[0]), Ok(value(6)));
    }

    #[test]
    fn bif_condition_variable_reads_from_global_storage_at_runtime() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut globals = GlobalVariables::new();
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        globals
            .view_mut()
            .write(variables[0], value(0))
            .expect("A should be writable");
        let (_sources, _id, zero_result) = run_with_bindings_operators_and_globals(
            "BIF A, 100\nPUSH7\n100 PUSH7",
            &bindings,
            &globals,
            &words,
            &primitives,
            operators.lookup(),
        );
        assert_eq!(zero_result.data_stack(), [value(7)]);

        globals
            .view_mut()
            .write(variables[0], value(1))
            .expect("A should be writable");
        let (_sources, _id, nonzero_result) = run_with_bindings_operators_and_globals(
            "BIF a, 100\nPUSH7\n100 PUSH7",
            &bindings,
            &globals,
            &words,
            &primitives,
            operators.lookup(),
        );
        assert_eq!(nonzero_result.data_stack(), [value(7), value(7)]);
    }

    #[test]
    fn bif_load_var_runtime_failure_maps_to_name_span() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut globals = GlobalVariables::new();
        let variables = register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap");
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");
        let (sources, id) = source("BIF A, 100\n100 PUSH7");

        let error = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_operators(
                &bindings,
                operators.lookup(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("LoadVar without execution globals should fail at runtime");

        let SourceProcessorError::Runtime(runtime) = error else {
            panic!("expected runtime error");
        };
        assert_eq!(
            runtime.source_span(),
            Ok(Some(span(sources.view(), id, 4, 5)))
        );
        assert_eq!(runtime.vm().address(), address(0));
        assert!(matches!(
            runtime.vm().kind(),
            crate::vm::VmErrorKind::InvalidGlobalVarId {
                source: crate::global_variable::GlobalVariableError::InvalidGlobalVarId { id }
            } if id == variables[0]
        ));
    }

    #[test]
    fn bif_name_primary_resolution_failures_are_compile_errors_at_name_span() {
        let mut source_words = SourceWordRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("source words should register");

        let cases = [
            (
                "BIF MISSING, 100\n100 PUSH7",
                4,
                11,
                ExpressionVariableErrorKind::UndefinedName,
            ),
            (
                "BIF PUSH7, 100\n100 PUSH7",
                4,
                9,
                ExpressionVariableErrorKind::TargetIsNotVariable,
            ),
            (
                "BIF VAR, 100\n100 PUSH7",
                4,
                7,
                ExpressionVariableErrorKind::TargetIsNotVariable,
            ),
        ];

        for (source_text, start, end, source_kind) in cases {
            let (sources, id) = source(source_text);
            let error = compile_source(
                sources.view(),
                id,
                SourceCompileContext::with_source_words_and_operators(
                    &bindings,
                    source_words.lookup(),
                    operators.lookup(),
                ),
            )
            .expect_err("non-variable expression name should fail");

            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, start, end),
                    kind: CompileErrorKind::ExpressionVariable {
                        source: source_kind
                    },
                }),
                "{source_text:?} should reject expression name without fallback"
            );
        }
    }

    #[test]
    fn bif_expression_name_resolution_failure_does_not_partially_commit() {
        let (_operator_words, _operator_primitives, operators) = operator_fixture();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        let a = globals.allocate();
        bindings
            .insert_new(name("A"), Binding::Variable(a))
            .expect("variable should register");
        let (sources, id) = source("A + MISSING");
        let mut lexer = Lexer::new(sources.view(), id).expect("lexer should build");
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token().expect("source should lex");
            if token.kind() == TokenKind::Eof {
                break;
            }
            tokens.push(token);
        }
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);

        let error = compile_expression_tokens(
            sources.view(),
            id,
            &tokens,
            &bindings,
            operators.lookup(),
            &mut builder,
        )
        .expect_err("later unresolved name should fail the expression");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 4, 11),
                kind: CompileErrorKind::ExpressionVariable {
                    source: ExpressionVariableErrorKind::UndefinedName
                },
            })
        );
        assert_eq!(code.len(), 0);
        assert_eq!(code.source_mapping().len(), 0);
    }

    #[test]
    fn bif_expression_arithmetic_failure_maps_runtime_error_to_operator_span() {
        let (sources, id) = source("BIF 1 / 0, 100\n100 PUSH7");
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let push7_id = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
            .expect("primitive should register");

        let error = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_operators(
                &bindings,
                operators.lookup(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("division by zero should fail at runtime");

        let SourceProcessorError::Runtime(error) = error else {
            panic!("expected runtime error");
        };
        assert_eq!(
            error.source_span(),
            Ok(Some(span(sources.view(), id, 6, 7)))
        );
        assert_eq!(error.vm().address(), address(2));
    }

    #[test]
    fn malformed_bif_expression_is_span_compile_error_without_runtime_start() {
        let (sources, id, error) = compile_with_operators_error("BIF 1 +, 100\n100");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 7, 7),
                kind: CompileErrorKind::Expression {
                    source: ExpressionSyntaxErrorKind::MissingOperand,
                },
            })
        );
    }

    #[test]
    fn bif_rejects_missing_comma_target_and_trailing_tokens_as_compile_errors() {
        let cases = [
            ("BIF 0 200", 0, 3, BifSyntaxErrorKind::MissingComma),
            ("BIF 0,", 5, 6, BifSyntaxErrorKind::MissingTarget),
            (
                "BIF 0, 200 300",
                11,
                14,
                BifSyntaxErrorKind::TrailingToken {
                    kind: TokenKind::IntegerLiteral,
                },
            ),
        ];

        for (source, start, end, source_kind) in cases {
            let (sources, id, error) = compile_with_operators_error(source);
            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, start, end),
                    kind: CompileErrorKind::BifSyntax {
                        source: source_kind
                    },
                }),
                "{source:?} should fail as malformed BIF"
            );
        }
    }

    #[test]
    fn bif_rejects_missing_condition_as_compile_error() {
        let (sources, id, error) = compile_with_operators_error("BIF , 200");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 3),
                kind: CompileErrorKind::BifSyntax {
                    source: BifSyntaxErrorKind::MissingCondition
                },
            })
        );
    }

    #[test]
    fn undefined_bif_line_number_is_compile_error_at_target_operand() {
        let (sources, id, error) = compile_with_operators_error("BIF 0, 200");
        let SourceProcessorError::Compile(error) = error else {
            panic!("expected compile error");
        };

        assert_eq!(error.span(), span(sources.view(), id, 7, 10));
        assert_eq!(
            error.kind(),
            CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Undefined {
                    line_number: LocalLineNumber::new(200),
                    span: span(sources.view(), id, 7, 10),
                })
            }
        );
    }

    #[test]
    fn duplicate_line_number_is_compile_error_at_duplicate_span() {
        let (sources, id, error) =
            compile_with_operators_error("100 BIF 1, 200\n100 BIF 1, 200\n200 BIF 1, 200");
        let SourceProcessorError::Compile(error) = error else {
            panic!("expected compile error");
        };

        assert_eq!(error.span(), span(sources.view(), id, 15, 18));
        assert_eq!(
            error.kind(),
            CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Duplicate {
                    line_number: LocalLineNumber::new(100),
                    original_span: span(sources.view(), id, 0, 3),
                    duplicate_span: span(sources.view(), id, 15, 18),
                })
            }
        );
    }

    #[test]
    fn colon_prefixed_line_number_syntax_is_not_accepted_as_local_line_number() {
        let (sources, id, error) = compile_with_operators_error("100: BIF 0, 100");

        assert_eq!(
            error,
            SourceProcessorError::Lex(LexError::InvalidCharacter {
                span: span(sources.view(), id, 3, 4),
                character: ':',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn run_leaves_data_stack_snapshot_in_source_order() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let push1 = primitives.register(push_1);
        let push2 = primitives.register(push_2);
        let push3 = primitives.register(push_3);
        register_primitive(&mut words, &mut bindings, name("PUSH1"), push1)
            .expect("primitive should register");
        register_primitive(&mut words, &mut bindings, name("PUSH2"), push2)
            .expect("primitive should register");
        register_primitive(&mut words, &mut bindings, name("PUSH3"), push3)
            .expect("primitive should register");
        let (sources, id) = source("PUSH1\nPUSH2\r\nPUSH3");
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

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(1), value(2), value(3)]);
        assert_eq!(result.instruction_count(), 4);
    }

    #[test]
    fn each_run_uses_fresh_vm_state() {
        let (mut sources, first) = source("PUSH1 PUSH2");
        let second = sources.register("");
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let push1 = primitives.register(push_1);
        let push2 = primitives.register(push_2);
        register_primitive(&mut words, &mut bindings, name("PUSH1"), push1)
            .expect("primitive should register");
        register_primitive(&mut words, &mut bindings, name("PUSH2"), push2)
            .expect("primitive should register");
        let first_result = run_source(
            sources.view(),
            first,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("first source should run");
        let second_result = run_source(
            sources.view(),
            second,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("second source should run");

        assert_eq!(first_result.data_stack(), [value(1), value(2)]);
        assert_eq!(second_result.data_stack(), []);
        assert_eq!(first_result.outcome(), RunOutcome::Halted);
        assert_eq!(second_result.outcome(), RunOutcome::Halted);
    }

    #[test]
    fn standalone_out_of_range_integer_is_rejected_as_statement() {
        for source in ["32768", "999999999999999999999999999999"] {
            let (sources, id, error) = compile_error(source);

            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, 0, source.len()),
                    kind: CompileErrorKind::BareExpression,
                }),
                "{source:?} should reject standalone integer statement"
            );
        }
    }

    #[test]
    fn unsupported_name_and_minus_are_compile_errors_with_spans() {
        let (sources, id, name_error) = compile_error("RUN");
        assert_eq!(
            name_error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 3),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName
                },
            })
        );

        let (sources, id, minus_error) = compile_error("-1");
        assert_eq!(
            minus_error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 1),
                kind: CompileErrorKind::BareExpression,
            })
        );
    }

    #[test]
    fn lexer_errors_are_not_reclassified_as_compile_errors() {
        let (sources, id, error) = compile_error("@");

        assert_eq!(
            error,
            SourceProcessorError::Lex(LexError::InvalidCharacter {
                span: span(sources.view(), id, 0, 1),
                character: '@',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn completed_statement_compile_error_takes_precedence_over_later_lexical_error() {
        let (sources, id, error) = compile_error("MISSING\n@");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 7),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName
                },
            })
        );
    }

    #[test]
    fn successful_completed_statements_do_not_publish_partial_unit_before_lexical_error() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let known = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("KNOWN"), known)
            .expect("primitive should register");
        let (sources, id) = source("KNOWN\n@");

        let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect_err("lexical failure should prevent partial unit publication");

        assert_eq!(
            error,
            SourceProcessorError::Lex(LexError::InvalidCharacter {
                span: span(sources.view(), id, 6, 7),
                character: '@',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn incomplete_tail_is_not_preanalyzed_or_compiled_before_lexical_error() {
        let (sources, id, error) = compile_error("100 @");

        assert_eq!(
            error,
            SourceProcessorError::Lex(LexError::InvalidCharacter {
                span: span(sources.view(), id, 4, 5),
                character: '@',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn invalid_source_id_is_reported_at_source_boundary() {
        let (sources, valid) = source("1");
        let invalid = valid.test_next_slot();
        let words = PublishedWords::new();
        let bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();

        assert_eq!(
            compile_source(
                sources.view(),
                invalid,
                SourceCompileContext::new(&bindings)
            )
            .expect_err("invalid source should fail"),
            SourceProcessorError::Lex(LexError::Source(SourceError::InvalidSourceId {
                id: invalid
            }))
        );
        assert_eq!(
            run_source(
                sources.view(),
                invalid,
                SourceExecutionContext::new(
                    &bindings,
                    PublishedWordLookup::new(&words),
                    primitives.lookup()
                )
            )
            .expect_err("invalid source should fail"),
            SourceProcessorError::Lex(LexError::Source(SourceError::InvalidSourceId {
                id: invalid
            }))
        );
    }

    #[test]
    fn mapping_matches_instruction_addresses_in_order() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        publish_initial(&mut words, &mut bindings, "PUSH1", completed_primitive(1));
        publish_initial(&mut words, &mut bindings, "PUSH2", completed_primitive(2));
        let (_sources, _id, unit) = compile_with_bindings("PUSH1 PUSH2", &bindings);

        assert_eq!(
            unit.source_mapping().code_space(),
            unit.instructions().code_space()
        );
        assert_eq!(unit.source_mapping().len(), unit.len());
        assert_eq!(
            (0..unit.len())
                .map(|index| unit.source_span(location(&unit, index)).is_ok())
                .collect::<Vec<_>>(),
            [true, true, true]
        );
    }

    #[test]
    fn temporary_mapping_location_uses_unit_code_space_identity() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        publish_initial(&mut words, &mut bindings, "PUSH1", completed_primitive(1));
        publish_initial(&mut words, &mut bindings, "PUSH2", completed_primitive(2));
        let (first_sources, first_source_id, first_unit) =
            compile_with_bindings("PUSH1", &bindings);
        let (second_sources, second_source_id, second_unit) =
            compile_with_bindings("PUSH2", &bindings);
        let first_span = span(first_sources.view(), first_source_id, 0, 5);
        let second_span = span(second_sources.view(), second_source_id, 0, 5);
        let mapping_views = [first_unit.source_mapping(), second_unit.source_mapping()];
        let lookup = SourceMappingLookup::new(&mapping_views).expect("unit mappings are distinct");

        assert_eq!(
            first_unit.source_mapping().code_space(),
            first_unit.instructions().code_space()
        );
        assert_eq!(
            first_unit
                .instructions()
                .location(address(0))
                .address()
                .as_index(),
            second_unit
                .instructions()
                .location(address(0))
                .address()
                .as_index()
        );
        assert_ne!(
            first_unit.instructions().code_space(),
            second_unit.instructions().code_space()
        );
        assert_eq!(
            lookup.source_span(first_unit.instructions().location(address(0))),
            Ok(Some(first_span))
        );
        assert_eq!(
            lookup.source_span(second_unit.instructions().location(address(0))),
            Ok(Some(second_span))
        );
    }

    #[test]
    fn temporary_mapping_rejects_other_code_space_without_index_fallback() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        publish_initial(&mut words, &mut bindings, "PUSH1", completed_primitive(1));
        let (_sources, _source_id, unit) = compile_with_bindings("PUSH1", &bindings);
        let mut other_code = InstructionSequence::new();
        let other_address = other_code.append(Instruction::Halt);

        assert_eq!(
            unit.source_span(other_code.view().location(other_address)),
            Err(SourceMappingLookupError::Address {
                source: crate::instruction::InstructionAddressError::CodeSpaceMismatch {
                    expected: unit.source_mapping().code_space(),
                    actual: other_code.code_space(),
                    address: other_address,
                }
            })
        );
    }

    #[test]
    fn names_compile_to_call_in_source_order_with_source_spans() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut shared_code = InstructionSequence::new();
        let first = publish_initial(&mut words, &mut bindings, "ALPHA", completed_primitive(1));
        let second = publish_initial(
            &mut words,
            &mut bindings,
            "BETA?",
            completed_compiled(&mut shared_code, 9),
        );

        let (sources, id, unit) = compile_with_bindings("alpha 12 beta?", &bindings);
        let view = sources.view();

        assert_eq!(unit.entry(), address(0));
        assert_eq!(unit.len(), 4);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Call(first))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(12)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Call(second))
        );
        assert_eq!(unit.instructions().get(address(3)), Ok(&Instruction::Halt));
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(view, id, 0, 5)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 1)),
            Ok(Some(span(view, id, 6, 8)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 2)),
            Ok(Some(span(view, id, 9, 14)))
        );
    }

    #[test]
    fn statement_leading_source_word_dispatches_through_binding_resolution() {
        let mut source_words = SourceWordRegistry::new();
        let words = PublishedWords::new();
        let primitives = PrimitiveRegistry::new();
        let mut bindings = Bindings::new();
        let source_word = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");
        let (sources, source_id) = source("source_marker");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("source word should compile");

        assert_eq!(words.len(), 0);
        assert_eq!(source_word.as_slot(), 0);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(99)))
        );
        assert_eq!(unit.instructions().get(address(1)), Ok(&Instruction::Halt));

        let result = run_unit(
            &unit,
            SourceExecutionContext::with_source_words(
                &bindings,
                source_words.lookup(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("source word unit should run");
        assert_eq!(result.data_stack(), [value(99)]);
    }

    #[test]
    fn source_word_case_variants_dispatch_to_same_handler() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");

        let (_sources, _source_id, unit) = {
            let (sources, source_id) = source("source_marker\nSource_Marker\nSOURCE_MARKER");
            let unit = compile_source(
                sources.view(),
                source_id,
                SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
            )
            .expect("source word case variants should compile");
            (sources, source_id, unit)
        };

        assert_eq!(unit.len(), 4);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(99)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(99)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn block_reader_consumes_one_statement_without_outer_reprocessing() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            consume_one_following_statement,
        )
        .expect("block source word should register");
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("marker source word should register");
        let (sources, source_id) = source("BLOCK\nMISSING\nSOURCE_MARKER");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("consumed unresolved statement should not be reprocessed by outer loop");

        assert_eq!(unit.len(), 3);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(99)))
        );
        assert_eq!(unit.instructions().get(address(2)), Ok(&Instruction::Halt));
    }

    #[test]
    fn block_reader_consumes_multiple_statements_without_duplicates() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("BLOCK2"),
            consume_two_following_statements,
        )
        .expect("block source word should register");
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("marker source word should register");
        let (sources, source_id) = source("BLOCK2\nFIRST\nSECOND\nSOURCE_MARKER");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("both consumed statements should be skipped by outer loop");

        assert_eq!(unit.len(), 4);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn block_reader_exposes_whole_statement_for_standalone_marker_detection() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            consume_standalone_marker,
        )
        .expect("block source word should register");
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("marker source word should register");
        let (sources, source_id) = source("BLOCK\nEND\nSOURCE_MARKER");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("standalone marker should be consumed by block reader");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(0)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn block_reader_keeps_multi_token_marker_candidate_as_statement() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            consume_standalone_marker,
        )
        .expect("block source word should register");
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("marker source word should register");
        let (sources, source_id) = source("BLOCK\nEND X\nSOURCE_MARKER");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("multi-token marker candidate should be available to the handler");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn block_reader_recognizes_owner_declared_marker_roles() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            classify_one_declared_block_item,
            vec![marker(
                "ELSE",
                SourceWordSyntaxMarkerRole::BlockContinuation,
            )],
        )
        .expect("block source word should register with marker");
        let (sources, source_id) = source("BLOCK\nelse");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("declared marker should be classified");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(10)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(sources.view(), source_id, 6, 10)))
        );
    }

    #[test]
    fn block_reader_does_not_treat_undeclared_name_as_marker() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            classify_one_declared_block_item,
            vec![marker("ENDIF", SourceWordSyntaxMarkerRole::BlockTerminator)],
        )
        .expect("block source word should register with marker");
        let (sources, source_id) = source("BLOCK\nELSE");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("undeclared marker spelling should remain a statement");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
    }

    #[test]
    fn block_reader_keeps_other_owner_marker_as_statement() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("OUTER"),
            consume_until_declared_terminator,
            vec![marker(
                "OUT_END",
                SourceWordSyntaxMarkerRole::BlockTerminator,
            )],
        )
        .expect("outer source word should register with marker");
        register_native_source_word_with_markers(
            &mut source_words,
            &mut bindings,
            name("INNER"),
            classify_one_declared_block_item,
            vec![marker(
                "INNER_END",
                SourceWordSyntaxMarkerRole::BlockTerminator,
            )],
        )
        .expect("inner source word should register with a distinct marker");
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");
        let (sources, source_id) = source("OUTER\nINNER_END\nOUT_END\nSOURCE_MARKER");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("outer reader should classify only its own markers");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(20)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn block_reader_reports_eof_span_for_missing_terminator_errors() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            read_eof_terminal_as_missing_terminator,
        )
        .expect("block source word should register");
        let (sources, source_id) = source("BLOCK");

        let error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect_err("missing block terminator should use EOF terminal span");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::UnsupportedSourceWord {
                span: span(sources.view(), source_id, 5, 5)
            })
        );
    }

    #[test]
    fn block_reader_does_not_convert_lexical_failure_terminal_to_eof() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("BLOCK"),
            observe_lex_terminal_without_converting_to_eof,
        )
        .expect("block source word should register");
        let (sources, source_id) = source("BLOCK\n@");

        let error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect_err("lexical failure should remain the source terminal error");

        assert!(matches!(
            error,
            SourceProcessorError::Lex(LexError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn nested_block_reader_fixture_advances_the_shared_cursor_once() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("NEST"),
            nested_reader_fixture,
        )
        .expect("nested source word should register");
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("marker source word should register");
        let (sources, source_id) = source("NEST\nINNER\nOUTER\nSOURCE_MARKER");

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("inner and outer reader use should share one cursor");

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(99)))
        );
    }

    #[test]
    fn source_word_binding_does_not_lower_to_runtime_call() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");

        let (_sources, _source_id, unit) = {
            let (sources, source_id) = source("source_marker");
            let unit = compile_source(
                sources.view(),
                source_id,
                SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
            )
            .expect("source word should compile");
            (sources, source_id, unit)
        };

        assert!(!matches!(
            unit.instructions().get(address(0)),
            Ok(Instruction::Call(_))
        ));
    }

    #[test]
    fn source_word_binding_without_lookup_is_internal_context_error() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let source_word = register_native_source_word(
            &mut source_words,
            &mut bindings,
            name("SOURCE_MARKER"),
            emit_source_word_marker,
        )
        .expect("source word should register");
        let (sources, source_id) = source("source_marker");

        let error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::new(&bindings),
        )
        .expect_err("source word binding without lookup should fail as context error");

        assert_eq!(
            error,
            SourceProcessorError::SourceWordContextUnavailable { id: source_word }
        );
    }

    #[test]
    fn variable_and_unresolved_leading_names_are_not_source_word_dispatch() {
        let mut globals = crate::global_variable::GlobalVariables::new();
        let variable = globals.allocate();
        let mut bindings = Bindings::new();
        bindings
            .insert_new(name("A"), Binding::Variable(variable))
            .expect("variable should register");

        let (sources, source_id) = source("A");
        let variable_error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::new(&bindings),
        )
        .expect_err("variable should not dispatch as source word");
        assert_eq!(
            variable_error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), source_id, 0, 1),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::TargetIsNotWord
                },
            })
        );

        let (sources, source_id) = source("MISSING");
        let unresolved_error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::new(&bindings),
        )
        .expect_err("unresolved name should not dispatch as source word");
        assert_eq!(
            unresolved_error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), source_id, 0, 7),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName
                },
            })
        );
    }

    #[test]
    fn var_declares_global_variable_through_source_word_binding_without_runtime_instruction() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        let builtin = register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");

        let (_sources, _id, unit) =
            compile_with_var("VAR SCORE", &mut bindings, &mut globals, &source_words);

        let Some(Binding::Variable(id)) = bindings.get(&name("score")).copied() else {
            panic!("SCORE should be published as a variable");
        };
        assert_eq!(
            bindings.get(&name("VAR")),
            Some(&Binding::SourceWord(builtin.var()))
        );
        assert_eq!(globals.view().read(id), Ok(value(0)));
        assert_eq!(globals.len(), 1);
        assert_eq!(unit.len(), 1);
        assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
    }

    #[test]
    fn var_uses_normalized_name_identity_for_mixed_case_declarations() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");

        compile_with_var("vAr Mixed_Case", &mut bindings, &mut globals, &source_words);

        let binding = bindings
            .get(&name("MIXED_CASE"))
            .copied()
            .expect("mixed-case declaration should publish");
        assert_eq!(bindings.get(&name("mixed_case")), Some(&binding));
        assert!(matches!(binding, Binding::Variable(_)));
    }

    #[test]
    fn var_rejects_malformed_statements_with_primary_source_word_error_span() {
        for source_text in ["VAR", "VAR 123", "VAR SCORE EXTRA"] {
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();
            let mut globals = GlobalVariables::new();
            register_builtin_source_words(&mut source_words, &mut bindings)
                .expect("built-in source words should bootstrap");

            let (sources, id, error) =
                compile_with_var_error(source_text, &mut bindings, &mut globals, &source_words);

            let expected = match source_text {
                "VAR" => SourceWordError::VarSyntax {
                    span: span(sources.view(), id, 0, 3),
                    kind: VarSyntaxErrorKind::MissingName,
                },
                "VAR 123" => SourceWordError::VarSyntax {
                    span: span(sources.view(), id, 4, 7),
                    kind: VarSyntaxErrorKind::MissingName,
                },
                "VAR SCORE EXTRA" => SourceWordError::VarSyntax {
                    span: span(sources.view(), id, 10, 15),
                    kind: VarSyntaxErrorKind::TrailingToken {
                        kind: TokenKind::Name,
                    },
                },
                _ => unreachable!(),
            };
            assert_eq!(error, SourceProcessorError::SourceWord(expected));
            assert_eq!(globals.len(), 0, "{source_text:?} should not allocate");
            assert_eq!(bindings.get(&name("SCORE")), None);
        }
    }

    #[test]
    fn var_rejects_duplicate_and_cross_kind_name_collisions_at_declared_name_span() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        compile_with_var("VAR SCORE", &mut bindings, &mut globals, &source_words);

        let (sources, id, error) =
            compile_with_var_error("VAR score", &mut bindings, &mut globals, &source_words);

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                span: span(sources.view(), id, 4, 9)
            })
        );
        assert_eq!(globals.len(), 1);

        let mut words = PublishedWords::new();
        let mut runtime_bindings = Bindings::new();
        let mut runtime_globals = GlobalVariables::new();
        let primitive = PrimitiveId::from_slot(90);
        register_primitive(
            &mut words,
            &mut runtime_bindings,
            name("RUNTIME"),
            primitive,
        )
        .expect("runtime word should register");
        register_builtin_source_words(&mut source_words, &mut runtime_bindings)
            .expect("VAR should register after runtime word");
        let (sources, id, error) = compile_with_var_error(
            "VAR RUNTIME",
            &mut runtime_bindings,
            &mut runtime_globals,
            &source_words,
        );
        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                span: span(sources.view(), id, 4, 11)
            })
        );
        assert_eq!(runtime_globals.len(), 0);
    }

    #[test]
    fn var_rejects_existing_source_word_and_builtin_variable_collisions_normally() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("VAR source word should bootstrap");
        register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("A-Z variables should bootstrap after VAR/LET");

        for (source_text, start, end) in [("VAR VAR", 4, 7), ("VAR let", 4, 7), ("VAR A", 4, 5)] {
            let (sources, id, error) =
                compile_with_var_error(source_text, &mut bindings, &mut globals, &source_words);
            assert_eq!(
                error,
                SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                    span: span(sources.view(), id, start, end)
                }),
                "{source_text:?} should be an ordinary binding collision"
            );
        }
        assert!(bindings.get(&name("VAR")).is_some());
        assert!(bindings.get(&name("LET")).is_some());
        assert!(bindings.get(&name("A")).is_some());
    }

    #[test]
    fn line_number_prefixed_var_is_rejected_before_publication() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("VAR source word should bootstrap");

        let (sources, id, error) =
            compile_with_var_error("10 VAR SCORE", &mut bindings, &mut globals, &source_words);

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarLocalLineNumberPrefix {
                span: span(sources.view(), id, 0, 2)
            })
        );
        assert_eq!(globals.len(), 0);
        assert_eq!(bindings.get(&name("SCORE")), None);
    }

    #[test]
    fn successful_var_commit_survives_later_statement_and_completed_lexical_failures() {
        for source_text in ["VAR SCORE\nMISSING", "VAR SCORE\n@"] {
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();
            let mut globals = GlobalVariables::new();
            register_builtin_source_words(&mut source_words, &mut bindings)
                .expect("VAR source word should bootstrap");

            let (_sources, _id, error) =
                compile_with_var_error(source_text, &mut bindings, &mut globals, &source_words);

            assert!(
                matches!(
                    error,
                    SourceProcessorError::Compile(_) | SourceProcessorError::Lex(_)
                ),
                "{source_text:?} should fail after the completed VAR statement"
            );
            assert!(matches!(
                bindings.get(&name("SCORE")),
                Some(Binding::Variable(_))
            ));
            assert_eq!(globals.len(), 1);
        }
    }

    #[test]
    fn same_statement_lexical_failure_does_not_commit_var() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("VAR source word should bootstrap");

        let (_sources, _id, error) =
            compile_with_var_error("VAR SCORE @", &mut bindings, &mut globals, &source_words);

        assert!(matches!(error, SourceProcessorError::Lex(_)));
        assert_eq!(bindings.get(&name("SCORE")), None);
        assert_eq!(globals.len(), 0);
    }

    #[test]
    fn def_publishes_empty_runtime_word_with_return_mapped_to_end() {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        let builtin = register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut code = PublishedCode::new();
        let initial_words_len = words.len();

        let (sources, id, unit) = compile_with_def(
            "def Foo\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        let Some(Binding::Word(foo)) = bindings.get(&name("FOO")).copied() else {
            panic!("FOO should be published as a runtime word");
        };
        assert_eq!(
            bindings.get(&name("DEF")),
            Some(&Binding::SourceWord(builtin.def()))
        );
        assert_eq!(words.len(), initial_words_len + 1);
        assert_eq!(
            words.get(foo).expect("published word should be defined"),
            &crate::word::WordDefinition::Compiled {
                entry: code.instruction_view().location(address(0))
            }
        );
        assert_eq!(code.len(), 1);
        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Return)
        );
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(address(0))),
            Ok(Some(span(sources.view(), id, 8, 11)))
        );
        assert_eq!(unit.len(), 1);
        assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
    }

    #[test]
    fn def_body_compiles_calls_before_appending_single_return() {
        let (mut words, mut primitives, operators) = operator_fixture();
        let primitive = primitives.register(push_7);
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        register_primitive(&mut words, &mut bindings, name("PUSH7"), primitive)
            .expect("runtime word should register");
        let mut globals = GlobalVariables::new();
        let mut code = PublishedCode::new();

        let (sources, id, _unit) = compile_with_def(
            "DEF FOO\npush7\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        assert_eq!(code.len(), 2);
        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Call(
                resolve_word_name(&bindings, "PUSH7").expect("PUSH7 should resolve")
            ))
        );
        assert_eq!(
            code.instruction_view().get(address(1)),
            Ok(&Instruction::Return)
        );
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(address(1))),
            Ok(Some(span(sources.view(), id, 14, 17)))
        );
    }

    #[test]
    fn def_rejects_header_errors_without_consuming_body_or_publishing() {
        for source_text in ["DEF", "DEF 123\nEND", "DEF FOO EXTRA\nEND"] {
            let (mut words, _primitives, operators) = operator_fixture();
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();
            let mut globals = GlobalVariables::new();
            register_builtin_source_words(&mut source_words, &mut bindings)
                .expect("built-in source words should bootstrap");
            let mut code = PublishedCode::new();
            let initial_words_len = words.len();

            let (sources, id, error) = compile_with_def_error(
                source_text,
                &mut bindings,
                &mut globals,
                &source_words,
                operators.lookup(),
                &mut code,
                &mut words,
            );

            let expected = match source_text {
                "DEF" => SourceWordError::DefSyntax {
                    span: span(sources.view(), id, 0, 3),
                    kind: DefSyntaxErrorKind::MissingName,
                },
                "DEF 123\nEND" => SourceWordError::DefSyntax {
                    span: span(sources.view(), id, 4, 7),
                    kind: DefSyntaxErrorKind::MissingName,
                },
                "DEF FOO EXTRA\nEND" => SourceWordError::DefSyntax {
                    span: span(sources.view(), id, 8, 13),
                    kind: DefSyntaxErrorKind::TrailingToken {
                        kind: TokenKind::Name,
                    },
                },
                _ => unreachable!(),
            };
            assert_eq!(error, SourceProcessorError::SourceWord(expected));
            assert_eq!(bindings.get(&name("FOO")), None);
            assert_eq!(words.len(), initial_words_len);
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn def_rejects_name_conflicts_and_reserved_end_before_building_body() {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        compile_with_var("VAR SCORE", &mut bindings, &mut globals, &source_words);
        let mut code = PublishedCode::new();
        let initial_words_len = words.len();

        let (sources, id, error) = compile_with_def_error(
            "DEF score\nMISSING\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );
        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefNameConflict {
                span: span(sources.view(), id, 4, 9)
            })
        );
        assert_eq!(code.len(), 0);
        assert_eq!(words.len(), initial_words_len);

        let (sources, id, error) = compile_with_def_error(
            "DEF END\nMISSING\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );
        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefReservedName {
                span: span(sources.view(), id, 4, 7)
            })
        );
        assert_eq!(code.len(), 0);
        assert_eq!(words.len(), initial_words_len);
    }

    #[test]
    fn def_consumes_only_through_standalone_end_and_returns_outer_processing() {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut code = PublishedCode::new();
        let push7 = words.add(completed_primitive(0));
        bindings
            .insert_new(name("PUSH7"), Binding::Word(push7))
            .expect("runtime word should register");

        let (_sources, _id, unit) = compile_with_def(
            "DEF FOO\nEND\nPUSH7",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        assert!(matches!(bindings.get(&name("FOO")), Some(Binding::Word(_))));
        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Return)
        );
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Call(push7))
        );
        assert_eq!(unit.instructions().get(address(1)), Ok(&Instruction::Halt));
    }

    #[test]
    fn def_reports_missing_end_and_lexical_terminal_without_publication() {
        for source_text in ["DEF FOO", "DEF FOO\n@"] {
            let (mut words, _primitives, operators) = operator_fixture();
            let mut source_words = SourceWordRegistry::new();
            let mut bindings = Bindings::new();
            let mut globals = GlobalVariables::new();
            register_builtin_source_words(&mut source_words, &mut bindings)
                .expect("built-in source words should bootstrap");
            let mut code = PublishedCode::new();
            let initial_words_len = words.len();

            let (sources, id, error) = compile_with_def_error(
                source_text,
                &mut bindings,
                &mut globals,
                &source_words,
                operators.lookup(),
                &mut code,
                &mut words,
            );

            match source_text {
                "DEF FOO" => assert_eq!(
                    error,
                    SourceProcessorError::SourceWord(SourceWordError::DefMissingEnd {
                        span: span(sources.view(), id, 7, 7)
                    })
                ),
                "DEF FOO\n@" => assert!(matches!(
                    error,
                    SourceProcessorError::SourceWord(SourceWordError::DefLex { .. })
                )),
                _ => unreachable!(),
            }
            assert_eq!(bindings.get(&name("FOO")), None);
            assert_eq!(words.len(), initial_words_len);
            assert_eq!(code.len(), 0);
        }
    }

    #[test]
    fn def_body_failure_does_not_publish_and_later_definition_can_build() {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut code = PublishedCode::new();
        let initial_words_len = words.len();

        let (sources, id, error) = compile_with_def_error(
            "DEF BAD\n1 MISSING\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefBodyCompile {
                span: span(sources.view(), id, 10, 17)
            })
        );
        assert_eq!(bindings.get(&name("BAD")), None);
        assert_eq!(words.len(), initial_words_len);
        assert_eq!(code.len(), 0);

        compile_with_def(
            "DEF GOOD\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );
        assert!(matches!(
            bindings.get(&name("GOOD")),
            Some(Binding::Word(_))
        ));
        assert_eq!(words.len(), initial_words_len + 1);
        assert_eq!(
            code.instruction_view().get(address(0)),
            Ok(&Instruction::Return)
        );
    }

    #[test]
    fn nested_def_body_fails_without_inner_publication_capability() {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut code = PublishedCode::new();
        let initial_words_len = words.len();

        let (sources, id, error) = compile_with_def_error(
            "DEF OUTER\nDEF INNER\nEND\nEND",
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefBodyCompile {
                span: span(sources.view(), id, 10, 13)
            })
        );
        assert_eq!(bindings.get(&name("OUTER")), None);
        assert_eq!(bindings.get(&name("INNER")), None);
        assert_eq!(words.len(), initial_words_len);
    }

    #[test]
    fn def_without_runtime_publication_context_is_structured_error() {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let (sources, id) = source("DEF FOO\nEND");

        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect_err("DEF should need runtime publication capability");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefPublicationContextUnavailable {
                span: span(sources.view(), id, 0, 3)
            })
        );
        assert_eq!(bindings.get(&name("FOO")), None);
    }

    #[test]
    fn published_def_runs_from_later_source_without_merging_code_spaces() {
        let mut session = RuntimeDefinitionSession::new();
        let push7 = session.register_primitive("PUSH7", push_7);
        session.publish_def("DEF FOO\nPUSH7\nEND");
        let Some(Binding::Word(foo)) = session.bindings.get(&name("FOO")).copied() else {
            panic!("FOO should be published");
        };
        let (caller_sources, caller_id, caller) = session.compile_caller("foo");
        let original_published_len = session.code.len();

        let result = session
            .run_unit_with_published_code(&caller)
            .expect("later caller should execute published FOO");

        assert_eq!(
            caller.instructions().get(address(0)),
            Ok(&Instruction::Call(foo))
        );
        assert_eq!(
            caller.source_span(location(&caller, 0)),
            Ok(Some(span(caller_sources.view(), caller_id, 0, 3)))
        );
        assert_eq!(
            session.code.instruction_view().get(address(0)),
            Ok(&Instruction::Call(push7))
        );
        assert_eq!(
            session.code.instruction_view().get(address(1)),
            Ok(&Instruction::Return)
        );
        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
        assert_eq!(result.instruction_count(), 2);
        assert_eq!(session.code.len(), original_published_len);
    }

    #[test]
    fn published_def_body_can_call_existing_published_runtime_word() {
        let mut session = RuntimeDefinitionSession::new();
        session.register_primitive("PUSH3", push_3);
        session.register_primitive("PUSH4", push_4);
        session.publish_def("DEF BASE\nPUSH3\nEND");
        let Some(Binding::Word(base)) = session.bindings.get(&name("BASE")).copied() else {
            panic!("BASE should be published");
        };

        session.publish_def("DEF WRAP\nbase\nPUSH4\nEND");
        let Some(Binding::Word(wrap)) = session.bindings.get(&name("WRAP")).copied() else {
            panic!("WRAP should be published");
        };
        let (_caller_sources, _caller_id, caller) = session.compile_caller("wrap");
        let result = session
            .run_unit_with_published_code(&caller)
            .expect("nested published word should run");

        assert_eq!(
            caller.instructions().get(address(0)),
            Ok(&Instruction::Call(wrap))
        );
        assert_eq!(
            session.code.instruction_view().get(address(2)),
            Ok(&Instruction::Call(base))
        );
        assert_eq!(result.data_stack(), [value(3), value(4)]);
    }

    #[test]
    fn production_published_runtime_error_maps_to_definition_source_span() {
        let mut session = RuntimeDefinitionSession::new();
        session.register_primitive("PUSH7", push_7);
        let fail = session.register_primitive("FAIL", fail_after_partial_stack_update);
        let (definition_sources, definition_id, _unit) =
            session.publish_def("DEF BAD\nPUSH7 fail\nEND");
        let (caller_sources, caller_id, caller) = session.compile_caller("bad");
        let published_views = [session.code.instruction_view()];
        let published_mappings = [session.code.source_mapping()];

        let error = run_unit(
            &caller,
            SourceExecutionContext::with_code_spaces_and_mappings(
                &session.bindings,
                &published_views,
                &published_mappings,
                PublishedWordLookup::new(&session.words),
                session.primitives.lookup(),
            ),
        )
        .expect_err("published body failure should fail caller");

        assert_eq!(
            caller.source_span(location(&caller, 0)),
            Ok(Some(span(caller_sources.view(), caller_id, 0, 3)))
        );
        assert_eq!(
            session.code.instruction_view().get(address(1)),
            Ok(&Instruction::Call(fail))
        );
        assert_runtime_error(
            error,
            session.code.instruction_view().location(address(1)),
            Ok(Some(span(definition_sources.view(), definition_id, 14, 18))),
        );
    }

    #[test]
    fn failed_def_fragment_does_not_publish_and_later_def_runs_after_fragment() {
        let mut session = RuntimeDefinitionSession::new();
        session.register_primitive("PUSH5", push_5);
        session.register_primitive("PUSH7", push_7);
        let (keep_sources, keep_id, _keep_unit) = session.publish_def("DEF KEEP\nPUSH5\nEND");
        let Some(Binding::Word(keep)) = session.bindings.get(&name("KEEP")).copied() else {
            panic!("KEEP should be published");
        };
        let keep_entry = session.code.instruction_view().location(address(0));
        let keep_span = span(keep_sources.view(), keep_id, 9, 14);
        let words_len_before_failure = session.words.len();
        let code_len_before_failure = session.code.len();

        let (bad_sources, bad_id, error) = session.publish_def_error("DEF BAD\n1 MISSING\nEND");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::DefBodyCompile {
                span: span(bad_sources.view(), bad_id, 10, 17)
            })
        );
        assert_eq!(session.bindings.get(&name("BAD")), None);
        assert_eq!(session.words.len(), words_len_before_failure);
        assert_eq!(
            session.words.get(keep).expect("KEEP should remain defined"),
            &crate::word::WordDefinition::Compiled { entry: keep_entry }
        );
        assert_eq!(
            session.code.source_mapping().source_span(keep_entry),
            Ok(Some(keep_span))
        );
        assert_eq!(session.code.len(), code_len_before_failure);

        session.publish_def("DEF GOOD\nPUSH7\nEND");
        let Some(Binding::Word(good)) = session.bindings.get(&name("GOOD")).copied() else {
            panic!("GOOD should be published");
        };
        let expected_good_entry = session
            .code
            .instruction_view()
            .location(address(code_len_before_failure));
        assert_eq!(
            session.words.get(good).expect("GOOD should be defined"),
            &crate::word::WordDefinition::Compiled {
                entry: expected_good_entry
            }
        );

        let (_keep_caller_sources, _keep_caller_id, keep_result) = session.run_caller("keep");
        let (_good_caller_sources, _good_caller_id, good_result) = session.run_caller("good");

        assert_eq!(keep_result.data_stack(), [value(5)]);
        assert_eq!(good_result.data_stack(), [value(7)]);
    }

    #[test]
    fn published_code_redefinition_preserves_early_bound_callers_and_mappings() {
        let mut session = RuntimeDefinitionSession::new();
        session.register_primitive("PUSH41", push_41);
        let (old_sources, old_id, _old_def_unit) = session.publish_def("DEF TARGET\nPUSH41\nEND");
        let Some(Binding::Word(old)) = session.bindings.get(&name("TARGET")).copied() else {
            panic!("TARGET should be published");
        };
        let (_old_caller_sources, _old_caller_id, old_caller) = session.compile_caller("target");
        let old_entry = session.code.instruction_view().location(address(0));
        let old_span = span(old_sources.view(), old_id, 11, 17);

        let (new_sources, new_id) = source("99\nEND");
        let new_value_span = span(new_sources.view(), new_id, 0, 2);
        let new_end_span = span(new_sources.view(), new_id, 3, 6);
        let redefinition = session
            .code
            .redefine_word(
                &mut session.words,
                &mut session.bindings,
                &name("TARGET"),
                |builder| {
                    builder.append_mapped(Instruction::Push(value(99)), new_value_span)?;
                    builder.append_mapped(Instruction::Return, new_end_span)?;
                    Ok(())
                },
            )
            .expect("TARGET should redefine in published code");
        let (_new_caller_sources, _new_caller_id, new_caller) = session.compile_caller("target");
        let new_entry = session.code.instruction_view().location(address(2));

        let old_result = session
            .run_unit_with_published_code(&old_caller)
            .expect("old caller should still run old body");
        let new_result = session
            .run_unit_with_published_code(&new_caller)
            .expect("new caller should run new body");

        assert_eq!(redefinition.previous(), old);
        assert_ne!(redefinition.previous(), redefinition.current());
        assert_eq!(
            old_caller.instructions().get(address(0)),
            Ok(&Instruction::Call(redefinition.previous()))
        );
        assert_eq!(
            new_caller.instructions().get(address(0)),
            Ok(&Instruction::Call(redefinition.current()))
        );
        assert_eq!(old_result.data_stack(), [value(41)]);
        assert_eq!(new_result.data_stack(), [value(99)]);
        assert_eq!(
            session
                .words
                .get(old)
                .expect("old word should remain defined"),
            &crate::word::WordDefinition::Compiled { entry: old_entry }
        );
        assert_eq!(
            session
                .words
                .get(redefinition.current())
                .expect("new word should be defined"),
            &crate::word::WordDefinition::Compiled { entry: new_entry }
        );
        assert_eq!(
            session.code.source_mapping().source_span(old_entry),
            Ok(Some(old_span))
        );
        assert_eq!(
            session.code.source_mapping().source_span(new_entry),
            Ok(Some(new_value_span))
        );
    }

    #[test]
    fn case_variants_resolve_to_same_word_id_during_compile() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let id = publish_initial(&mut words, &mut bindings, "ready?", completed_primitive(2));

        let (_sources, _source_id, unit) = compile_with_bindings("ready? Ready? READY?", &bindings);

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Call(id))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Call(id))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Call(id))
        );
    }

    #[test]
    fn primitive_and_compiled_words_use_same_resolve_and_emit_path() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut shared_code = InstructionSequence::new();
        let primitive = publish_initial(&mut words, &mut bindings, "PRIM", completed_primitive(3));
        let compiled = publish_initial(
            &mut words,
            &mut bindings,
            "USER_WORD",
            completed_compiled(&mut shared_code, 10),
        );

        let (_sources, _source_id, unit) = compile_with_bindings("prim user_word", &bindings);

        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Call(primitive))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Call(compiled))
        );
    }

    #[test]
    fn saved_execution_unit_keeps_old_word_id_after_redefinition() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut shared_code = InstructionSequence::new();
        let old = publish_initial(&mut words, &mut bindings, "TARGET", completed_primitive(4));

        let (_old_sources, _old_source_id, old_unit) = compile_with_bindings("target", &bindings);
        let redefinition = redefine_word(
            &mut words,
            &mut bindings,
            &name("TARGET"),
            completed_compiled(&mut shared_code, 11),
        )
        .expect("existing word should redefine");
        let (_new_sources, _new_source_id, new_unit) = compile_with_bindings("target", &bindings);

        assert_eq!(redefinition.previous(), old);
        assert_ne!(redefinition.previous(), redefinition.current());
        assert_eq!(
            old_unit.instructions().get(address(0)),
            Ok(&Instruction::Call(redefinition.previous()))
        );
        assert_eq!(
            new_unit.instructions().get(address(0)),
            Ok(&Instruction::Call(redefinition.current()))
        );
        assert_eq!(
            old_unit.instructions().get(address(0)),
            Ok(&Instruction::Call(old))
        );
    }

    #[test]
    fn undefined_name_is_span_compile_error_without_publication_mutation() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let id = publish_initial(&mut words, &mut bindings, "KNOWN", completed_primitive(5));
        primitives.register(push_7);
        let (sources, source_id) = source("known missing");

        let error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::new(&bindings),
        )
        .expect_err("undefined name should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), source_id, 6, 13),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName
                },
            })
        );
        assert_eq!(words.len(), 1);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.get(&name("KNOWN")), Some(&Binding::Word(id)));
        assert_eq!(primitives.len(), 1);
    }

    #[test]
    fn primitive_word_call_runs_from_temporary_execution_unit() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(push_7);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), primitive)
            .expect("primitive should register");
        let (sources, source_id) = source("push7");

        let result = run_source(
            sources.view(),
            source_id,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("primitive call should run");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
        assert_eq!(result.instruction_count(), 2);
    }

    #[test]
    fn compiled_word_call_runs_with_temporary_and_published_code_spaces() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();
        let mut published_code = InstructionSequence::new();
        publish_initial(
            &mut words,
            &mut bindings,
            "USER_WORD",
            completed_compiled(&mut published_code, 10),
        );
        published_code.append(Instruction::Return);
        let (sources, source_id) = source("user_word");
        let published_views = [published_code.view()];

        let result = run_source(
            sources.view(),
            source_id,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("published compiled word should run");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(10)]);
        assert_eq!(result.instruction_count(), 2);
    }

    #[test]
    fn integer_literals_primitive_and_compiled_calls_run_in_source_order() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut published_code = InstructionSequence::new();
        let push2 = primitives.register(push_2);
        let primitive = primitives.register(add_top_two);
        publish_initial(
            &mut words,
            &mut bindings,
            "USER_WORD",
            completed_compiled(&mut published_code, 5),
        );
        published_code.append(Instruction::Return);
        register_primitive(&mut words, &mut bindings, name("PUSH2"), push2)
            .expect("primitive should register");
        register_primitive(&mut words, &mut bindings, name("ADD"), primitive)
            .expect("primitive should register");
        let (sources, source_id) = source("PUSH2 user_word add");
        let published_views = [published_code.view()];

        let result = run_source(
            sources.view(),
            source_id,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("mixed source should run");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
    }

    #[test]
    fn published_compiled_word_can_call_nested_compiled_words() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();

        let mut inner_code = InstructionSequence::new();
        let inner_entry = inner_code.append(Instruction::Push(value(3)));
        inner_code.append(Instruction::Return);
        let inner = publish_initial(
            &mut words,
            &mut bindings,
            "INNER",
            completed_compiled_at(&inner_code, inner_entry),
        );

        let mut middle_code = InstructionSequence::new();
        let middle_entry = middle_code.append(Instruction::Call(inner));
        middle_code.append(Instruction::Push(value(4)));
        middle_code.append(Instruction::Return);
        let middle = publish_initial(
            &mut words,
            &mut bindings,
            "MIDDLE",
            completed_compiled_at(&middle_code, middle_entry),
        );

        let mut outer_code = InstructionSequence::new();
        let outer_entry = outer_code.append(Instruction::Call(middle));
        outer_code.append(Instruction::Push(value(5)));
        outer_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "OUTER",
            completed_compiled_at(&outer_code, outer_entry),
        );

        let (sources, source_id) = source("outer");
        let published_views = [inner_code.view(), middle_code.view(), outer_code.view()];

        let result = run_source(
            sources.view(),
            source_id,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("nested compiled call should run");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(3), value(4), value(5)]);
    }

    #[test]
    fn saved_unit_runs_old_compiled_entry_after_redefinition() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();

        let mut old_code = InstructionSequence::new();
        let old_entry = old_code.append(Instruction::Push(value(41)));
        old_code.append(Instruction::Return);
        let old = publish_initial(
            &mut words,
            &mut bindings,
            "TARGET",
            completed_compiled_at(&old_code, old_entry),
        );
        let (_old_sources, _old_source_id, old_unit) = compile_with_bindings("target", &bindings);

        let mut new_code = InstructionSequence::new();
        let new_entry = new_code.append(Instruction::Push(value(99)));
        new_code.append(Instruction::Return);
        let redefinition = redefine_word(
            &mut words,
            &mut bindings,
            &name("TARGET"),
            completed_compiled_at(&new_code, new_entry),
        )
        .expect("existing word should redefine");
        let (new_sources, new_source_id) = source("target");
        let published_views = [old_code.view(), new_code.view()];

        let old_result = run_unit(
            &old_unit,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("old early-bound unit should run");
        let new_result = run_source(
            new_sources.view(),
            new_source_id,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("newly compiled unit should run");

        assert_eq!(redefinition.previous(), old);
        assert_ne!(redefinition.previous(), redefinition.current());
        assert_eq!(
            old_unit.instructions().get(address(0)),
            Ok(&Instruction::Call(old))
        );
        assert_eq!(old_result.data_stack(), [value(41)]);
        assert_eq!(new_result.data_stack(), [value(99)]);
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn source_run_does_not_publish_temporary_code_or_reuse_vm_state() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();
        let mut published_code = InstructionSequence::new();
        publish_initial(
            &mut words,
            &mut bindings,
            "USER_WORD",
            completed_compiled(&mut published_code, 8),
        );
        published_code.append(Instruction::Return);
        let original_word_count = words.len();
        let original_published_len = published_code.len();
        let (mut sources, first) = source("user_word user_word");
        let second = sources.register("user_word");
        let published_views = [published_code.view()];
        let first_result = run_source(
            sources.view(),
            first,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("first source should run with a fresh VM");
        let second_result = run_source(
            sources.view(),
            second,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("second source should run with a fresh VM");

        assert_eq!(first_result.data_stack(), [value(8), value(8)]);
        assert_eq!(second_result.data_stack(), [value(8)]);
        assert_eq!(words.len(), original_word_count);
        assert_eq!(published_code.len(), original_published_len);
    }

    #[test]
    fn primitive_failure_reports_call_address_through_vm_boundary() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let push7 = primitives.register(push_7);
        let primitive = primitives.register(fail_after_partial_stack_update);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
            .expect("primitive should register");
        register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
            .expect("primitive should register");
        let (sources, source_id) = source("PUSH7 fail");

        let error = run_source(
            sources.view(),
            source_id,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("primitive failure should fail source run");
        let SourceProcessorError::Runtime(error) = error else {
            panic!("expected runtime error");
        };
        assert_eq!(
            error.source_span(),
            Ok(Some(span(sources.view(), source_id, 6, 10)))
        );
        let error = error.vm();

        assert_eq!(error.address(), address(1));
        match error.kind() {
            crate::vm::VmErrorKind::PrimitiveFailed {
                primitive: actual, ..
            } => assert_eq!(actual, primitive),
            other => panic!("unexpected VM error kind: {other:?}"),
        }
    }

    #[test]
    fn temporary_runtime_error_maps_to_temporary_source_span() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let push7 = primitives.register(push_7);
        let primitive = primitives.register(fail_after_partial_stack_update);
        register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
            .expect("primitive should register");
        register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
            .expect("primitive should register");
        let (sources, source_id, unit) = compile_with_bindings("PUSH7 fail", &bindings);

        let error = run_unit(
            &unit,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("primitive failure should fail source run");

        assert_runtime_error(
            error,
            location(&unit, 1),
            Ok(Some(span(sources.view(), source_id, 6, 10))),
        );
    }

    #[test]
    fn published_runtime_error_maps_to_published_source_span() {
        let mut sources = SourceTexts::new();
        let published_source = sources.register("fail");
        let temporary_source = sources.register("bad");
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail_after_partial_stack_update);
        let fail = publish_initial(
            &mut words,
            &mut bindings,
            "FAIL",
            CompletedWordDefinition::primitive(primitive),
        );
        let mut published_code = InstructionSequence::new();
        let published_entry = published_code.append(Instruction::Call(fail));
        let published_return = published_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "BAD",
            completed_compiled_at(&published_code, published_entry),
        );
        let published_span = span(sources.view(), published_source, 0, 4);
        let published_mapping = mapping_for(
            &published_code,
            &[
                (published_entry, Some(published_span)),
                (published_return, None),
            ],
        );
        let published_views = [published_code.view()];
        let mapping_views = [published_mapping.view()];

        let error = run_source(
            sources.view(),
            temporary_source,
            SourceExecutionContext::with_code_spaces_and_mappings(
                &bindings,
                &published_views,
                &mapping_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("published primitive failure should fail source run");

        assert_runtime_error(
            error,
            published_code.view().location(published_entry),
            Ok(Some(published_span)),
        );
    }

    #[test]
    fn nested_published_runtime_error_uses_deepest_callee_mapping() {
        let mut sources = SourceTexts::new();
        let inner_source = sources.register("inner_fail");
        let middle_source = sources.register("middle_call");
        let outer_source = sources.register("outer_call");
        let temporary_source = sources.register("outer");
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail_after_partial_stack_update);
        let fail = publish_initial(
            &mut words,
            &mut bindings,
            "FAIL",
            CompletedWordDefinition::primitive(primitive),
        );

        let mut inner_code = InstructionSequence::new();
        let inner_entry = inner_code.append(Instruction::Call(fail));
        let inner_return = inner_code.append(Instruction::Return);
        let inner = publish_initial(
            &mut words,
            &mut bindings,
            "INNER",
            completed_compiled_at(&inner_code, inner_entry),
        );

        let mut middle_code = InstructionSequence::new();
        let middle_entry = middle_code.append(Instruction::Call(inner));
        let middle_return = middle_code.append(Instruction::Return);
        let middle = publish_initial(
            &mut words,
            &mut bindings,
            "MIDDLE",
            completed_compiled_at(&middle_code, middle_entry),
        );

        let mut outer_code = InstructionSequence::new();
        let outer_entry = outer_code.append(Instruction::Call(middle));
        let outer_return = outer_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "OUTER",
            completed_compiled_at(&outer_code, outer_entry),
        );

        let inner_span = span(sources.view(), inner_source, 0, 10);
        let middle_span = span(sources.view(), middle_source, 0, 11);
        let outer_span = span(sources.view(), outer_source, 0, 10);
        let inner_mapping = mapping_for(
            &inner_code,
            &[(inner_entry, Some(inner_span)), (inner_return, None)],
        );
        let middle_mapping = mapping_for(
            &middle_code,
            &[(middle_entry, Some(middle_span)), (middle_return, None)],
        );
        let outer_mapping = mapping_for(
            &outer_code,
            &[(outer_entry, Some(outer_span)), (outer_return, None)],
        );
        let published_views = [inner_code.view(), middle_code.view(), outer_code.view()];
        let mapping_views = [
            inner_mapping.view(),
            middle_mapping.view(),
            outer_mapping.view(),
        ];

        let error = run_source(
            sources.view(),
            temporary_source,
            SourceExecutionContext::with_code_spaces_and_mappings(
                &bindings,
                &published_views,
                &mapping_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("nested published primitive failure should fail source run");

        assert_eq!(inner_entry.as_index(), middle_entry.as_index());
        assert_eq!(middle_entry.as_index(), outer_entry.as_index());
        assert_runtime_error(
            error,
            inner_code.view().location(inner_entry),
            Ok(Some(inner_span)),
        );
    }

    #[test]
    fn published_runtime_error_without_mapping_is_unknown_space() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail_after_partial_stack_update);
        let fail = publish_initial(
            &mut words,
            &mut bindings,
            "FAIL",
            CompletedWordDefinition::primitive(primitive),
        );
        let mut published_code = InstructionSequence::new();
        let published_entry = published_code.append(Instruction::Call(fail));
        published_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "BAD",
            completed_compiled_at(&published_code, published_entry),
        );
        let (sources, source_id) = source("bad");
        let published_views = [published_code.view()];

        let error = run_source(
            sources.view(),
            source_id,
            SourceExecutionContext::with_code_spaces(
                &bindings,
                &published_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("published primitive failure should fail source run");

        assert_runtime_error(
            error,
            published_code.view().location(published_entry),
            Err(SourceMappingLookupError::UnknownCodeSpace {
                code_space: published_code.code_space(),
            }),
        );
    }

    #[test]
    fn runtime_error_mapping_distinguishes_end_out_of_range_and_unmapped() {
        let mut sources = SourceTexts::new();
        let temporary_source = sources.register("bad");
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail_after_partial_stack_update);
        let fail = publish_initial(
            &mut words,
            &mut bindings,
            "FAIL",
            CompletedWordDefinition::primitive(primitive),
        );

        let mut end_code = InstructionSequence::new();
        let end_entry = end_code.append(Instruction::Call(fail));
        end_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "ENDFAIL",
            completed_compiled_at(&end_code, end_entry),
        );
        let end_mapping = InstructionSourceMapping::new(end_code.code_space());

        let mut out_of_range_code = InstructionSequence::new();
        let out_of_range_padding = out_of_range_code.append(Instruction::Push(value(1)));
        out_of_range_code.append(Instruction::Push(value(2)));
        let out_of_range_entry = out_of_range_code.append(Instruction::Call(fail));
        out_of_range_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "RANGEFAIL",
            completed_compiled_at(&out_of_range_code, out_of_range_entry),
        );
        let out_of_range_mapping = mapping_for(
            &out_of_range_code,
            &[(
                out_of_range_padding,
                Some(span(sources.view(), temporary_source, 0, 3)),
            )],
        );

        let mut unmapped_code = InstructionSequence::new();
        let unmapped_entry = unmapped_code.append(Instruction::Call(fail));
        unmapped_code.append(Instruction::Return);
        publish_initial(
            &mut words,
            &mut bindings,
            "UNMAPPEDFAIL",
            completed_compiled_at(&unmapped_code, unmapped_entry),
        );
        let unmapped_mapping = mapping_for(&unmapped_code, &[(unmapped_entry, None)]);

        let published_views = [
            end_code.view(),
            out_of_range_code.view(),
            unmapped_code.view(),
        ];
        let mapping_views = [
            end_mapping.view(),
            out_of_range_mapping.view(),
            unmapped_mapping.view(),
        ];
        let end_source = sources.register("endfail");
        let out_of_range_source = sources.register("rangefail");
        let unmapped_source = sources.register("unmappedfail");

        assert_runtime_error(
            run_source(
                sources.view(),
                end_source,
                SourceExecutionContext::with_code_spaces_and_mappings(
                    &bindings,
                    &published_views,
                    &mapping_views,
                    PublishedWordLookup::new(&words),
                    primitives.lookup(),
                ),
            )
            .expect_err("end mapping should fail"),
            end_code.view().location(end_entry),
            Err(SourceMappingLookupError::Address {
                source: crate::instruction::InstructionAddressError::EndAddress {
                    address: end_entry,
                },
            }),
        );
        assert_runtime_error(
            run_source(
                sources.view(),
                out_of_range_source,
                SourceExecutionContext::with_code_spaces_and_mappings(
                    &bindings,
                    &published_views,
                    &mapping_views,
                    PublishedWordLookup::new(&words),
                    primitives.lookup(),
                ),
            )
            .expect_err("out-of-range mapping should fail"),
            out_of_range_code.view().location(out_of_range_entry),
            Err(SourceMappingLookupError::Address {
                source: crate::instruction::InstructionAddressError::InvalidAddress {
                    address: out_of_range_entry,
                },
            }),
        );
        assert_runtime_error(
            run_source(
                sources.view(),
                unmapped_source,
                SourceExecutionContext::with_code_spaces_and_mappings(
                    &bindings,
                    &published_views,
                    &mapping_views,
                    PublishedWordLookup::new(&words),
                    primitives.lookup(),
                ),
            )
            .expect_err("unmapped location should preserve VM failure"),
            unmapped_code.view().location(unmapped_entry),
            Ok(None),
        );
    }

    #[test]
    fn compile_failure_does_not_return_partial_execution_unit() {
        let (sources, id) = source("RUN");
        let bindings = Bindings::new();
        let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect_err("source should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 3),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName
                },
            })
        );
    }

    #[test]
    fn compile_error_accessors_expose_primary_span_and_kind() {
        let (sources, id, error) = compile_error("32768");
        let SourceProcessorError::Compile(error) = error else {
            panic!("expected compile error");
        };

        assert_eq!(error.span(), span(sources.view(), id, 0, 5));
        assert_eq!(error.kind(), CompileErrorKind::BareExpression);
    }
}
