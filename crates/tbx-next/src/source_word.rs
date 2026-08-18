use crate::binding::{Binding, BindingInsertError, Bindings};
use crate::expression::{
    parse_expression, ExpressionError, ExpressionStaging, ExpressionVariableErrorKind,
};
use crate::global_variable::GlobalVariables;
use crate::instruction::Instruction;
use crate::instruction_builder::{InstructionBuildError, InstructionBuildTarget};
use crate::lexer::{LexError, Token, TokenKind};
use crate::name::{NameError, NormalizedName};
use crate::operator::OperatorLookup;
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::word::WordId;
use crate::word_resolution::{resolve_binding_name, ResolvedBinding, WordResolutionError};

/// Internal identifier for a published source-processing word.
///
/// Source words share ordinary name binding with runtime words and variables,
/// but they are intentionally not executable runtime words and never receive a
/// `WordId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceWordId {
    slot: usize,
}

impl SourceWordId {
    pub(crate) const fn from_slot(slot: usize) -> Self {
        Self { slot }
    }

    pub(crate) const fn as_slot(self) -> usize {
        self.slot
    }
}

pub(crate) type NativeSourceWordHandler =
    fn(&mut NativeSourceWordContext<'_, '_>) -> Result<(), SourceWordError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceWordSyntaxMarkerRole {
    BlockContinuation,
    BlockTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceWordSyntaxMarker {
    name: NormalizedName,
    role: SourceWordSyntaxMarkerRole,
}

impl SourceWordSyntaxMarker {
    pub(crate) fn new(name: NormalizedName, role: SourceWordSyntaxMarkerRole) -> Self {
        Self { name, role }
    }

    pub(crate) fn name(&self) -> &NormalizedName {
        &self.name
    }

    pub(crate) const fn role(&self) -> SourceWordSyntaxMarkerRole {
        self.role
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceWordError {
    Source {
        source: SourceError,
    },
    InstructionBuild {
        source: InstructionBuildError,
    },
    UnsupportedSourceWord {
        span: SourceSpan,
    },
    VarSyntax {
        span: SourceSpan,
        kind: VarSyntaxErrorKind,
    },
    VarLocalLineNumberPrefix {
        span: SourceSpan,
    },
    VarPublicationContextUnavailable,
    VarName {
        span: SourceSpan,
        source: NameError,
    },
    VarNameConflict {
        span: SourceSpan,
    },
    VarReservedName {
        span: SourceSpan,
    },
    VarBindingCommitInvariantViolated {
        span: SourceSpan,
    },
    LetSyntax {
        span: SourceSpan,
        kind: LetSyntaxErrorKind,
    },
    LetTarget {
        span: SourceSpan,
        source: ExpressionVariableErrorKind,
    },
    LetExpressionContextUnavailable {
        span: SourceSpan,
    },
    Expression {
        source: ExpressionError,
    },
    DefSyntax {
        span: SourceSpan,
        kind: DefSyntaxErrorKind,
    },
    DefName {
        span: SourceSpan,
        source: NameError,
    },
    DefNameConflict {
        span: SourceSpan,
    },
    DefReservedName {
        span: SourceSpan,
    },
    DefPublicationContextUnavailable {
        span: SourceSpan,
    },
    DefMissingEnd {
        span: SourceSpan,
    },
    DefLex {
        source: LexError,
    },
    DefBodyCompile {
        span: SourceSpan,
    },
    DefBodyBuild {
        span: SourceSpan,
    },
    DefDefinition {
        span: SourceSpan,
    },
    DefBindingCommitInvariantViolated {
        span: SourceSpan,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceWordLookupError {
    InvalidSourceWordId { id: SourceWordId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VarSyntaxErrorKind {
    MissingName,
    TrailingToken { kind: TokenKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LetSyntaxErrorKind {
    Target,
    Equal,
    Rhs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefSyntaxErrorKind {
    MissingName,
    TrailingToken { kind: TokenKind },
}

impl SourceWordError {
    pub(crate) fn primary_span(self) -> Option<SourceSpan> {
        match self {
            Self::Source { .. }
            | Self::InstructionBuild { .. }
            | Self::VarPublicationContextUnavailable
            | Self::Expression { .. } => None,
            Self::UnsupportedSourceWord { span }
            | Self::VarSyntax { span, .. }
            | Self::VarLocalLineNumberPrefix { span }
            | Self::VarName { span, .. }
            | Self::VarNameConflict { span }
            | Self::VarReservedName { span }
            | Self::VarBindingCommitInvariantViolated { span }
            | Self::LetSyntax { span, .. }
            | Self::LetTarget { span, .. }
            | Self::LetExpressionContextUnavailable { span }
            | Self::DefSyntax { span, .. }
            | Self::DefName { span, .. }
            | Self::DefNameConflict { span }
            | Self::DefReservedName { span }
            | Self::DefPublicationContextUnavailable { span }
            | Self::DefMissingEnd { span }
            | Self::DefBodyCompile { span }
            | Self::DefBodyBuild { span }
            | Self::DefDefinition { span }
            | Self::DefBindingCommitInvariantViolated { span } => Some(span),
            Self::DefLex { source } => match source {
                LexError::Source(_) => None,
                LexError::InvalidCharacter { span, .. } => Some(span),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceStatementReaderError {
    Missing {
        expected: SourceStatementExpected,
        span: SourceSpan,
    },
    Unexpected {
        expected: SourceStatementExpected,
        actual: Token,
    },
    TrailingToken {
        actual: Token,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceStatementExpected {
    Name,
    Token(TokenKind),
    Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceBlockStatement<'source> {
    tokens: &'source [Token],
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceBlockMarker<'source> {
    statement: SourceBlockStatement<'source>,
    token: Token,
    name: NormalizedName,
    role: SourceWordSyntaxMarkerRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceBlockTerminal {
    Eof { span: SourceSpan },
    LexError { error: LexError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceBlockRead<'source> {
    Statement(SourceBlockStatement<'source>),
    Terminal(SourceBlockTerminal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceBlockItem<'source> {
    Statement(SourceBlockStatement<'source>),
    Marker(SourceBlockMarker<'source>),
    Terminal(SourceBlockTerminal),
}

pub(crate) trait SourceBlockCursor<'source> {
    fn read_next_block_statement(&mut self) -> Result<SourceBlockRead<'source>, SourceWordError>;
}

pub(crate) struct SourceBlockReader<'source, 'cursor> {
    view: SourceView<'source>,
    cursor: &'cursor mut dyn SourceBlockCursor<'source>,
    syntax_markers: &'cursor [SourceWordSyntaxMarker],
}

pub(crate) trait RuntimeDefinitionPublisher<'source> {
    fn publish_runtime_definition(
        &mut self,
        bindings: &mut Bindings,
        name: NormalizedName,
        name_span: SourceSpan,
        body: &[SourceBlockStatement<'source>],
        end_span: SourceSpan,
    ) -> Result<WordId, SourceWordError>;
}

impl<'source> SourceBlockStatement<'source> {
    pub(crate) const fn new(tokens: &'source [Token], span: SourceSpan) -> Self {
        Self { tokens, span }
    }

    pub(crate) const fn tokens(self) -> &'source [Token] {
        self.tokens
    }

    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }

    pub(crate) fn leading_name(self) -> Option<Token> {
        self.tokens
            .first()
            .copied()
            .filter(|token| token.kind() == TokenKind::Name)
    }

    pub(crate) fn standalone_name(self) -> Option<Token> {
        match self.tokens {
            [token] if token.kind() == TokenKind::Name => Some(*token),
            _ => None,
        }
    }
}

impl<'source> SourceBlockMarker<'source> {
    pub(crate) const fn statement(&self) -> SourceBlockStatement<'source> {
        self.statement
    }

    pub(crate) const fn token(&self) -> Token {
        self.token
    }

    pub(crate) fn remaining_tokens(&self) -> &'source [Token] {
        let marker_len = usize::from(!self.statement.tokens().is_empty());
        &self.statement.tokens()[marker_len..]
    }

    pub(crate) fn name(&self) -> &NormalizedName {
        &self.name
    }

    pub(crate) const fn role(&self) -> SourceWordSyntaxMarkerRole {
        self.role
    }

    pub(crate) const fn span(&self) -> SourceSpan {
        self.statement.span()
    }
}

impl SourceBlockTerminal {
    pub(crate) const fn eof_span(self) -> Option<SourceSpan> {
        match self {
            Self::Eof { span } => Some(span),
            Self::LexError { .. } => None,
        }
    }

    pub(crate) const fn lex_error(self) -> Option<LexError> {
        match self {
            Self::Eof { .. } => None,
            Self::LexError { error } => Some(error),
        }
    }
}

impl<'source, 'cursor> SourceBlockReader<'source, 'cursor> {
    pub(crate) fn new(
        view: SourceView<'source>,
        cursor: &'cursor mut dyn SourceBlockCursor<'source>,
        syntax_markers: &'cursor [SourceWordSyntaxMarker],
    ) -> Self {
        Self {
            view,
            cursor,
            syntax_markers,
        }
    }

    pub(crate) fn next_statement(&mut self) -> Result<SourceBlockRead<'source>, SourceWordError> {
        self.cursor.read_next_block_statement()
    }

    pub(crate) fn next_item(&mut self) -> Result<SourceBlockItem<'source>, SourceWordError> {
        match self.next_statement()? {
            SourceBlockRead::Statement(statement) => {
                if let Some(marker) = self.classify_marker(statement)? {
                    Ok(SourceBlockItem::Marker(marker))
                } else {
                    Ok(SourceBlockItem::Statement(statement))
                }
            }
            SourceBlockRead::Terminal(terminal) => Ok(SourceBlockItem::Terminal(terminal)),
        }
    }

    fn classify_marker(
        &self,
        statement: SourceBlockStatement<'source>,
    ) -> Result<Option<SourceBlockMarker<'source>>, SourceWordError> {
        // #1513/#1516: structured matching is owner-declaration driven and
        // complete-statement based. The outer reader must not raw-scan nested
        // marker spellings or treat another source word's markers as its own.
        let Some(token) = statement.leading_name() else {
            return Ok(None);
        };
        let source_name = self
            .view
            .slice(token.span())
            .map_err(|source| SourceWordError::Source { source })?;
        let Ok(name) = NormalizedName::new(source_name) else {
            return Ok(None);
        };

        Ok(self
            .syntax_markers
            .iter()
            .find(|marker| marker.name() == &name)
            .map(|marker| SourceBlockMarker {
                statement,
                token,
                name,
                role: marker.role(),
            }))
    }
}

/// Forward-only reader over the body of one completed logical statement.
///
/// Source words receive this boundary instead of raw statement tokens. It can
/// consume only the current statement slice handed in by segmentation and has
/// no rewind, absolute seek, or cross-statement access.
#[derive(Debug)]
pub(crate) struct SourceStatementReader<'source> {
    tokens: &'source [Token],
    position: usize,
    missing_anchor: SourceSpan,
}

impl<'source> SourceStatementReader<'source> {
    const fn new(tokens: &'source [Token], missing_anchor: SourceSpan) -> Self {
        Self {
            tokens,
            position: 0,
            missing_anchor,
        }
    }

    pub(crate) fn read_name(&mut self) -> Result<Token, SourceStatementReaderError> {
        let token = self.expect_present(SourceStatementExpected::Name)?;
        if token.kind() != TokenKind::Name {
            return Err(SourceStatementReaderError::Unexpected {
                expected: SourceStatementExpected::Name,
                actual: token,
            });
        }
        self.consume(token);
        Ok(token)
    }

    pub(crate) fn expect(
        &mut self,
        expected: TokenKind,
    ) -> Result<Token, SourceStatementReaderError> {
        let expected_item = SourceStatementExpected::Token(expected);
        let token = self.expect_present(expected_item)?;
        if token.kind() != expected {
            return Err(SourceStatementReaderError::Unexpected {
                expected: expected_item,
                actual: token,
            });
        }
        self.consume(token);
        Ok(token)
    }

    pub(crate) fn remaining_expression(
        &mut self,
    ) -> Result<&'source [Token], SourceStatementReaderError> {
        if self.is_exhausted() {
            return Err(SourceStatementReaderError::Missing {
                expected: SourceStatementExpected::Expression,
                span: self.missing_anchor,
            });
        }

        let remaining = &self.tokens[self.position..];
        self.position = self.tokens.len();
        Ok(remaining)
    }

    pub(crate) fn finish(&self) -> Result<(), SourceStatementReaderError> {
        if let Some(actual) = self.tokens.get(self.position).copied() {
            return Err(SourceStatementReaderError::TrailingToken { actual });
        }
        Ok(())
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.position == self.tokens.len()
    }

    fn expect_present(
        &self,
        expected: SourceStatementExpected,
    ) -> Result<Token, SourceStatementReaderError> {
        self.tokens
            .get(self.position)
            .copied()
            .ok_or(SourceStatementReaderError::Missing {
                expected,
                span: self.missing_anchor,
            })
    }

    fn consume(&mut self, token: Token) {
        self.position += 1;
        self.missing_anchor = token.span();
    }
}

/// Narrow source-processing capability passed to native source words.
///
/// This is deliberately smaller than a compiler or VM handle. Native source
/// words can inspect the current logical statement and emit mapped temporary
/// instructions. Publication-capable contexts expose only explicit declaration
/// operations; native handlers still cannot mutate words, runtime VM state, or
/// published code spaces through this context.
pub(crate) struct NativeSourceWordContext<'source, 'state> {
    view: SourceView<'source>,
    source_id: SourceId,
    source_word_token: Token,
    reader: SourceStatementReader<'source>,
    block_reader: Option<SourceBlockReader<'source, 'state>>,
    bindings: NativeSourceWordBindingAccess<'state>,
    operators: Option<OperatorLookup>,
    code: &'state mut dyn InstructionBuildTarget,
    local_line_number_prefix: Option<SourceSpan>,
    globals: Option<&'state mut GlobalVariables>,
    runtime_definitions: Option<&'state mut dyn RuntimeDefinitionPublisher<'source>>,
}

pub(crate) struct NativeSourceWordContextParts<'source, 'state> {
    pub(crate) view: SourceView<'source>,
    pub(crate) source_id: SourceId,
    pub(crate) tokens: &'source [Token],
    pub(crate) block_reader: Option<SourceBlockReader<'source, 'state>>,
    pub(crate) bindings: NativeSourceWordBindingAccess<'state>,
    pub(crate) operators: Option<OperatorLookup>,
    pub(crate) code: &'state mut dyn InstructionBuildTarget,
    pub(crate) local_line_number_prefix: Option<SourceSpan>,
    pub(crate) globals: Option<&'state mut GlobalVariables>,
    pub(crate) runtime_definitions: Option<&'state mut dyn RuntimeDefinitionPublisher<'source>>,
}

impl<'source, 'state> NativeSourceWordContext<'source, 'state> {
    pub(crate) fn new(parts: NativeSourceWordContextParts<'source, 'state>) -> Self {
        let source_word_token = parts
            .tokens
            .first()
            .copied()
            .expect("source word context requires its leading token");
        let reader = SourceStatementReader::new(&parts.tokens[1..], source_word_token.span());
        Self {
            view: parts.view,
            source_id: parts.source_id,
            source_word_token,
            reader,
            block_reader: parts.block_reader,
            bindings: parts.bindings,
            operators: parts.operators,
            code: parts.code,
            local_line_number_prefix: parts.local_line_number_prefix,
            globals: parts.globals,
            runtime_definitions: parts.runtime_definitions,
        }
    }

    pub(crate) const fn view(&self) -> SourceView<'source> {
        self.view
    }

    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub(crate) fn source_word_token(&self) -> Token {
        self.source_word_token
    }

    pub(crate) fn statement_reader_mut(&mut self) -> &mut SourceStatementReader<'source> {
        &mut self.reader
    }

    pub(crate) fn block_reader_mut(&mut self) -> Option<&mut SourceBlockReader<'source, 'state>> {
        self.block_reader.as_mut()
    }

    pub(crate) const fn local_line_number_prefix(&self) -> Option<SourceSpan> {
        self.local_line_number_prefix
    }

    pub(crate) fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<(), SourceWordError> {
        self.code
            .append_mapped(instruction, span)
            .map(|_| ())
            .map_err(|source| SourceWordError::InstructionBuild { source })
    }

    pub(crate) fn publish_global_variable(
        &mut self,
        name: NormalizedName,
        span: SourceSpan,
    ) -> Result<(), SourceWordError> {
        let Some(globals) = &mut self.globals else {
            return Err(SourceWordError::VarPublicationContextUnavailable);
        };

        let bindings = match &mut self.bindings {
            NativeSourceWordBindingAccess::Read(_) => {
                return Err(SourceWordError::VarPublicationContextUnavailable);
            }
            NativeSourceWordBindingAccess::Write(bindings) => &mut **bindings,
        };

        bindings
            .validate_new_name(&name)
            .map_err(|source| match source {
                BindingInsertError::NameConflict => SourceWordError::VarNameConflict { span },
                BindingInsertError::ReservedName => SourceWordError::VarReservedName { span },
            })?;

        let id = globals.allocate();
        // #1370/#1478/#1487 make binding insertion the VAR commit point:
        // after this succeeds, no recoverable fallible work may remain here.
        bindings
            .insert_new(name, Binding::Variable(id))
            .map_err(|source| match source {
                BindingInsertError::NameConflict => {
                    SourceWordError::VarBindingCommitInvariantViolated { span }
                }
                BindingInsertError::ReservedName => {
                    SourceWordError::VarBindingCommitInvariantViolated { span }
                }
            })
    }

    pub(crate) fn validate_runtime_definition_name(
        &self,
        name: &NormalizedName,
        span: SourceSpan,
    ) -> Result<(), SourceWordError> {
        self.bindings()
            .validate_new_name(name)
            .map_err(|source| match source {
                BindingInsertError::NameConflict => SourceWordError::DefNameConflict { span },
                BindingInsertError::ReservedName => SourceWordError::DefReservedName { span },
            })
    }

    pub(crate) fn has_runtime_definition_publication(&self) -> bool {
        self.runtime_definitions.is_some()
            && matches!(&self.bindings, NativeSourceWordBindingAccess::Write(_))
    }

    pub(crate) fn publish_runtime_definition(
        &mut self,
        name: NormalizedName,
        name_span: SourceSpan,
        body: &[SourceBlockStatement<'source>],
        end_span: SourceSpan,
    ) -> Result<WordId, SourceWordError> {
        let Some(publisher) = &mut self.runtime_definitions else {
            return Err(SourceWordError::DefPublicationContextUnavailable {
                span: self.source_word_token.span(),
            });
        };
        let NativeSourceWordBindingAccess::Write(bindings) = &mut self.bindings else {
            return Err(SourceWordError::DefPublicationContextUnavailable {
                span: self.source_word_token.span(),
            });
        };

        publisher.publish_runtime_definition(bindings, name, name_span, body, end_span)
    }

    pub(crate) fn resolve_variable_target(
        &self,
        source_name: &str,
    ) -> Result<crate::global_variable::GlobalVarId, ExpressionVariableErrorKind> {
        resolve_variable_name(self.bindings(), source_name)
    }

    pub(crate) fn stage_expression(
        &self,
        tokens: &[Token],
        anchor: SourceSpan,
    ) -> Result<ExpressionStaging, SourceWordError> {
        let Some(operators) = self.operators else {
            return Err(SourceWordError::LetExpressionContextUnavailable { span: anchor });
        };

        let mut expression_tokens = tokens
            .iter()
            .copied()
            .filter(|token| token.kind() != TokenKind::LineBoundary)
            .collect::<Vec<_>>();
        let end = expression_tokens
            .last()
            .map_or(anchor.end(), |token| token.span().end());
        expression_tokens.push(Token::new(
            TokenKind::Eof,
            self.view.span(self.source_id, end, end).map_err(|source| {
                SourceWordError::Expression {
                    source: ExpressionError::Source(source),
                }
            })?,
        ));

        let resolver = |source_name: &str| resolve_variable_name(self.bindings(), source_name);
        parse_expression(self.view, &expression_tokens, operators, &resolver)
            .map_err(|source| SourceWordError::Expression { source })
    }

    pub(crate) fn commit_staging(
        &mut self,
        staging: &ExpressionStaging,
    ) -> Result<(), SourceWordError> {
        staging.commit_to(self.code).map_err(|source| match source {
            ExpressionError::InstructionBuild(source) => {
                SourceWordError::InstructionBuild { source }
            }
            source => SourceWordError::Expression { source },
        })
    }

    fn bindings(&self) -> &Bindings {
        match &self.bindings {
            NativeSourceWordBindingAccess::Read(bindings) => bindings,
            NativeSourceWordBindingAccess::Write(bindings) => bindings,
        }
    }
}

pub(crate) enum NativeSourceWordBindingAccess<'a> {
    Read(&'a Bindings),
    Write(&'a mut Bindings),
}

pub(crate) fn var_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    if let Some(span) = context.local_line_number_prefix() {
        return Err(SourceWordError::VarLocalLineNumberPrefix { span });
    }

    let reader = context.statement_reader_mut();
    let name_token = reader.read_name().map_err(var_reader_error)?;
    reader.finish().map_err(var_reader_error)?;

    let source_name = context
        .view()
        .slice(name_token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let name = NormalizedName::new(source_name).map_err(|source| SourceWordError::VarName {
        span: name_token.span(),
        source,
    })?;

    context.publish_global_variable(name, name_token.span())
}

pub(crate) fn let_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    let (target_token, equal_span, rhs_tokens) = {
        let reader = context.statement_reader_mut();
        let target_token = reader.read_name().map_err(let_reader_error)?;
        let equal_token = reader.expect(TokenKind::Equal).map_err(let_reader_error)?;
        let rhs_tokens = reader.remaining_expression().map_err(let_reader_error)?;
        (target_token, equal_token.span(), rhs_tokens)
    };

    let source_name = context
        .view()
        .slice(target_token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let target = context
        .resolve_variable_target(source_name)
        .map_err(|source| SourceWordError::LetTarget {
            span: target_token.span(),
            source,
        })?;

    let mut staging = context.stage_expression(rhs_tokens, equal_span)?;
    staging.append_mapped_instruction(Instruction::StoreVar(target), target_token.span());
    context.commit_staging(&staging)
}

pub(crate) fn def_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    let name_token = {
        let reader = context.statement_reader_mut();
        let name_token = reader.read_name().map_err(def_reader_error)?;
        reader.finish().map_err(def_reader_error)?;
        name_token
    };

    let source_name = context
        .view()
        .slice(name_token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let name = NormalizedName::new(source_name).map_err(|source| SourceWordError::DefName {
        span: name_token.span(),
        source,
    })?;
    context.validate_runtime_definition_name(&name, name_token.span())?;
    if !context.has_runtime_definition_publication() {
        return Err(SourceWordError::DefPublicationContextUnavailable {
            span: context.source_word_token().span(),
        });
    }

    let view = context.view();
    let mut body = Vec::new();
    let end_span = loop {
        let read = {
            let Some(reader) = context.block_reader_mut() else {
                return Err(SourceWordError::DefPublicationContextUnavailable {
                    span: context.source_word_token().span(),
                });
            };
            reader.next_statement()?
        };

        match read {
            SourceBlockRead::Statement(statement) if is_standalone_end(view, statement)? => {
                break statement.span();
            }
            SourceBlockRead::Statement(statement) => body.push(statement),
            SourceBlockRead::Terminal(SourceBlockTerminal::Eof { span }) => {
                return Err(SourceWordError::DefMissingEnd { span });
            }
            SourceBlockRead::Terminal(SourceBlockTerminal::LexError { error }) => {
                return Err(SourceWordError::DefLex { source: error });
            }
        }
    };

    context.publish_runtime_definition(name, name_token.span(), &body, end_span)?;
    Ok(())
}

pub(crate) fn unsupported_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    let first = context.source_word_token();
    Err(SourceWordError::UnsupportedSourceWord { span: first.span() })
}

fn is_standalone_end(
    view: SourceView<'_>,
    statement: SourceBlockStatement<'_>,
) -> Result<bool, SourceWordError> {
    let Some(token) = statement.standalone_name() else {
        return Ok(false);
    };
    let source_name = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let Ok(name) = NormalizedName::new(source_name) else {
        return Ok(false);
    };
    Ok(name.as_str() == "END")
}

fn var_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::VarSyntax {
            span,
            kind: VarSyntaxErrorKind::MissingName,
        },
        SourceStatementReaderError::Unexpected { actual, .. } => SourceWordError::VarSyntax {
            span: actual.span(),
            kind: VarSyntaxErrorKind::MissingName,
        },
        SourceStatementReaderError::TrailingToken { actual } => SourceWordError::VarSyntax {
            span: actual.span(),
            kind: VarSyntaxErrorKind::TrailingToken {
                kind: actual.kind(),
            },
        },
    }
}

fn def_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::DefSyntax {
            span,
            kind: DefSyntaxErrorKind::MissingName,
        },
        SourceStatementReaderError::Unexpected { actual, .. } => SourceWordError::DefSyntax {
            span: actual.span(),
            kind: DefSyntaxErrorKind::MissingName,
        },
        SourceStatementReaderError::TrailingToken { actual } => SourceWordError::DefSyntax {
            span: actual.span(),
            kind: DefSyntaxErrorKind::TrailingToken {
                kind: actual.kind(),
            },
        },
    }
}

fn let_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    let (span, kind) = match error {
        SourceStatementReaderError::Missing { expected, span } => {
            (span, let_syntax_kind_for_missing(expected))
        }
        SourceStatementReaderError::Unexpected { expected, actual } => {
            (actual.span(), let_syntax_kind_for_unexpected(expected))
        }
        SourceStatementReaderError::TrailingToken { actual } => {
            (actual.span(), LetSyntaxErrorKind::Rhs)
        }
    };
    SourceWordError::LetSyntax { span, kind }
}

fn let_syntax_kind_for_missing(expected: SourceStatementExpected) -> LetSyntaxErrorKind {
    match expected {
        SourceStatementExpected::Name => LetSyntaxErrorKind::Target,
        SourceStatementExpected::Token(TokenKind::Equal) => LetSyntaxErrorKind::Equal,
        SourceStatementExpected::Token(_) | SourceStatementExpected::Expression => {
            LetSyntaxErrorKind::Rhs
        }
    }
}

fn let_syntax_kind_for_unexpected(expected: SourceStatementExpected) -> LetSyntaxErrorKind {
    match expected {
        SourceStatementExpected::Name => LetSyntaxErrorKind::Target,
        SourceStatementExpected::Token(TokenKind::Equal) => LetSyntaxErrorKind::Equal,
        SourceStatementExpected::Token(_) | SourceStatementExpected::Expression => {
            LetSyntaxErrorKind::Rhs
        }
    }
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

#[derive(Debug, Default)]
pub(crate) struct SourceWordRegistry {
    entries: Vec<SourceWordEntry>,
}

#[derive(Debug, Clone)]
struct SourceWordEntry {
    handler: NativeSourceWordHandler,
    syntax_markers: Vec<SourceWordSyntaxMarker>,
}

impl SourceWordRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, handler: NativeSourceWordHandler) -> SourceWordId {
        self.register_with_markers(handler, Vec::new())
    }

    pub(crate) fn register_with_markers(
        &mut self,
        handler: NativeSourceWordHandler,
        syntax_markers: Vec<SourceWordSyntaxMarker>,
    ) -> SourceWordId {
        let id = SourceWordId::from_slot(self.entries.len());
        self.entries.push(SourceWordEntry {
            handler,
            syntax_markers,
        });
        id
    }

    pub(crate) fn lookup(&self) -> SourceWordLookup<'_> {
        SourceWordLookup { registry: self }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceWordLookup<'a> {
    registry: &'a SourceWordRegistry,
}

impl<'a> SourceWordLookup<'a> {
    pub(crate) fn lookup_handler(
        self,
        id: SourceWordId,
    ) -> Result<NativeSourceWordHandler, SourceWordLookupError> {
        self.registry
            .entries
            .get(id.as_slot())
            .map(|entry| entry.handler)
            .ok_or(SourceWordLookupError::InvalidSourceWordId { id })
    }

    pub(crate) fn syntax_markers(
        self,
        id: SourceWordId,
    ) -> Result<&'a [SourceWordSyntaxMarker], SourceWordLookupError> {
        self.registry
            .entries
            .get(id.as_slot())
            .map(|entry| entry.syntax_markers.as_slice())
            .ok_or(SourceWordLookupError::InvalidSourceWordId { id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_code::BlockCodeBuilder;
    use crate::source::SourceTexts;
    use crate::source_mapping::SourceMappedCode;
    use crate::value::Value;

    fn statement_tokens(text: &str) -> (SourceTexts, SourceId, Vec<Token>) {
        let mut sources = SourceTexts::new();
        let source_id = sources.register(text);
        let mut lexer = crate::lexer::Lexer::new(sources.view(), source_id)
            .expect("test source should create lexer");
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token().expect("test source should lex");
            if token.kind() == TokenKind::Eof {
                break;
            }
            tokens.push(token);
        }
        (sources, source_id, tokens)
    }

    struct OneStatementCursor<'source> {
        next: Option<SourceBlockStatement<'source>>,
        eof_span: SourceSpan,
    }

    impl<'source> OneStatementCursor<'source> {
        fn new(statement: SourceBlockStatement<'source>, eof_span: SourceSpan) -> Self {
            Self {
                next: Some(statement),
                eof_span,
            }
        }
    }

    impl<'source> SourceBlockCursor<'source> for OneStatementCursor<'source> {
        fn read_next_block_statement(
            &mut self,
        ) -> Result<SourceBlockRead<'source>, SourceWordError> {
            Ok(self.next.take().map_or(
                SourceBlockRead::Terminal(SourceBlockTerminal::Eof {
                    span: self.eof_span,
                }),
                SourceBlockRead::Statement,
            ))
        }
    }

    fn syntax_marker(input: &str, role: SourceWordSyntaxMarkerRole) -> SourceWordSyntaxMarker {
        SourceWordSyntaxMarker::new(
            NormalizedName::new(input).expect("test marker name should normalize"),
            role,
        )
    }

    fn with_next_block_item(
        text: &str,
        syntax_markers: &[SourceWordSyntaxMarker],
        inspect: impl FnOnce(SourceView<'_>, SourceId, &[Token], SourceBlockItem<'_>),
    ) {
        let (sources, source_id, tokens) = statement_tokens(text);
        let statement_span = span(sources.view(), source_id, 0, text.len());
        let statement = SourceBlockStatement::new(&tokens, statement_span);
        let mut cursor = OneStatementCursor::new(statement, statement_span);
        let mut reader = SourceBlockReader::new(sources.view(), &mut cursor, syntax_markers);

        let item = reader
            .next_item()
            .expect("test block item should classify without error");

        inspect(sources.view(), source_id, &tokens, item);
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn push_one(context: &mut NativeSourceWordContext<'_, '_>) -> Result<(), SourceWordError> {
        let first = context.source_word_token();
        context.append_mapped(Instruction::Push(Value::integer(1)), first.span())
    }

    #[test]
    fn block_reader_classifies_standalone_leading_marker_with_empty_remainder() {
        let markers = [syntax_marker(
            "ELSE",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )];

        with_next_block_item("ELSE", &markers, |view, source_id, _tokens, item| {
            let SourceBlockItem::Marker(marker) = item else {
                panic!("standalone leading marker should classify");
            };

            assert_eq!(marker.name().as_str(), "ELSE");
            assert_eq!(marker.role(), SourceWordSyntaxMarkerRole::BlockContinuation);
            assert_eq!(marker.statement().span(), span(view, source_id, 0, 4));
            assert_eq!(marker.span(), span(view, source_id, 0, 4));
            assert_eq!(marker.token().span(), span(view, source_id, 0, 4));
            assert!(marker.remaining_tokens().is_empty());
        });
    }

    #[test]
    fn block_reader_classifies_leading_marker_with_nonempty_remainder() {
        let markers = [syntax_marker(
            "ELSIF",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )];

        with_next_block_item("ELSIF X > 0", &markers, |view, source_id, _tokens, item| {
            let SourceBlockItem::Marker(marker) = item else {
                panic!("leading marker with payload should classify");
            };

            assert_eq!(marker.name().as_str(), "ELSIF");
            assert_eq!(marker.statement().span(), span(view, source_id, 0, 11));
            assert_eq!(marker.span(), span(view, source_id, 0, 11));
            assert_eq!(marker.token().span(), span(view, source_id, 0, 5));
            assert_eq!(
                marker
                    .remaining_tokens()
                    .iter()
                    .map(|token| token.kind())
                    .collect::<Vec<_>>(),
                [
                    TokenKind::Name,
                    TokenKind::Greater,
                    TokenKind::IntegerLiteral
                ]
            );
            assert_eq!(
                marker.remaining_tokens().first().map(|token| token.span()),
                Some(span(view, source_id, 6, 7))
            );
        });
    }

    #[test]
    fn block_reader_uses_only_leading_name_for_marker_identity() {
        let markers = [syntax_marker(
            "CASE",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )];

        with_next_block_item("CASE 1 +", &markers, |_view, _source_id, _tokens, item| {
            let SourceBlockItem::Marker(marker) = item else {
                panic!("payload syntax should not affect marker identity");
            };

            assert_eq!(marker.name().as_str(), "CASE");
            assert_eq!(marker.remaining_tokens().len(), 2);
        });
    }

    #[test]
    fn block_reader_does_not_classify_undeclared_leading_name() {
        let markers = [syntax_marker(
            "ELSE",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )];

        with_next_block_item("ELSIF X", &markers, |view, source_id, tokens, item| {
            let SourceBlockItem::Statement(statement) = item else {
                panic!("undeclared leading name should remain a statement");
            };

            assert_eq!(statement.tokens(), tokens);
            assert_eq!(statement.span(), span(view, source_id, 0, 7));
        });
    }

    #[test]
    fn block_reader_does_not_scan_marker_name_after_statement_start() {
        let markers = [syntax_marker(
            "ELSE",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )];

        with_next_block_item(
            "PRINT ELSE",
            &markers,
            |_view, _source_id, _tokens, item| {
                assert!(matches!(item, SourceBlockItem::Statement(_)));
            },
        );
    }

    #[test]
    fn block_reader_does_not_classify_name_after_line_number_prefix() {
        let markers = [syntax_marker(
            "ELSE",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )];

        with_next_block_item("100 ELSE", &markers, |_view, _source_id, _tokens, item| {
            assert!(matches!(item, SourceBlockItem::Statement(_)));
        });
    }

    #[test]
    fn block_reader_ignores_marker_declared_only_by_outer_owner() {
        let child_markers = [syntax_marker(
            "END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )];

        with_next_block_item(
            "ELSE X",
            &child_markers,
            |_view, _source_id, _tokens, item| {
                assert!(matches!(item, SourceBlockItem::Statement(_)));
            },
        );
    }

    #[test]
    fn native_context_lends_one_reader_without_resetting_position() {
        let (sources, source_id, tokens) = statement_tokens("TEST A B");
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);
        let bindings = Bindings::new();
        let mut context = NativeSourceWordContext::new(NativeSourceWordContextParts {
            view: sources.view(),
            source_id,
            tokens: &tokens,
            block_reader: None,
            bindings: NativeSourceWordBindingAccess::Read(&bindings),
            operators: None,
            code: &mut builder,
            local_line_number_prefix: None,
            globals: None,
            runtime_definitions: None,
        });

        let first_body_token = context
            .statement_reader_mut()
            .read_name()
            .expect("first body name should be read");
        let second_body_token = context
            .statement_reader_mut()
            .read_name()
            .expect("second borrow should continue from current position");

        assert_eq!(
            first_body_token.span(),
            span(sources.view(), source_id, 5, 6)
        );
        assert_eq!(
            second_body_token.span(),
            span(sources.view(), source_id, 7, 8)
        );
        context
            .statement_reader_mut()
            .finish()
            .expect("reader should be exhausted after both body names");
    }

    #[test]
    fn statement_reader_reads_name_and_expected_token_sequentially() {
        let (sources, source_id, tokens) = statement_tokens("LET Score = 1");
        let mut reader = SourceStatementReader::new(&tokens[1..], tokens[0].span());

        let name = reader.read_name().expect("name should be read");
        let equal = reader.expect(TokenKind::Equal).expect("'=' should be read");

        assert_eq!(name.kind(), TokenKind::Name);
        assert_eq!(name.span(), span(sources.view(), source_id, 4, 9));
        assert_eq!(equal.kind(), TokenKind::Equal);
        assert_eq!(equal.span(), span(sources.view(), source_id, 10, 11));
    }

    #[test]
    fn statement_reader_reports_token_kind_mismatch_at_actual_span() {
        let (sources, source_id, tokens) = statement_tokens("LET A 1");
        let mut reader = SourceStatementReader::new(&tokens[1..], tokens[0].span());

        reader.read_name().expect("name should be read");
        let error = reader
            .expect(TokenKind::Equal)
            .expect_err("integer should not satisfy '='");

        assert_eq!(
            error,
            SourceStatementReaderError::Unexpected {
                expected: SourceStatementExpected::Token(TokenKind::Equal),
                actual: Token::new(
                    TokenKind::IntegerLiteral,
                    span(sources.view(), source_id, 6, 7)
                ),
            }
        );
    }

    #[test]
    fn statement_reader_reports_missing_required_token_at_previous_span() {
        let (sources, source_id, tokens) = statement_tokens("LET A");
        let mut reader = SourceStatementReader::new(&tokens[1..], tokens[0].span());

        reader.read_name().expect("name should be read");
        let error = reader
            .expect(TokenKind::Equal)
            .expect_err("missing '=' should fail");

        assert_eq!(
            error,
            SourceStatementReaderError::Missing {
                expected: SourceStatementExpected::Token(TokenKind::Equal),
                span: span(sources.view(), source_id, 4, 5),
            }
        );
    }

    #[test]
    fn statement_reader_returns_remaining_expression_and_finishes() {
        let (_sources, _source_id, tokens) = statement_tokens("LET A = 1 + 2 * 3");
        let mut reader = SourceStatementReader::new(&tokens[1..], tokens[0].span());

        reader.read_name().expect("name should be read");
        reader.expect(TokenKind::Equal).expect("'=' should be read");
        let rhs = reader
            .remaining_expression()
            .expect("remaining RHS should exist");

        assert_eq!(
            rhs.iter().map(|token| token.kind()).collect::<Vec<_>>(),
            [
                TokenKind::IntegerLiteral,
                TokenKind::Plus,
                TokenKind::IntegerLiteral,
                TokenKind::Star,
                TokenKind::IntegerLiteral,
            ]
        );
        assert!(reader.is_exhausted());
        reader
            .finish()
            .expect("remaining expression consumes to end");
    }

    #[test]
    fn statement_reader_rejects_empty_remaining_expression() {
        let (sources, source_id, tokens) = statement_tokens("LET A =");
        let mut reader = SourceStatementReader::new(&tokens[1..], tokens[0].span());

        reader.read_name().expect("name should be read");
        reader.expect(TokenKind::Equal).expect("'=' should be read");
        let error = reader
            .remaining_expression()
            .expect_err("empty RHS should fail");

        assert_eq!(
            error,
            SourceStatementReaderError::Missing {
                expected: SourceStatementExpected::Expression,
                span: span(sources.view(), source_id, 6, 7),
            }
        );
    }

    #[test]
    fn statement_reader_finish_rejects_trailing_token() {
        let (sources, source_id, tokens) = statement_tokens("VAR SCORE EXTRA");
        let mut reader = SourceStatementReader::new(&tokens[1..], tokens[0].span());

        reader.read_name().expect("name should be read");
        let error = reader.finish().expect_err("trailing token should fail");

        assert_eq!(
            error,
            SourceStatementReaderError::TrailingToken {
                actual: Token::new(TokenKind::Name, span(sources.view(), source_id, 10, 15)),
            }
        );
    }

    #[test]
    fn registry_allocates_monotonic_source_word_ids_without_word_ids() {
        let mut registry = SourceWordRegistry::new();

        let first = registry.register(push_one);
        let second = registry.register(push_one);

        assert_eq!(first.as_slot(), 0);
        assert_eq!(second.as_slot(), 1);
        assert_ne!(first, second);
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
    }

    #[test]
    fn read_only_lookup_resolves_registered_handler() {
        let mut registry = SourceWordRegistry::new();
        let id = registry.register(push_one);

        assert!(registry.lookup().lookup_handler(id).is_ok());
    }

    #[test]
    fn read_only_lookup_rejects_unregistered_source_word_id() {
        let registry = SourceWordRegistry::new();
        let id = SourceWordId::from_slot(0);

        assert_eq!(
            registry.lookup().lookup_handler(id),
            Err(SourceWordLookupError::InvalidSourceWordId { id })
        );
    }

    #[test]
    fn native_context_emits_mapped_temporary_instruction() {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("TEST");
        let mut lexer = crate::lexer::Lexer::new(sources.view(), source_id)
            .expect("test source should create lexer");
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token().expect("test source should lex");
            tokens.push(token);
            if token.kind() == crate::lexer::TokenKind::Eof {
                break;
            }
        }
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);
        let bindings = Bindings::new();
        {
            let mut context = NativeSourceWordContext::new(NativeSourceWordContextParts {
                view: sources.view(),
                source_id,
                tokens: &tokens[..1],
                block_reader: None,
                bindings: NativeSourceWordBindingAccess::Read(&bindings),
                operators: None,
                code: &mut builder,
                local_line_number_prefix: None,
                globals: None,
                runtime_definitions: None,
            });

            push_one(&mut context).expect("test source word should emit");
        }
        builder.finish().expect("block should complete");

        assert_eq!(
            code.instruction_view()
                .get(crate::instruction::InstructionAddress::from_index(0)),
            Ok(&Instruction::Push(Value::integer(1)))
        );
    }

    #[test]
    fn var_reserved_name_is_rejected_without_allocating_global_slot() {
        for input in ["VAR END", "VAR end", "VAR End"] {
            let (sources, source_id, tokens) = statement_tokens(input);
            let mut code = SourceMappedCode::new();
            let mut builder = BlockCodeBuilder::new(&mut code);
            let mut bindings = Bindings::new();
            let mut globals = GlobalVariables::new();
            let expected_span = span(sources.view(), source_id, 4, 7);

            let result = {
                let mut context = NativeSourceWordContext::new(NativeSourceWordContextParts {
                    view: sources.view(),
                    source_id,
                    tokens: &tokens,
                    block_reader: None,
                    bindings: NativeSourceWordBindingAccess::Write(&mut bindings),
                    operators: None,
                    code: &mut builder,
                    local_line_number_prefix: None,
                    globals: Some(&mut globals),
                    runtime_definitions: None,
                });

                var_source_word(&mut context)
            };

            assert_eq!(
                result,
                Err(SourceWordError::VarReservedName {
                    span: expected_span
                })
            );
            assert!(globals.is_empty());
            assert!(bindings.is_empty());
        }
    }
}
