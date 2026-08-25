use std::collections::HashMap;

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
use crate::source_word_evaluator::{
    evaluate_source_word_with_state, SourceWordEvaluationError, SourceWordEvaluationState,
    UserDefinedSourceWordContext, UserDefinedSourceWordContextParts,
};
use crate::source_word_ir::{
    FixedToken, LocalBinding, LocalReference, SourceInstructionOrigin,
    SourceProcessingCapabilities, SourceProcessingInstruction, SourceProcessingOperation,
    SourceWordBuildError, SourceWordImplementation, SourceWordImplementationBuilder,
};
use crate::structured_grammar::{
    MarkerCardinality, MarkerGroup, MarkerIdentity, StructuredGrammar,
};
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

pub(crate) type NativeStructuredSourceWordStartHandler =
    fn(
        &mut NativeSourceWordContext<'_, '_>,
    ) -> Result<StructuredSourceWordInstance, SourceWordError>;

#[derive(Debug, Clone)]
pub(crate) struct UserDefinedStructuredSourceWordImplementation {
    start: SourceWordImplementation,
    markers: Vec<UserDefinedStructuredMarkerImplementation>,
    terminator: UserDefinedStructuredTerminatorImplementation,
}

#[derive(Debug, Clone)]
pub(crate) struct UserDefinedStructuredMarkerImplementation {
    name: NormalizedName,
    implementation: SourceWordImplementation,
}

#[derive(Debug, Clone)]
pub(crate) struct UserDefinedStructuredTerminatorImplementation {
    name: NormalizedName,
    implementation: SourceWordImplementation,
}

pub(crate) trait NativeStructuredSourceWordOwner: std::fmt::Debug {
    fn current_body_context(&self) -> StructuredBodyContext;

    fn accept_marker<'source>(
        &mut self,
        context: &mut NativeStructuredSourceWordContext<'source, '_>,
        marker: SourceBlockMarker<'source>,
        accept: crate::structured_grammar::GrammarAccept,
    ) -> Result<(), SourceWordError>;

    fn complete<'source>(
        &mut self,
        context: &mut NativeStructuredSourceWordContext<'source, '_>,
        marker: SourceBlockMarker<'source>,
    ) -> Result<(), SourceWordError>;
}

#[derive(Debug)]
pub(crate) struct StructuredSourceWordInstance {
    owner: Box<dyn NativeStructuredSourceWordOwner>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StructuredBodyContext {
    build_target: StructuredBuildTargetScope,
    line_number_scope: StructuredLineNumberScope,
    capabilities: StructuredBodyCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredBuildTargetScope {
    Enclosing,
    OwnerLocal(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredLineNumberScope {
    Enclosing,
    OwnerLocal(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StructuredBodyCapabilities {
    publication: bool,
}

pub(crate) struct NativeStructuredSourceWordContext<'source, 'state> {
    view: SourceView<'source>,
    source_id: SourceId,
    bindings: &'state Bindings,
    operators: Option<OperatorLookup>,
    code: &'state mut dyn InstructionBuildTarget,
    line_numbers: &'state mut crate::line_number::LocalLineNumberTable,
    capabilities: SourceProcessingCapabilities,
    owner_local_targets: Vec<StructuredOwnerLocalTarget>,
}

pub(crate) struct NativeStructuredSourceWordContextParts<'source, 'state> {
    pub(crate) view: SourceView<'source>,
    pub(crate) source_id: SourceId,
    pub(crate) bindings: &'state Bindings,
    pub(crate) operators: Option<OperatorLookup>,
    pub(crate) code: &'state mut dyn InstructionBuildTarget,
    pub(crate) line_numbers: &'state mut crate::line_number::LocalLineNumberTable,
    pub(crate) capabilities: SourceProcessingCapabilities,
    pub(crate) owner_local_targets: Vec<StructuredOwnerLocalTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructuredOwnerLocalTarget {
    instructions: Vec<StructuredOwnerLocalInstruction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StructuredOwnerLocalInstruction {
    instruction: Instruction,
    span: Option<SourceSpan>,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    IfSyntax {
        span: SourceSpan,
        kind: IfSyntaxErrorKind,
    },
    SyntaxDefinition {
        span: SourceSpan,
        kind: SyntaxDefinitionErrorKind,
    },
    SyntaxName {
        span: SourceSpan,
        source: NameError,
    },
    SyntaxNameConflict {
        span: SourceSpan,
    },
    SyntaxReservedName {
        span: SourceSpan,
    },
    SyntaxPublicationContextUnavailable {
        span: SourceSpan,
    },
    SyntaxBindingCommitInvariantViolated {
        span: SourceSpan,
    },
    SyntaxBuild {
        source: SourceWordBuildError,
    },
    UserDefinedEvaluation {
        source: SourceWordEvaluationError,
    },
    StructuredGrammar {
        span: SourceSpan,
        source: crate::structured_grammar::GrammarProgressError,
    },
    StructuredMissingTerminator {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IfSyntaxErrorKind {
    MissingCondition,
    TrailingToken { kind: TokenKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxDefinitionErrorKind {
    MissingName,
    TrailingToken { kind: TokenKind },
    MissingKind,
    UnsupportedKind,
    MissingEnds,
    UnknownOperation,
    MissingOperand,
    ExpectedAs,
    ExpectedFixedToken,
    TrailingOperationToken { kind: TokenKind },
}

impl SourceWordError {
    pub(crate) fn primary_span(&self) -> Option<SourceSpan> {
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
            | Self::DefBindingCommitInvariantViolated { span }
            | Self::IfSyntax { span, .. }
            | Self::SyntaxDefinition { span, .. }
            | Self::SyntaxName { span, .. }
            | Self::SyntaxNameConflict { span }
            | Self::SyntaxReservedName { span }
            | Self::SyntaxPublicationContextUnavailable { span }
            | Self::SyntaxBindingCommitInvariantViolated { span }
            | Self::StructuredGrammar { span, .. }
            | Self::StructuredMissingTerminator { span } => Some(*span),
            Self::SyntaxBuild { source } => Some(source.primary_span()),
            Self::UserDefinedEvaluation { source } => source.primary_span(),
            Self::DefLex { source } => match source {
                LexError::Source(_) => None,
                LexError::InvalidCharacter { span, .. } => Some(*span),
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
    pub(crate) fn new(
        statement: SourceBlockStatement<'source>,
        token: Token,
        name: NormalizedName,
        role: SourceWordSyntaxMarkerRole,
    ) -> Self {
        Self {
            statement,
            token,
            name,
            role,
        }
    }

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

impl StructuredSourceWordInstance {
    pub(crate) fn new(owner: Box<dyn NativeStructuredSourceWordOwner>) -> Self {
        Self { owner }
    }

    pub(crate) fn into_owner(self) -> Box<dyn NativeStructuredSourceWordOwner> {
        self.owner
    }
}

impl StructuredBodyContext {
    pub(crate) const fn inherited() -> Self {
        Self {
            build_target: StructuredBuildTargetScope::Enclosing,
            line_number_scope: StructuredLineNumberScope::Enclosing,
            capabilities: StructuredBodyCapabilities::inherit(),
        }
    }

    pub(crate) const fn new(
        build_target: StructuredBuildTargetScope,
        line_number_scope: StructuredLineNumberScope,
        capabilities: StructuredBodyCapabilities,
    ) -> Self {
        Self {
            build_target,
            line_number_scope,
            capabilities,
        }
    }

    pub(crate) const fn build_target(self) -> StructuredBuildTargetScope {
        self.build_target
    }

    pub(crate) const fn line_number_scope(self) -> StructuredLineNumberScope {
        self.line_number_scope
    }

    pub(crate) const fn capabilities(self) -> StructuredBodyCapabilities {
        self.capabilities
    }
}

impl StructuredBodyCapabilities {
    pub(crate) const fn inherit() -> Self {
        Self { publication: true }
    }

    pub(crate) const fn without_publication() -> Self {
        Self { publication: false }
    }

    pub(crate) const fn allows_publication(self) -> bool {
        self.publication
    }

    pub(crate) const fn intersect(self, enclosing: Self) -> Self {
        Self {
            publication: self.publication && enclosing.publication,
        }
    }
}

impl<'source, 'state> NativeStructuredSourceWordContext<'source, 'state> {
    pub(crate) fn new(parts: NativeStructuredSourceWordContextParts<'source, 'state>) -> Self {
        Self {
            view: parts.view,
            source_id: parts.source_id,
            bindings: parts.bindings,
            operators: parts.operators,
            code: parts.code,
            line_numbers: parts.line_numbers,
            capabilities: parts.capabilities,
            owner_local_targets: parts.owner_local_targets,
        }
    }

    pub(crate) const fn view(&self) -> SourceView<'source> {
        self.view
    }

    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
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

    pub(crate) fn current_address(&self) -> crate::instruction::InstructionAddress {
        self.code.current_address()
    }

    pub(crate) fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<crate::instruction::InstructionAddress, SourceWordError> {
        self.code
            .append_mapped_jump_placeholder(span)
            .map_err(|source| SourceWordError::InstructionBuild { source })
    }

    pub(crate) fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<crate::instruction::InstructionAddress, SourceWordError> {
        self.code
            .append_mapped_jump_if_zero_placeholder(span)
            .map_err(|source| SourceWordError::InstructionBuild { source })
    }

    pub(crate) fn patch_branch_target(
        &mut self,
        branch: crate::instruction::InstructionAddress,
        target: crate::instruction::InstructionAddress,
    ) -> Result<(), SourceWordError> {
        self.code
            .patch_branch_target(branch, target)
            .map_err(|source| SourceWordError::InstructionBuild { source })
    }

    pub(crate) fn append_owner_local_target(
        &mut self,
        index: usize,
        anchor: SourceSpan,
    ) -> Result<(), SourceWordError> {
        let Some(target) = self.owner_local_targets.get(index) else {
            return Err(SourceWordError::UnsupportedSourceWord { span: anchor });
        };
        target.append_to(self.code, anchor)
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

        let resolver = |source_name: &str| resolve_variable_name(self.bindings, source_name);
        parse_expression(self.view, &expression_tokens, operators, &resolver)
            .map_err(|source| SourceWordError::Expression { source })
    }

    fn evaluate_user_defined_source_word(
        &mut self,
        implementation: &SourceWordImplementation,
        state: &mut SourceWordEvaluationState,
        tokens: &'source [Token],
    ) -> Result<(), SourceWordError> {
        let mut context = UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
            view: self.view,
            source_id: self.source_id,
            tokens,
            bindings: self.bindings,
            operators: self.operators,
            code: self.code,
            line_numbers: self.line_numbers,
            capabilities: self.capabilities,
        });
        evaluate_source_word_with_state(implementation, &mut context, state)
            .map_err(|source| SourceWordError::UserDefinedEvaluation { source })
    }
}

impl UserDefinedStructuredSourceWordImplementation {
    fn new(
        start: SourceWordImplementation,
        markers: Vec<UserDefinedStructuredMarkerImplementation>,
        terminator: UserDefinedStructuredTerminatorImplementation,
    ) -> Self {
        Self {
            start,
            markers,
            terminator,
        }
    }

    pub(crate) fn start(&self) -> &SourceWordImplementation {
        &self.start
    }

    fn marker(&self, group_index: usize) -> Option<&SourceWordImplementation> {
        self.markers
            .get(group_index)
            .map(|marker| &marker.implementation)
    }

    fn terminator(&self) -> &SourceWordImplementation {
        &self.terminator.implementation
    }
}

impl UserDefinedStructuredMarkerImplementation {
    fn new(name: NormalizedName, implementation: SourceWordImplementation) -> Self {
        Self {
            name,
            implementation,
        }
    }
}

impl UserDefinedStructuredTerminatorImplementation {
    fn new(name: NormalizedName, implementation: SourceWordImplementation) -> Self {
        Self {
            name,
            implementation,
        }
    }
}

impl StructuredOwnerLocalTarget {
    pub(crate) fn new(instructions: Vec<(Instruction, Option<SourceSpan>)>) -> Self {
        Self {
            instructions: instructions
                .into_iter()
                .map(|(instruction, span)| StructuredOwnerLocalInstruction { instruction, span })
                .collect(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.instructions.len()
    }

    fn append_to(
        &self,
        code: &mut dyn InstructionBuildTarget,
        anchor: SourceSpan,
    ) -> Result<(), SourceWordError> {
        let parent_start = code.current_address();
        for mapped in &self.instructions {
            let instruction = self.rebase_instruction(mapped.instruction, parent_start, anchor)?;
            if let Some(span) = mapped.span {
                code.append_resolved_mapped(instruction, span)
            } else {
                code.append_resolved_unmapped(instruction)
            }
            .map_err(|source| SourceWordError::InstructionBuild { source })?;
        }
        Ok(())
    }

    fn rebase_instruction(
        &self,
        instruction: Instruction,
        parent_start: crate::instruction::InstructionAddress,
        anchor: SourceSpan,
    ) -> Result<Instruction, SourceWordError> {
        match instruction {
            Instruction::Jump(target) => self
                .rebase_target(target, parent_start, anchor)
                .map(Instruction::Jump),
            Instruction::JumpIfZero(target) => self
                .rebase_target(target, parent_start, anchor)
                .map(Instruction::JumpIfZero),
            Instruction::Push(_)
            | Instruction::LoadVar(_)
            | Instruction::StoreVar(_)
            | Instruction::Call(_)
            | Instruction::Return
            | Instruction::Halt => Ok(instruction),
        }
    }

    fn rebase_target(
        &self,
        target: crate::instruction::InstructionAddress,
        parent_start: crate::instruction::InstructionAddress,
        anchor: SourceSpan,
    ) -> Result<crate::instruction::InstructionAddress, SourceWordError> {
        if target.as_index() > self.instructions.len() {
            return Err(SourceWordError::UnsupportedSourceWord { span: anchor });
        }
        let parent_index = parent_start
            .as_index()
            .checked_add(target.as_index())
            .ok_or(SourceWordError::UnsupportedSourceWord { span: anchor })?;
        Ok(crate::instruction::InstructionAddress::from_index(
            parent_index,
        ))
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
    pub(crate) const fn new(tokens: &'source [Token], missing_anchor: SourceSpan) -> Self {
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

    pub(crate) fn expression_until(
        &mut self,
        delimiter: TokenKind,
    ) -> Result<&'source [Token], SourceStatementReaderError> {
        if self.is_exhausted() {
            return Err(SourceStatementReaderError::Missing {
                expected: SourceStatementExpected::Expression,
                span: self.missing_anchor,
            });
        }

        let start = self.position;
        let mut depth = 0usize;
        while let Some(token) = self.tokens.get(self.position).copied() {
            match token.kind() {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                }
                kind if kind == delimiter && depth == 0 => break,
                _ => {}
            }
            self.position += 1;
        }

        if start == self.position {
            return Err(SourceStatementReaderError::Missing {
                expected: SourceStatementExpected::Expression,
                span: self.missing_anchor,
            });
        }

        Ok(&self.tokens[start..self.position])
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
    source_word_publication: Option<&'state mut SourceWordRegistry>,
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
    pub(crate) source_word_publication: Option<&'state mut SourceWordRegistry>,
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
            source_word_publication: parts.source_word_publication,
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

    pub(crate) fn statement_span(&self) -> Result<SourceSpan, SourceWordError> {
        let last = self
            .reader
            .tokens
            .last()
            .copied()
            .unwrap_or(self.source_word_token);
        self.view
            .span(
                self.source_id,
                self.source_word_token.span().start(),
                last.span().end(),
            )
            .map_err(|source| SourceWordError::Source { source })
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

    pub(crate) fn publish_statement_source_word(
        &mut self,
        name: NormalizedName,
        name_span: SourceSpan,
        implementation: SourceWordImplementation,
    ) -> Result<SourceWordId, SourceWordError> {
        let Some(source_words) = &mut self.source_word_publication else {
            return Err(SourceWordError::SyntaxPublicationContextUnavailable {
                span: self.source_word_token.span(),
            });
        };
        let NativeSourceWordBindingAccess::Write(bindings) = &mut self.bindings else {
            return Err(SourceWordError::SyntaxPublicationContextUnavailable {
                span: self.source_word_token.span(),
            });
        };

        bindings
            .validate_new_source_word_with_markers(&name, &[])
            .map_err(|source| match source {
                BindingInsertError::NameConflict => {
                    SourceWordError::SyntaxNameConflict { span: name_span }
                }
                BindingInsertError::ReservedName => {
                    SourceWordError::SyntaxReservedName { span: name_span }
                }
            })?;

        let id = source_words.register_user_defined_statement(implementation);
        // #1556/#1513 make binding insertion the publication point. The
        // registry entry is unreachable until this succeeds through the shared
        // source-word binding namespace.
        bindings
            .insert_new_source_word_with_markers(name, id, &[])
            .map_err(|_| SourceWordError::SyntaxBindingCommitInvariantViolated {
                span: name_span,
            })?;
        Ok(id)
    }

    pub(crate) fn publish_structured_source_word(
        &mut self,
        name: NormalizedName,
        name_span: SourceSpan,
        grammar: StructuredGrammar,
        syntax_markers: Vec<SourceWordSyntaxMarker>,
        implementation: UserDefinedStructuredSourceWordImplementation,
    ) -> Result<SourceWordId, SourceWordError> {
        let Some(source_words) = &mut self.source_word_publication else {
            return Err(SourceWordError::SyntaxPublicationContextUnavailable {
                span: self.source_word_token.span(),
            });
        };
        let NativeSourceWordBindingAccess::Write(bindings) = &mut self.bindings else {
            return Err(SourceWordError::SyntaxPublicationContextUnavailable {
                span: self.source_word_token.span(),
            });
        };
        let marker_names = syntax_markers
            .iter()
            .map(|marker| marker.name().clone())
            .collect::<Vec<_>>();

        bindings
            .validate_new_source_word_with_markers(&name, &marker_names)
            .map_err(|source| match source {
                BindingInsertError::NameConflict => {
                    SourceWordError::SyntaxNameConflict { span: name_span }
                }
                BindingInsertError::ReservedName => {
                    SourceWordError::SyntaxReservedName { span: name_span }
                }
            })?;

        let id =
            source_words.register_user_defined_structured(grammar, syntax_markers, implementation);
        // #1513/#1556 require the user-defined structured artifact, binding,
        // and marker reservations to become visible as a single unit.
        bindings
            .insert_new_source_word_with_markers(name, id, &marker_names)
            .map_err(|_| SourceWordError::SyntaxBindingCommitInvariantViolated {
                span: name_span,
            })?;
        Ok(id)
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

pub(crate) fn syntax_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    let name_token = {
        let reader = context.statement_reader_mut();
        let name_token = reader.read_name().map_err(syntax_definition_reader_error)?;
        reader.finish().map_err(syntax_definition_reader_error)?;
        name_token
    };

    let source_name = context
        .view()
        .slice(name_token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let name = NormalizedName::new(source_name).map_err(|source| SourceWordError::SyntaxName {
        span: name_token.span(),
        source,
    })?;

    let kind = read_syntax_body_item(context, SyntaxDefinitionErrorKind::MissingKind)?;
    let SourceBlockItem::Marker(marker) = kind else {
        return Err(SourceWordError::SyntaxDefinition {
            span: syntax_item_span(kind, context.source_word_token().span()),
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let view = context.view();
    match marker.name().as_str() {
        "STATEMENT" => {
            publish_statement_syntax_definition(context, view, name, name_token.span(), marker)
        }
        "BLOCK" => publish_block_syntax_definition(context, view, name, name_token.span(), marker),
        _ => Err(SourceWordError::SyntaxDefinition {
            span: marker.span(),
            kind: SyntaxDefinitionErrorKind::UnsupportedKind,
        }),
    }
}

fn publish_statement_syntax_definition(
    context: &mut NativeSourceWordContext<'_, '_>,
    view: SourceView<'_>,
    name: NormalizedName,
    name_span: SourceSpan,
    kind_marker: SourceBlockMarker<'_>,
) -> Result<(), SourceWordError> {
    require_empty_syntax_marker_remainder(
        &kind_marker,
        SyntaxDefinitionErrorKind::UnsupportedKind,
    )?;
    let mut builder = SourceWordImplementationBuilder::new();
    loop {
        match read_syntax_body_item(context, SyntaxDefinitionErrorKind::MissingEnds)? {
            SourceBlockItem::Statement(statement) => {
                builder.push(parse_source_processing_statement(view, statement)?);
            }
            SourceBlockItem::Marker(marker) if marker.name().as_str() == "ENDS" => {
                require_empty_syntax_marker_remainder_with(&marker, |token| {
                    SyntaxDefinitionErrorKind::TrailingOperationToken { kind: token.kind() }
                })?;
                break;
            }
            SourceBlockItem::Marker(marker) => {
                return Err(SourceWordError::SyntaxDefinition {
                    span: marker.span(),
                    kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                });
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => {
                return Err(SourceWordError::SyntaxDefinition {
                    span,
                    kind: SyntaxDefinitionErrorKind::MissingEnds,
                });
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => {
                return Err(SourceWordError::DefLex { source: error });
            }
        }
    }

    let implementation = builder
        .complete()
        .map_err(|source| SourceWordError::SyntaxBuild { source })?;
    context.publish_statement_source_word(name, name_span, implementation)?;
    Ok(())
}

fn publish_block_syntax_definition(
    context: &mut NativeSourceWordContext<'_, '_>,
    view: SourceView<'_>,
    name: NormalizedName,
    name_span: SourceSpan,
    kind_marker: SourceBlockMarker<'_>,
) -> Result<(), SourceWordError> {
    require_empty_syntax_marker_remainder(
        &kind_marker,
        SyntaxDefinitionErrorKind::UnsupportedKind,
    )?;
    let sections = read_block_syntax_sections(context, view)?;
    let artifacts = complete_block_syntax_sections(sections, kind_marker.span())?;
    context.publish_structured_source_word(
        name,
        name_span,
        artifacts.grammar,
        artifacts.syntax_markers,
        artifacts.implementation,
    )?;
    Ok(())
}

#[derive(Debug)]
struct BlockSyntaxArtifacts {
    grammar: StructuredGrammar,
    syntax_markers: Vec<SourceWordSyntaxMarker>,
    implementation: UserDefinedStructuredSourceWordImplementation,
}

#[derive(Debug)]
struct BlockSyntaxSection {
    kind: BlockSyntaxSectionKind,
    header_span: SourceSpan,
    instructions: Vec<SourceProcessingInstruction>,
}

#[derive(Debug)]
enum BlockSyntaxSectionKind {
    Start,
    Marker {
        name: NormalizedName,
        cardinality: MarkerCardinality,
    },
    Last {
        name: NormalizedName,
    },
}

#[derive(Debug, Clone, Copy)]
struct BlockSectionLocalDefinition {
    section_index: usize,
    visible_outside_section: bool,
}

fn read_block_syntax_sections(
    context: &mut NativeSourceWordContext<'_, '_>,
    view: SourceView<'_>,
) -> Result<Vec<BlockSyntaxSection>, SourceWordError> {
    let mut sections: Vec<BlockSyntaxSection> = Vec::new();

    loop {
        match read_syntax_body_item(context, SyntaxDefinitionErrorKind::MissingEnds)? {
            SourceBlockItem::Statement(statement) => {
                let Some(section) = sections.last_mut() else {
                    return Err(SourceWordError::SyntaxDefinition {
                        span: statement.span(),
                        kind: SyntaxDefinitionErrorKind::MissingKind,
                    });
                };
                section
                    .instructions
                    .push(parse_source_processing_statement(view, statement)?);
            }
            SourceBlockItem::Marker(marker) if marker.name().as_str() == "ENDS" => {
                require_empty_syntax_marker_remainder_with(&marker, |token| {
                    SyntaxDefinitionErrorKind::TrailingOperationToken { kind: token.kind() }
                })?;
                break;
            }
            SourceBlockItem::Marker(marker) => {
                sections.push(parse_block_syntax_section_header(view, marker)?);
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => {
                return Err(SourceWordError::SyntaxDefinition {
                    span,
                    kind: SyntaxDefinitionErrorKind::MissingEnds,
                });
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => {
                return Err(SourceWordError::DefLex { source: error });
            }
        }
    }

    Ok(sections)
}

fn parse_block_syntax_section_header(
    view: SourceView<'_>,
    marker: SourceBlockMarker<'_>,
) -> Result<BlockSyntaxSection, SourceWordError> {
    let kind = match marker.name().as_str() {
        "START" => {
            require_empty_syntax_marker_remainder(
                &marker,
                SyntaxDefinitionErrorKind::UnsupportedKind,
            )?;
            BlockSyntaxSectionKind::Start
        }
        "MARK" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::One,
        },
        "MARK_OPTIONAL" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::Optional,
        },
        "MARK_ANY" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::ZeroOrMore,
        },
        "MARK_SOME" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::OneOrMore,
        },
        "LAST" => BlockSyntaxSectionKind::Last {
            name: read_syntax_marker_name(view, &marker)?,
        },
        _ => {
            return Err(SourceWordError::SyntaxDefinition {
                span: marker.span(),
                kind: SyntaxDefinitionErrorKind::UnsupportedKind,
            });
        }
    };
    Ok(BlockSyntaxSection {
        kind,
        header_span: marker.span(),
        instructions: Vec::new(),
    })
}

fn read_syntax_marker_name(
    view: SourceView<'_>,
    marker: &SourceBlockMarker<'_>,
) -> Result<NormalizedName, SourceWordError> {
    let mut reader = SourceStatementReader::new(marker.remaining_tokens(), marker.token().span());
    let token = reader.read_name().map_err(syntax_operation_reader_error)?;
    let name = normalized_token(view, token)?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(name)
}

fn complete_block_syntax_sections(
    sections: Vec<BlockSyntaxSection>,
    fallback_span: SourceSpan,
) -> Result<BlockSyntaxArtifacts, SourceWordError> {
    let Some(BlockSyntaxSection {
        kind: BlockSyntaxSectionKind::Start,
        header_span,
        ..
    }) = sections.first()
    else {
        let span = sections
            .first()
            .map(|section| section.header_span)
            .unwrap_or(fallback_span);
        return Err(SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let start_header_span = *header_span;

    let mut validation = SourceWordImplementationBuilder::new();
    for instruction in sections
        .iter()
        .flat_map(|section| section.instructions.iter().cloned())
    {
        validation.push(instruction);
    }
    validation
        .complete()
        .map_err(|source| SourceWordError::SyntaxBuild { source })?;
    validate_block_section_local_visibility(&sections)
        .map_err(|source| SourceWordError::SyntaxBuild { source })?;

    let mut sections = sections.into_iter();
    let start = sections.next().expect("start section was validated above");
    let start = SourceWordImplementation::from_prevalidated_instructions(start.instructions);
    let mut groups = Vec::new();
    let mut syntax_markers = Vec::new();
    let mut markers = Vec::new();
    let mut terminator = None;
    let mut terminator_implementation = None;

    for section in sections {
        let section_span = section_origin_span(&section);
        match section.kind {
            BlockSyntaxSectionKind::Start => {
                return Err(SourceWordError::SyntaxDefinition {
                    span: section_span,
                    kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                });
            }
            BlockSyntaxSectionKind::Marker { name, cardinality } => {
                if terminator.is_some() {
                    return Err(SourceWordError::SyntaxDefinition {
                        span: section_span,
                        kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                    });
                }
                let marker = MarkerIdentity::new(name.clone());
                groups.push(MarkerGroup::new(marker, cardinality));
                syntax_markers.push(SourceWordSyntaxMarker::new(
                    name.clone(),
                    SourceWordSyntaxMarkerRole::BlockContinuation,
                ));
                markers.push(UserDefinedStructuredMarkerImplementation::new(
                    name,
                    SourceWordImplementation::from_prevalidated_instructions(section.instructions),
                ));
            }
            BlockSyntaxSectionKind::Last { name } => {
                if terminator.is_some() {
                    return Err(SourceWordError::SyntaxDefinition {
                        span: section_span,
                        kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                    });
                }
                terminator = Some(MarkerIdentity::new(name.clone()));
                syntax_markers.push(SourceWordSyntaxMarker::new(
                    name.clone(),
                    SourceWordSyntaxMarkerRole::BlockTerminator,
                ));
                terminator_implementation =
                    Some(UserDefinedStructuredTerminatorImplementation::new(
                        name,
                        SourceWordImplementation::from_prevalidated_instructions(
                            section.instructions,
                        ),
                    ));
            }
        }
    }

    let Some(terminator_identity) = terminator else {
        return Err(SourceWordError::SyntaxDefinition {
            span: start_header_span,
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let Some(terminator_implementation) = terminator_implementation else {
        return Err(SourceWordError::SyntaxDefinition {
            span: start_header_span,
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let grammar = StructuredGrammar::new(groups, Some(terminator_identity)).map_err(|_| {
        SourceWordError::SyntaxDefinition {
            span: terminator_implementation
                .implementation
                .instructions()
                .first()
                .map_or(start_header_span, |instruction| instruction.origin().span()),
            kind: SyntaxDefinitionErrorKind::UnsupportedKind,
        }
    })?;

    Ok(BlockSyntaxArtifacts {
        grammar,
        syntax_markers,
        implementation: UserDefinedStructuredSourceWordImplementation::new(
            start,
            markers,
            terminator_implementation,
        ),
    })
}

fn validate_block_section_local_visibility(
    sections: &[BlockSyntaxSection],
) -> Result<(), SourceWordBuildError> {
    let mut locals: HashMap<NormalizedName, BlockSectionLocalDefinition> = HashMap::new();

    for (section_index, section) in sections.iter().enumerate() {
        let visible_outside_section = block_section_locals_are_visible_outside(section);
        for instruction in &section.instructions {
            for reference in instruction.operation().consumed_local_references() {
                let Some(definition) = locals.get(reference.name()) else {
                    return Err(SourceWordBuildError::UndefinedLocal {
                        reference: reference.clone(),
                    });
                };
                if definition.section_index != section_index && !definition.visible_outside_section
                {
                    return Err(SourceWordBuildError::UndefinedLocal {
                        reference: reference.clone(),
                    });
                }
            }

            if let Some(binding) = instruction.operation().produced_binding_for_validation() {
                locals.insert(
                    binding.name().clone(),
                    BlockSectionLocalDefinition {
                        section_index,
                        visible_outside_section,
                    },
                );
            }
        }
    }

    Ok(())
}

fn block_section_locals_are_visible_outside(section: &BlockSyntaxSection) -> bool {
    match &section.kind {
        BlockSyntaxSectionKind::Start => true,
        BlockSyntaxSectionKind::Marker {
            cardinality: MarkerCardinality::One,
            ..
        } => true,
        BlockSyntaxSectionKind::Marker { .. } | BlockSyntaxSectionKind::Last { .. } => false,
    }
}

fn section_origin_span(section: &BlockSyntaxSection) -> SourceSpan {
    section
        .instructions
        .first()
        .map(|instruction| instruction.origin().span())
        .unwrap_or(section.header_span)
}

fn read_syntax_body_item<'source>(
    context: &mut NativeSourceWordContext<'source, '_>,
    eof_kind: SyntaxDefinitionErrorKind,
) -> Result<SourceBlockItem<'source>, SourceWordError> {
    let Some(reader) = context.block_reader_mut() else {
        return Err(SourceWordError::SyntaxPublicationContextUnavailable {
            span: context.source_word_token().span(),
        });
    };
    match reader.next_item()? {
        SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => {
            Err(SourceWordError::SyntaxDefinition {
                span,
                kind: eof_kind,
            })
        }
        SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => {
            Err(SourceWordError::DefLex { source: error })
        }
        item => Ok(item),
    }
}

fn require_empty_syntax_marker_remainder(
    marker: &SourceBlockMarker<'_>,
    kind: SyntaxDefinitionErrorKind,
) -> Result<(), SourceWordError> {
    require_empty_syntax_marker_remainder_with(marker, |_| kind)
}

fn require_empty_syntax_marker_remainder_with(
    marker: &SourceBlockMarker<'_>,
    error_kind: impl FnOnce(Token) -> SyntaxDefinitionErrorKind,
) -> Result<(), SourceWordError> {
    if let Some(token) = marker.remaining_tokens().first().copied() {
        return Err(SourceWordError::SyntaxDefinition {
            span: token.span(),
            kind: error_kind(token),
        });
    }
    Ok(())
}

fn syntax_item_span(item: SourceBlockItem<'_>, fallback: SourceSpan) -> SourceSpan {
    match item {
        SourceBlockItem::Statement(statement) => statement.span(),
        SourceBlockItem::Marker(marker) => marker.span(),
        SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => span,
        SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => match error {
            LexError::InvalidCharacter { span, .. } => span,
            LexError::Source(_) => fallback,
        },
    }
}

fn parse_source_processing_statement(
    view: SourceView<'_>,
    statement: SourceBlockStatement<'_>,
) -> Result<SourceProcessingInstruction, SourceWordError> {
    let first = statement
        .tokens()
        .first()
        .copied()
        .ok_or(SourceWordError::SyntaxDefinition {
            span: statement.span(),
            kind: SyntaxDefinitionErrorKind::UnknownOperation,
        })?;
    let origin = SourceInstructionOrigin::new(first.span());
    let mut reader = SourceStatementReader::new(&statement.tokens()[1..], first.span());
    let operation_name = normalized_token(view, first)?;
    let operation = match operation_name.as_str() {
        "READ_NAME" => SourceProcessingOperation::ReadName {
            bind: read_as_binding(view, &mut reader)?,
        },
        "EXPECT" => SourceProcessingOperation::Expect {
            token: read_fixed_token(view, &mut reader)?,
        },
        "EXPECT_END" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::ExpectEnd
        }
        "READ_LINE_NUM" => SourceProcessingOperation::ReadLineNumber {
            bind: read_as_binding(view, &mut reader)?,
        },
        "READ_EXPR" => SourceProcessingOperation::ReadExpression {
            bind: read_as_binding(view, &mut reader)?,
        },
        "READ_EXPR_UNTIL" => {
            let delimiter = read_fixed_token(view, &mut reader)?;
            SourceProcessingOperation::ReadExpressionUntil {
                delimiter,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "RESOLVE_VAR" => {
            let name = read_local_reference(view, &mut reader)?;
            SourceProcessingOperation::ResolveVariable {
                name,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "EMIT_EXPR" => SourceProcessingOperation::EmitExpression {
            expression: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_STORE" => SourceProcessingOperation::EmitStore {
            target: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_RETURN" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitReturn
        }
        "POSITION" => SourceProcessingOperation::Position {
            bind: read_as_binding(view, &mut reader)?,
        },
        "EMIT_BRANCH" => SourceProcessingOperation::EmitBranch {
            destination: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_BRANCH_IF_FALSE" => SourceProcessingOperation::EmitBranchIfFalse {
            destination: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_BRANCH_FOLLOWING" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchFollowing
        }
        "EMIT_BRANCH_IF_FALSE_FOLLOWING" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchIfFalseFollowing
        }
        "EMIT_BRANCH_COMPLETE" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchComplete
        }
        "EMIT_BRANCH_IF_FALSE_COMPLETE" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchIfFalseComplete
        }
        _ => {
            return Err(SourceWordError::SyntaxDefinition {
                span: first.span(),
                kind: SyntaxDefinitionErrorKind::UnknownOperation,
            });
        }
    };
    Ok(SourceProcessingInstruction::new(operation, origin))
}

fn read_as_binding(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalBinding, SourceWordError> {
    let as_token = reader.read_name().map_err(syntax_operation_reader_error)?;
    require_name_token(view, as_token, "AS", SyntaxDefinitionErrorKind::ExpectedAs)?;
    let binding = read_local_binding(view, reader)?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(binding)
}

fn read_only_local_reference(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalReference, SourceWordError> {
    let reference = read_local_reference(view, reader)?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(reference)
}

fn read_local_binding(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalBinding, SourceWordError> {
    let token = reader.read_name().map_err(syntax_operation_reader_error)?;
    Ok(LocalBinding::new(
        normalized_token(view, token)?,
        token.span(),
    ))
}

fn read_local_reference(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalReference, SourceWordError> {
    let token = reader.read_name().map_err(syntax_operation_reader_error)?;
    Ok(LocalReference::new(
        normalized_token(view, token)?,
        token.span(),
    ))
}

fn read_fixed_token(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<FixedToken, SourceWordError> {
    let token = reader
        .expect(TokenKind::FixedTokenLiteral)
        .map_err(|error| match error {
            SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
                span,
                kind: SyntaxDefinitionErrorKind::ExpectedFixedToken,
            },
            SourceStatementReaderError::Unexpected { actual, .. }
            | SourceStatementReaderError::TrailingToken { actual } => {
                SourceWordError::SyntaxDefinition {
                    span: actual.span(),
                    kind: SyntaxDefinitionErrorKind::ExpectedFixedToken,
                }
            }
        })?;
    let spelling = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    fixed_token_from_literal(spelling).ok_or(SourceWordError::SyntaxDefinition {
        span: token.span(),
        kind: SyntaxDefinitionErrorKind::ExpectedFixedToken,
    })
}

fn fixed_token_from_literal(spelling: &str) -> Option<FixedToken> {
    match spelling.strip_prefix('"')?.strip_suffix('"')? {
        "+" => Some(FixedToken::Plus),
        "-" => Some(FixedToken::Minus),
        "*" => Some(FixedToken::Star),
        "/" => Some(FixedToken::Slash),
        "%" => Some(FixedToken::Percent),
        "," => Some(FixedToken::Comma),
        "(" => Some(FixedToken::LeftParen),
        ")" => Some(FixedToken::RightParen),
        "=" => Some(FixedToken::Equal),
        "<>" => Some(FixedToken::NotEqual),
        "<" => Some(FixedToken::Less),
        "<=" => Some(FixedToken::LessEqual),
        ">" => Some(FixedToken::Greater),
        ">=" => Some(FixedToken::GreaterEqual),
        _ => None,
    }
}

fn require_name_token(
    view: SourceView<'_>,
    token: Token,
    expected: &str,
    error_kind: SyntaxDefinitionErrorKind,
) -> Result<(), SourceWordError> {
    let source_name = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    if source_name.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(SourceWordError::SyntaxDefinition {
            span: token.span(),
            kind: error_kind,
        })
    }
}

fn normalized_token(view: SourceView<'_>, token: Token) -> Result<NormalizedName, SourceWordError> {
    let source_name = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    NormalizedName::new(source_name).map_err(|source| SourceWordError::SyntaxName {
        span: token.span(),
        source,
    })
}

#[derive(Debug)]
pub(crate) struct UserDefinedStructuredSourceWordOwner {
    implementation: UserDefinedStructuredSourceWordImplementation,
    state: SourceWordEvaluationState,
}

impl UserDefinedStructuredSourceWordOwner {
    pub(crate) fn new(
        implementation: UserDefinedStructuredSourceWordImplementation,
        state: SourceWordEvaluationState,
    ) -> Self {
        Self {
            implementation,
            state,
        }
    }
}

impl NativeStructuredSourceWordOwner for UserDefinedStructuredSourceWordOwner {
    fn current_body_context(&self) -> StructuredBodyContext {
        StructuredBodyContext::inherited()
    }

    fn accept_marker<'source>(
        &mut self,
        context: &mut NativeStructuredSourceWordContext<'source, '_>,
        marker: SourceBlockMarker<'source>,
        accept: crate::structured_grammar::GrammarAccept,
    ) -> Result<(), SourceWordError> {
        let crate::structured_grammar::GrammarAccept::Intermediate { group_index } = accept else {
            return Err(SourceWordError::UnsupportedSourceWord {
                span: marker.span(),
            });
        };
        let Some(implementation) = self.implementation.marker(group_index) else {
            return Err(SourceWordError::UnsupportedSourceWord {
                span: marker.span(),
            });
        };
        context.evaluate_user_defined_source_word(
            implementation,
            &mut self.state,
            marker.statement().tokens(),
        )
    }

    fn complete<'source>(
        &mut self,
        context: &mut NativeStructuredSourceWordContext<'source, '_>,
        marker: SourceBlockMarker<'source>,
    ) -> Result<(), SourceWordError> {
        context.evaluate_user_defined_source_word(
            self.implementation.terminator(),
            &mut self.state,
            marker.statement().tokens(),
        )
    }
}

#[derive(Debug)]
struct IfSourceWordOwner {
    branches: Vec<IfBranch>,
    current_body_index: usize,
}

#[derive(Debug)]
struct IfBranch {
    condition: Option<ExpressionStaging>,
    origin_span: SourceSpan,
}

pub(crate) fn if_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<StructuredSourceWordInstance, SourceWordError> {
    let condition = parse_required_if_condition_from_reader(context)?;
    let origin_span = context.statement_span()?;
    Ok(StructuredSourceWordInstance::new(Box::new(
        IfSourceWordOwner {
            branches: vec![IfBranch {
                condition: Some(condition),
                origin_span,
            }],
            current_body_index: 0,
        },
    )))
}

impl NativeStructuredSourceWordOwner for IfSourceWordOwner {
    fn current_body_context(&self) -> StructuredBodyContext {
        StructuredBodyContext::new(
            StructuredBuildTargetScope::OwnerLocal(self.current_body_index),
            StructuredLineNumberScope::OwnerLocal(self.current_body_index),
            StructuredBodyCapabilities::without_publication(),
        )
    }

    fn accept_marker<'source>(
        &mut self,
        context: &mut NativeStructuredSourceWordContext<'source, '_>,
        marker: SourceBlockMarker<'source>,
        _accept: crate::structured_grammar::GrammarAccept,
    ) -> Result<(), SourceWordError> {
        let condition = if marker.name().as_str() == "ELSIF" {
            Some(parse_required_if_condition_from_tokens(
                marker.remaining_tokens(),
                marker.token().span(),
                context,
            )?)
        } else {
            require_empty_if_marker_remainder(&marker)?;
            None
        };

        self.branches.push(IfBranch {
            condition,
            origin_span: marker.span(),
        });
        self.current_body_index += 1;
        Ok(())
    }

    fn complete<'source>(
        &mut self,
        context: &mut NativeStructuredSourceWordContext<'source, '_>,
        marker: SourceBlockMarker<'source>,
    ) -> Result<(), SourceWordError> {
        require_empty_if_marker_remainder(&marker)?;

        let mut merge_jumps = Vec::new();
        for index in 0..self.branches.len() {
            let branch = &self.branches[index];
            let branch_if_false = if let Some(condition) = &branch.condition {
                commit_if_condition(context, condition)?;
                Some(context.append_mapped_jump_if_zero_placeholder(branch.origin_span)?)
            } else {
                None
            };

            context.append_owner_local_target(index, branch.origin_span)?;

            if index + 1 < self.branches.len() {
                merge_jumps.push(context.append_mapped_jump_placeholder(branch.origin_span)?);
            }

            if let Some(branch_if_false) = branch_if_false {
                context.patch_branch_target(branch_if_false, context.current_address())?;
            }
        }

        let merge_target = context.current_address();
        for jump in merge_jumps {
            context.patch_branch_target(jump, merge_target)?;
        }
        Ok(())
    }
}

fn parse_required_if_condition_from_reader(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<ExpressionStaging, SourceWordError> {
    let (tokens, anchor) = {
        let reader = context.statement_reader_mut();
        let tokens = reader
            .remaining_expression()
            .map_err(if_reader_error_for_condition)?;
        (tokens, context.source_word_token().span())
    };
    parse_required_if_condition_from_tokens(tokens, anchor, context)
}

fn parse_required_if_condition_from_tokens(
    tokens: &[Token],
    anchor: SourceSpan,
    context: &impl IfConditionContext,
) -> Result<ExpressionStaging, SourceWordError> {
    if tokens.is_empty() {
        return Err(SourceWordError::IfSyntax {
            span: anchor,
            kind: IfSyntaxErrorKind::MissingCondition,
        });
    }
    context.stage_if_expression(tokens, anchor)
}

trait IfConditionContext {
    fn stage_if_expression(
        &self,
        tokens: &[Token],
        anchor: SourceSpan,
    ) -> Result<ExpressionStaging, SourceWordError>;
}

impl IfConditionContext for NativeSourceWordContext<'_, '_> {
    fn stage_if_expression(
        &self,
        tokens: &[Token],
        anchor: SourceSpan,
    ) -> Result<ExpressionStaging, SourceWordError> {
        self.stage_expression(tokens, anchor)
    }
}

impl IfConditionContext for NativeStructuredSourceWordContext<'_, '_> {
    fn stage_if_expression(
        &self,
        tokens: &[Token],
        anchor: SourceSpan,
    ) -> Result<ExpressionStaging, SourceWordError> {
        self.stage_expression(tokens, anchor)
    }
}

fn commit_if_condition(
    context: &mut NativeStructuredSourceWordContext<'_, '_>,
    condition: &ExpressionStaging,
) -> Result<(), SourceWordError> {
    for entry in condition.entries() {
        context.append_mapped(entry.instruction(), entry.span())?;
    }
    Ok(())
}

fn require_empty_if_marker_remainder(
    marker: &SourceBlockMarker<'_>,
) -> Result<(), SourceWordError> {
    if let Some(token) = marker.remaining_tokens().first().copied() {
        return Err(SourceWordError::IfSyntax {
            span: token.span(),
            kind: IfSyntaxErrorKind::TrailingToken { kind: token.kind() },
        });
    }
    Ok(())
}

fn if_reader_error_for_condition(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::IfSyntax {
            span,
            kind: IfSyntaxErrorKind::MissingCondition,
        },
        SourceStatementReaderError::Unexpected { actual, .. }
        | SourceStatementReaderError::TrailingToken { actual } => SourceWordError::IfSyntax {
            span: actual.span(),
            kind: IfSyntaxErrorKind::TrailingToken {
                kind: actual.kind(),
            },
        },
    }
}

fn syntax_definition_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::MissingName,
        },
        SourceStatementReaderError::Unexpected { actual, .. } => {
            SourceWordError::SyntaxDefinition {
                span: actual.span(),
                kind: SyntaxDefinitionErrorKind::MissingName,
            }
        }
        SourceStatementReaderError::TrailingToken { actual } => SourceWordError::SyntaxDefinition {
            span: actual.span(),
            kind: SyntaxDefinitionErrorKind::TrailingToken {
                kind: actual.kind(),
            },
        },
    }
}

fn syntax_operation_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::MissingOperand,
        },
        SourceStatementReaderError::Unexpected { actual, .. } => {
            SourceWordError::SyntaxDefinition {
                span: actual.span(),
                kind: SyntaxDefinitionErrorKind::MissingOperand,
            }
        }
        SourceStatementReaderError::TrailingToken { actual } => SourceWordError::SyntaxDefinition {
            span: actual.span(),
            kind: SyntaxDefinitionErrorKind::TrailingOperationToken {
                kind: actual.kind(),
            },
        },
    }
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
    kind: SourceWordKind,
    syntax_markers: Vec<SourceWordSyntaxMarker>,
}

#[derive(Debug, Clone)]
enum SourceWordKind {
    OneShot(OneShotSourceWordImplementation),
    Structured {
        implementation: StructuredSourceWordImplementation,
        grammar: StructuredGrammar,
    },
}

#[derive(Debug, Clone)]
enum OneShotSourceWordImplementation {
    Native(NativeSourceWordHandler),
    UserDefined(SourceWordImplementation),
}

#[derive(Debug, Clone)]
enum StructuredSourceWordImplementation {
    Native(NativeStructuredSourceWordStartHandler),
    UserDefined(UserDefinedStructuredSourceWordImplementation),
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
            kind: SourceWordKind::OneShot(OneShotSourceWordImplementation::Native(handler)),
            syntax_markers,
        });
        id
    }

    pub(crate) fn register_user_defined_statement(
        &mut self,
        implementation: SourceWordImplementation,
    ) -> SourceWordId {
        let id = SourceWordId::from_slot(self.entries.len());
        self.entries.push(SourceWordEntry {
            kind: SourceWordKind::OneShot(OneShotSourceWordImplementation::UserDefined(
                implementation,
            )),
            syntax_markers: Vec::new(),
        });
        id
    }

    pub(crate) fn register_structured(
        &mut self,
        start: NativeStructuredSourceWordStartHandler,
        grammar: StructuredGrammar,
        syntax_markers: Vec<SourceWordSyntaxMarker>,
    ) -> SourceWordId {
        let id = SourceWordId::from_slot(self.entries.len());
        self.entries.push(SourceWordEntry {
            kind: SourceWordKind::Structured {
                implementation: StructuredSourceWordImplementation::Native(start),
                grammar,
            },
            syntax_markers,
        });
        id
    }

    pub(crate) fn register_user_defined_structured(
        &mut self,
        grammar: StructuredGrammar,
        syntax_markers: Vec<SourceWordSyntaxMarker>,
        implementation: UserDefinedStructuredSourceWordImplementation,
    ) -> SourceWordId {
        let id = SourceWordId::from_slot(self.entries.len());
        self.entries.push(SourceWordEntry {
            kind: SourceWordKind::Structured {
                implementation: StructuredSourceWordImplementation::UserDefined(implementation),
                grammar,
            },
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

#[derive(Debug, Clone, Copy)]
pub(crate) enum SourceWordDispatch<'a> {
    OneShot(OneShotSourceWordDispatch<'a>),
    Structured {
        implementation: StructuredSourceWordDispatch<'a>,
        grammar: &'a StructuredGrammar,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum OneShotSourceWordDispatch<'a> {
    Native(NativeSourceWordHandler),
    UserDefined(&'a SourceWordImplementation),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StructuredSourceWordDispatch<'a> {
    Native(NativeStructuredSourceWordStartHandler),
    UserDefined(&'a UserDefinedStructuredSourceWordImplementation),
}

impl<'a> SourceWordLookup<'a> {
    pub(crate) fn lookup_dispatch(
        self,
        id: SourceWordId,
    ) -> Result<SourceWordDispatch<'a>, SourceWordLookupError> {
        let entry = self
            .registry
            .entries
            .get(id.as_slot())
            .ok_or(SourceWordLookupError::InvalidSourceWordId { id })?;
        Ok(match &entry.kind {
            SourceWordKind::OneShot(implementation) => {
                SourceWordDispatch::OneShot(match implementation {
                    OneShotSourceWordImplementation::Native(handler) => {
                        OneShotSourceWordDispatch::Native(*handler)
                    }
                    OneShotSourceWordImplementation::UserDefined(implementation) => {
                        OneShotSourceWordDispatch::UserDefined(implementation)
                    }
                })
            }
            SourceWordKind::Structured {
                implementation,
                grammar,
            } => SourceWordDispatch::Structured {
                implementation: match implementation {
                    StructuredSourceWordImplementation::Native(start) => {
                        StructuredSourceWordDispatch::Native(*start)
                    }
                    StructuredSourceWordImplementation::UserDefined(implementation) => {
                        StructuredSourceWordDispatch::UserDefined(implementation)
                    }
                },
                grammar,
            },
        })
    }

    pub(crate) fn lookup_handler(
        self,
        id: SourceWordId,
    ) -> Result<NativeSourceWordHandler, SourceWordLookupError> {
        match self.lookup_dispatch(id)? {
            SourceWordDispatch::OneShot(OneShotSourceWordDispatch::Native(handler)) => Ok(handler),
            SourceWordDispatch::OneShot(OneShotSourceWordDispatch::UserDefined(_)) => {
                Err(SourceWordLookupError::InvalidSourceWordId { id })
            }
            SourceWordDispatch::Structured { .. } => {
                Err(SourceWordLookupError::InvalidSourceWordId { id })
            }
        }
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
            source_word_publication: None,
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
                source_word_publication: None,
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
                    source_word_publication: None,
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
