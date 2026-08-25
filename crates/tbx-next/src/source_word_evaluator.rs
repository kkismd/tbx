use std::collections::HashMap;

use crate::binding::Bindings;
use crate::expression::{parse_expression, ExpressionError, ExpressionStaging};
use crate::global_variable::GlobalVarId;
use crate::instruction::{Instruction, InstructionAddress};
use crate::instruction_builder::{InstructionBuildError, InstructionBuildTarget};
use crate::lexer::{Token, TokenKind};
use crate::line_number::LocalLineNumber;
use crate::line_number::LocalLineNumberTable;
use crate::name::{NameError, NormalizedName};
use crate::operator::OperatorLookup;
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::source_word::{
    SourceStatementExpected, SourceStatementReader, SourceStatementReaderError,
};
use crate::source_word_ir::{
    LocalBinding, LocalReference, SourceInstructionOrigin, SourceProcessingCapabilities,
    SourceProcessingOperation, SourceWordImplementation,
};
use crate::word_resolution::{resolve_binding_name, ResolvedBinding, WordResolutionError};

pub(crate) struct UserDefinedSourceWordContext<'source, 'state> {
    view: SourceView<'source>,
    source_id: SourceId,
    reader: SourceStatementReader<'source>,
    bindings: &'state Bindings,
    operators: Option<OperatorLookup>,
    code: &'state mut dyn InstructionBuildTarget,
    line_numbers: &'state mut LocalLineNumberTable,
    capabilities: SourceProcessingCapabilities,
}

pub(crate) struct UserDefinedSourceWordContextParts<'source, 'state> {
    pub(crate) view: SourceView<'source>,
    pub(crate) source_id: SourceId,
    pub(crate) tokens: &'source [Token],
    pub(crate) bindings: &'state Bindings,
    pub(crate) operators: Option<OperatorLookup>,
    pub(crate) code: &'state mut dyn InstructionBuildTarget,
    pub(crate) line_numbers: &'state mut LocalLineNumberTable,
    pub(crate) capabilities: SourceProcessingCapabilities,
}

#[derive(Debug, Default)]
pub(crate) struct SourceWordEvaluationState {
    locals: RuntimeLocals,
    structural_branches: StructuralBranchState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceWordEvaluationError {
    CapabilityUnavailable {
        required: SourceProcessingCapabilities,
        available: SourceProcessingCapabilities,
        origin: SourceInstructionOrigin,
    },
    Source {
        source: SourceError,
    },
    Reader {
        source: SourceStatementReaderError,
        origin: SourceInstructionOrigin,
    },
    Name {
        span: SourceSpan,
        source: NameError,
        origin: SourceInstructionOrigin,
    },
    LineNumberLiteralOutOfRange {
        span: SourceSpan,
        origin: SourceInstructionOrigin,
    },
    LineNumberLiteralConversion {
        span: SourceSpan,
        origin: SourceInstructionOrigin,
    },
    VariableResolution {
        span: SourceSpan,
        source: crate::expression::ExpressionVariableErrorKind,
        origin: SourceInstructionOrigin,
    },
    Expression {
        source: ExpressionError,
        origin: SourceInstructionOrigin,
    },
    InstructionBuild {
        source: InstructionBuildError,
        origin: SourceInstructionOrigin,
    },
    UndefinedLocal {
        reference: LocalReference,
        origin: SourceInstructionOrigin,
    },
    LocalTypeMismatch {
        reference: LocalReference,
        expected: RuntimeLocalType,
        actual: RuntimeLocalType,
        origin: SourceInstructionOrigin,
    },
    UnsupportedStructuralBranch {
        origin: SourceInstructionOrigin,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeLocalType {
    NameInput,
    VariableTarget,
    ExpressionArtifact,
    OwnerLocalCodePosition,
    LocalLineTarget,
}

impl SourceWordEvaluationError {
    pub(crate) fn primary_span(&self) -> Option<SourceSpan> {
        match self {
            Self::CapabilityUnavailable { origin, .. }
            | Self::UnsupportedStructuralBranch { origin } => Some(origin.span()),
            Self::Source { .. } => None,
            Self::Reader { source, origin: _ } => Some(match source {
                SourceStatementReaderError::Missing { span, .. } => *span,
                SourceStatementReaderError::Unexpected { actual, .. }
                | SourceStatementReaderError::TrailingToken { actual } => actual.span(),
            }),
            Self::Name { span, .. }
            | Self::LineNumberLiteralOutOfRange { span, .. }
            | Self::LineNumberLiteralConversion { span, .. }
            | Self::VariableResolution { span, .. } => Some(*span),
            Self::InstructionBuild { origin, .. } => Some(origin.span()),
            Self::Expression { source, origin } => match source {
                ExpressionError::Source(_) | ExpressionError::InstructionBuild(_) => {
                    Some(origin.span())
                }
                ExpressionError::Syntax(error) => Some(error.span()),
                ExpressionError::Variable(error) => Some(error.span()),
            },
            Self::UndefinedLocal { reference, .. } | Self::LocalTypeMismatch { reference, .. } => {
                Some(reference.span())
            }
        }
    }
}

#[derive(Debug, Clone)]
enum RuntimeLocal {
    NameInput {
        name: NormalizedName,
        span: SourceSpan,
    },
    VariableTarget {
        id: GlobalVarId,
        span: SourceSpan,
    },
    ExpressionArtifact(ExpressionStaging),
    OwnerLocalCodePosition(InstructionAddress),
    LocalLineTarget(LocalLineNumber),
}

#[derive(Debug, Default)]
struct RuntimeLocals {
    values: HashMap<NormalizedName, RuntimeLocal>,
}

#[derive(Debug, Default)]
struct StructuralBranchState {
    // #1561 keeps FOLLOWING/COMPLETE branch placeholders owner-local and
    // private so user-authored source words do not gain a generic TARGET API.
    following: Vec<StructuralBranchPatch>,
    complete: Vec<StructuralBranchPatch>,
    pending_section_start: Option<InstructionAddress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StructuralBranchPatch {
    branch: InstructionAddress,
    origin: SourceInstructionOrigin,
}

impl StructuralBranchState {
    fn begin_section(&mut self, start: InstructionAddress) {
        self.pending_section_start = Some(start);
    }

    fn record_following(&mut self, branch: InstructionAddress, origin: SourceInstructionOrigin) {
        self.following
            .push(StructuralBranchPatch { branch, origin });
    }

    fn record_complete(&mut self, branch: InstructionAddress, origin: SourceInstructionOrigin) {
        self.complete.push(StructuralBranchPatch { branch, origin });
    }

    fn patch_following_section_start(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
    ) -> Result<(), SourceWordEvaluationError> {
        let Some(target) = self.pending_section_start.take() else {
            return Ok(());
        };
        self.patch_following_to(code, target)
    }

    fn patch_following_after_complete_branch(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
    ) -> Result<(), SourceWordEvaluationError> {
        let target = code.current_address();
        self.pending_section_start = None;
        self.patch_following_to(code, target)
    }

    fn patch_all_to_current_address(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
    ) -> Result<(), SourceWordEvaluationError> {
        let target = code.current_address();
        self.pending_section_start = None;
        self.patch_following_to(code, target)?;
        self.patch_complete_to(code, target)
    }

    fn patch_following_to(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
        target: InstructionAddress,
    ) -> Result<(), SourceWordEvaluationError> {
        patch_structural_branches(code, &mut self.following, target)
    }

    fn patch_complete_to(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
        target: InstructionAddress,
    ) -> Result<(), SourceWordEvaluationError> {
        patch_structural_branches(code, &mut self.complete, target)
    }
}

fn patch_structural_branches(
    code: &mut dyn InstructionBuildTarget,
    patches: &mut Vec<StructuralBranchPatch>,
    target: InstructionAddress,
) -> Result<(), SourceWordEvaluationError> {
    let mut patched = 0;
    while let Some(patch) = patches.get(patched).copied() {
        if let Err(source) = code.patch_branch_target(patch.branch, target) {
            patches.drain(0..patched);
            return Err(SourceWordEvaluationError::InstructionBuild {
                source,
                origin: patch.origin,
            });
        }
        patched += 1;
    }
    patches.clear();
    Ok(())
}

impl<'source, 'state> UserDefinedSourceWordContext<'source, 'state> {
    pub(crate) fn new(parts: UserDefinedSourceWordContextParts<'source, 'state>) -> Self {
        let source_word_token = parts
            .tokens
            .first()
            .copied()
            .expect("user-defined source word context requires its leading token");
        Self {
            view: parts.view,
            source_id: parts.source_id,
            reader: SourceStatementReader::new(&parts.tokens[1..], source_word_token.span()),
            bindings: parts.bindings,
            operators: parts.operators,
            code: parts.code,
            line_numbers: parts.line_numbers,
            capabilities: parts.capabilities,
        }
    }
}

pub(crate) fn evaluate_source_word(
    implementation: &SourceWordImplementation,
    context: &mut UserDefinedSourceWordContext<'_, '_>,
) -> Result<(), SourceWordEvaluationError> {
    let mut state = SourceWordEvaluationState::new();
    evaluate_source_word_with_state(implementation, context, &mut state)
}

pub(crate) fn evaluate_source_word_with_state(
    implementation: &SourceWordImplementation,
    context: &mut UserDefinedSourceWordContext<'_, '_>,
    state: &mut SourceWordEvaluationState,
) -> Result<(), SourceWordEvaluationError> {
    if !context.capabilities.allows(implementation.capabilities()) {
        return Err(SourceWordEvaluationError::CapabilityUnavailable {
            required: implementation.capabilities(),
            available: context.capabilities,
            origin: implementation
                .instructions()
                .first()
                .expect("non-empty required capability set needs an instruction")
                .origin(),
        });
    }

    state
        .structural_branches
        .begin_section(context.code.current_address());

    for instruction in implementation.instructions() {
        evaluate_instruction(
            instruction.operation(),
            instruction.origin(),
            context,
            &mut state.locals,
            &mut state.structural_branches,
        )?;
    }
    Ok(())
}

impl SourceWordEvaluationState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn complete_structural_branches(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
    ) -> Result<(), SourceWordEvaluationError> {
        self.structural_branches.patch_all_to_current_address(code)
    }
}

fn evaluate_instruction(
    operation: &SourceProcessingOperation,
    origin: SourceInstructionOrigin,
    context: &mut UserDefinedSourceWordContext<'_, '_>,
    locals: &mut RuntimeLocals,
    structural_branches: &mut StructuralBranchState,
) -> Result<(), SourceWordEvaluationError> {
    // #1556/#1559 keep this as a small source-processing evaluator: operations
    // are connected only to the current context capabilities, not to runtime VM
    // values or a generic source-processing stack.
    match operation {
        SourceProcessingOperation::EmitBranchComplete
        | SourceProcessingOperation::EmitBranchIfFalseComplete => {}
        _ => structural_branches.patch_following_section_start(context.code)?,
    }

    match operation {
        SourceProcessingOperation::ReadName { bind } => {
            let token = context
                .reader
                .read_name()
                .map_err(|source| SourceWordEvaluationError::Reader { source, origin })?;
            let source_name = context
                .view
                .slice(token.span())
                .map_err(|source| SourceWordEvaluationError::Source { source })?;
            let name = NormalizedName::new(source_name).map_err(|source| {
                SourceWordEvaluationError::Name {
                    span: token.span(),
                    source,
                    origin,
                }
            })?;
            locals.bind(
                bind,
                RuntimeLocal::NameInput {
                    name,
                    span: token.span(),
                },
            );
        }
        SourceProcessingOperation::Expect { token } => {
            context
                .reader
                .expect(token.token_kind())
                .map_err(|source| SourceWordEvaluationError::Reader { source, origin })?;
        }
        SourceProcessingOperation::ExpectEnd => {
            context
                .reader
                .finish()
                .map_err(|source| SourceWordEvaluationError::Reader { source, origin })?;
        }
        SourceProcessingOperation::ReadLineNumber { bind } => {
            let token = context
                .reader
                .expect(TokenKind::IntegerLiteral)
                .map_err(|source| SourceWordEvaluationError::Reader { source, origin })?;
            let line_number = parse_line_number(context.view, token, origin)?;
            locals.bind(bind, RuntimeLocal::LocalLineTarget(line_number));
        }
        SourceProcessingOperation::ReadExpression { bind } => {
            let tokens = context
                .reader
                .remaining_expression()
                .map_err(|source| SourceWordEvaluationError::Reader { source, origin })?;
            let expression = stage_expression(context, tokens, origin.span(), origin)?;
            locals.bind(bind, RuntimeLocal::ExpressionArtifact(expression));
        }
        SourceProcessingOperation::ReadExpressionUntil { delimiter, bind } => {
            let tokens = context
                .reader
                .expression_until(delimiter.token_kind())
                .map_err(|source| SourceWordEvaluationError::Reader { source, origin })?;
            let expression = stage_expression(context, tokens, origin.span(), origin)?;
            locals.bind(bind, RuntimeLocal::ExpressionArtifact(expression));
        }
        SourceProcessingOperation::ResolveVariable { name, bind } => {
            let (source_name, span) = locals.name(name, origin)?;
            let id = resolve_variable_name(context.bindings, source_name.as_str()).map_err(
                |source| SourceWordEvaluationError::VariableResolution {
                    span,
                    source,
                    origin,
                },
            )?;
            locals.bind(bind, RuntimeLocal::VariableTarget { id, span });
        }
        SourceProcessingOperation::EmitExpression { expression } => {
            let expression = locals.expression(expression, origin)?;
            expression
                .commit_to(context.code)
                .map_err(|source| match source {
                    ExpressionError::InstructionBuild(source) => {
                        SourceWordEvaluationError::InstructionBuild { source, origin }
                    }
                    source => SourceWordEvaluationError::Expression { source, origin },
                })?;
        }
        SourceProcessingOperation::EmitStore { target } => {
            let (id, span) = locals.variable_target(target, origin)?;
            context
                .code
                .append_mapped(Instruction::StoreVar(id), span)
                .map_err(|source| SourceWordEvaluationError::InstructionBuild { source, origin })?;
        }
        SourceProcessingOperation::EmitReturn => {
            context
                .code
                .append_mapped(Instruction::Return, origin.span())
                .map_err(|source| SourceWordEvaluationError::InstructionBuild { source, origin })?;
        }
        SourceProcessingOperation::Position { bind } => {
            locals.bind(
                bind,
                RuntimeLocal::OwnerLocalCodePosition(context.code.current_address()),
            );
        }
        SourceProcessingOperation::EmitBranch { destination } => {
            match locals.branch_destination(destination, origin)? {
                BranchDestination::OwnerLocalCodePosition(target) => {
                    let branch = context
                        .code
                        .append_mapped_jump_placeholder(origin.span())
                        .map_err(|source| SourceWordEvaluationError::InstructionBuild {
                            source,
                            origin,
                        })?;
                    context
                        .code
                        .patch_branch_target(branch, target)
                        .map_err(|source| SourceWordEvaluationError::InstructionBuild {
                            source,
                            origin,
                        })?;
                }
                BranchDestination::LocalLineTarget(line_number) => {
                    let branch = context
                        .code
                        .append_mapped_jump_placeholder(origin.span())
                        .map_err(|source| SourceWordEvaluationError::InstructionBuild {
                            source,
                            origin,
                        })?;
                    context
                        .line_numbers
                        .add_patch(line_number, branch, destination.span());
                }
            }
        }
        SourceProcessingOperation::EmitBranchIfFalse { destination } => {
            match locals.branch_destination(destination, origin)? {
                BranchDestination::OwnerLocalCodePosition(target) => {
                    let branch = context
                        .code
                        .append_mapped_jump_if_zero_placeholder(origin.span())
                        .map_err(|source| SourceWordEvaluationError::InstructionBuild {
                            source,
                            origin,
                        })?;
                    context
                        .code
                        .patch_branch_target(branch, target)
                        .map_err(|source| SourceWordEvaluationError::InstructionBuild {
                            source,
                            origin,
                        })?;
                }
                BranchDestination::LocalLineTarget(line_number) => {
                    let branch = context
                        .code
                        .append_mapped_jump_if_zero_placeholder(origin.span())
                        .map_err(|source| SourceWordEvaluationError::InstructionBuild {
                            source,
                            origin,
                        })?;
                    context
                        .line_numbers
                        .add_patch(line_number, branch, destination.span());
                }
            }
        }
        SourceProcessingOperation::EmitBranchFollowing => {
            let branch = context
                .code
                .append_mapped_jump_placeholder(origin.span())
                .map_err(|source| SourceWordEvaluationError::InstructionBuild { source, origin })?;
            structural_branches.record_following(branch, origin);
        }
        SourceProcessingOperation::EmitBranchIfFalseFollowing => {
            let branch = context
                .code
                .append_mapped_jump_if_zero_placeholder(origin.span())
                .map_err(|source| SourceWordEvaluationError::InstructionBuild { source, origin })?;
            structural_branches.record_following(branch, origin);
        }
        SourceProcessingOperation::EmitBranchComplete => {
            let branch = context
                .code
                .append_mapped_jump_placeholder(origin.span())
                .map_err(|source| SourceWordEvaluationError::InstructionBuild { source, origin })?;
            structural_branches.record_complete(branch, origin);
            structural_branches.patch_following_after_complete_branch(context.code)?;
        }
        SourceProcessingOperation::EmitBranchIfFalseComplete => {
            let branch = context
                .code
                .append_mapped_jump_if_zero_placeholder(origin.span())
                .map_err(|source| SourceWordEvaluationError::InstructionBuild { source, origin })?;
            structural_branches.record_complete(branch, origin);
            structural_branches.patch_following_after_complete_branch(context.code)?;
        }
    }
    Ok(())
}

fn stage_expression(
    context: &UserDefinedSourceWordContext<'_, '_>,
    tokens: &[Token],
    anchor: SourceSpan,
    origin: SourceInstructionOrigin,
) -> Result<ExpressionStaging, SourceWordEvaluationError> {
    let Some(operators) = context.operators else {
        return Err(SourceWordEvaluationError::Reader {
            source: SourceStatementReaderError::Missing {
                expected: SourceStatementExpected::Expression,
                span: anchor,
            },
            origin,
        });
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
        context
            .view
            .span(context.source_id, end, end)
            .map_err(|source| SourceWordEvaluationError::Expression {
                source: ExpressionError::Source(source),
                origin,
            })?,
    ));

    let resolver = |source_name: &str| resolve_variable_name(context.bindings, source_name);
    parse_expression(context.view, &expression_tokens, operators, &resolver)
        .map_err(|source| SourceWordEvaluationError::Expression { source, origin })
}

fn parse_line_number(
    view: SourceView<'_>,
    token: Token,
    origin: SourceInstructionOrigin,
) -> Result<LocalLineNumber, SourceWordEvaluationError> {
    let source = view
        .slice(token.span())
        .map_err(|source| SourceWordEvaluationError::Source { source })?;
    let value = source.parse::<u64>().map_err(|_| {
        SourceWordEvaluationError::LineNumberLiteralConversion {
            span: token.span(),
            origin,
        }
    })?;
    if value > u64::from(u16::MAX) {
        return Err(SourceWordEvaluationError::LineNumberLiteralOutOfRange {
            span: token.span(),
            origin,
        });
    }
    Ok(LocalLineNumber::new(value))
}

fn resolve_variable_name(
    bindings: &Bindings,
    source_name: &str,
) -> Result<GlobalVarId, crate::expression::ExpressionVariableErrorKind> {
    match resolve_binding_name(bindings, source_name) {
        Ok(ResolvedBinding::Variable(id)) => Ok(id),
        Ok(ResolvedBinding::RuntimeWord(_) | ResolvedBinding::SourceWord(_)) => {
            Err(crate::expression::ExpressionVariableErrorKind::TargetIsNotVariable)
        }
        Err(WordResolutionError::InvalidWordName) => {
            Err(crate::expression::ExpressionVariableErrorKind::InvalidName)
        }
        Err(WordResolutionError::UndefinedName) => {
            Err(crate::expression::ExpressionVariableErrorKind::UndefinedName)
        }
        Err(WordResolutionError::TargetIsNotWord) => {
            unreachable!("binding lookup does not require a runtime word target")
        }
    }
}

impl RuntimeLocals {
    fn bind(&mut self, binding: &LocalBinding, local: RuntimeLocal) {
        self.values.insert(binding.name().clone(), local);
    }

    fn get(
        &self,
        reference: &LocalReference,
        origin: SourceInstructionOrigin,
    ) -> Result<&RuntimeLocal, SourceWordEvaluationError> {
        self.values
            .get(reference.name())
            .ok_or_else(|| SourceWordEvaluationError::UndefinedLocal {
                reference: reference.clone(),
                origin,
            })
    }

    fn name(
        &self,
        reference: &LocalReference,
        origin: SourceInstructionOrigin,
    ) -> Result<(NormalizedName, SourceSpan), SourceWordEvaluationError> {
        match self.get(reference, origin)? {
            RuntimeLocal::NameInput { name, span } => Ok((name.clone(), *span)),
            actual => Err(SourceWordEvaluationError::LocalTypeMismatch {
                reference: reference.clone(),
                expected: RuntimeLocalType::NameInput,
                actual: actual.local_type(),
                origin,
            }),
        }
    }

    fn expression(
        &self,
        reference: &LocalReference,
        origin: SourceInstructionOrigin,
    ) -> Result<&ExpressionStaging, SourceWordEvaluationError> {
        match self.get(reference, origin)? {
            RuntimeLocal::ExpressionArtifact(expression) => Ok(expression),
            actual => Err(SourceWordEvaluationError::LocalTypeMismatch {
                reference: reference.clone(),
                expected: RuntimeLocalType::ExpressionArtifact,
                actual: actual.local_type(),
                origin,
            }),
        }
    }

    fn variable_target(
        &self,
        reference: &LocalReference,
        origin: SourceInstructionOrigin,
    ) -> Result<(GlobalVarId, SourceSpan), SourceWordEvaluationError> {
        match self.get(reference, origin)? {
            RuntimeLocal::VariableTarget { id, span } => Ok((*id, *span)),
            actual => Err(SourceWordEvaluationError::LocalTypeMismatch {
                reference: reference.clone(),
                expected: RuntimeLocalType::VariableTarget,
                actual: actual.local_type(),
                origin,
            }),
        }
    }

    fn branch_destination(
        &self,
        reference: &LocalReference,
        origin: SourceInstructionOrigin,
    ) -> Result<BranchDestination, SourceWordEvaluationError> {
        match self.get(reference, origin)? {
            RuntimeLocal::OwnerLocalCodePosition(address) => {
                Ok(BranchDestination::OwnerLocalCodePosition(*address))
            }
            RuntimeLocal::LocalLineTarget(line_number) => {
                Ok(BranchDestination::LocalLineTarget(*line_number))
            }
            actual => Err(SourceWordEvaluationError::LocalTypeMismatch {
                reference: reference.clone(),
                expected: RuntimeLocalType::OwnerLocalCodePosition,
                actual: actual.local_type(),
                origin,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchDestination {
    OwnerLocalCodePosition(InstructionAddress),
    LocalLineTarget(LocalLineNumber),
}

impl RuntimeLocal {
    const fn local_type(&self) -> RuntimeLocalType {
        match self {
            Self::NameInput { .. } => RuntimeLocalType::NameInput,
            Self::VariableTarget { .. } => RuntimeLocalType::VariableTarget,
            Self::ExpressionArtifact(_) => RuntimeLocalType::ExpressionArtifact,
            Self::OwnerLocalCodePosition(_) => RuntimeLocalType::OwnerLocalCodePosition,
            Self::LocalLineTarget(_) => RuntimeLocalType::LocalLineTarget,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::block_code::{BlockCodeBuildError, BlockCodeBuilder};
    use crate::global_variable::GlobalVariables;
    use crate::instruction::Instruction;
    use crate::lexer::Lexer;
    use crate::operator::register_operator_primitives;
    use crate::primitive::PrimitiveRegistry;
    use crate::source::SourceTexts;
    use crate::source_mapping::SourceMappedCode;
    use crate::source_word_ir::{
        FixedToken, LocalBinding, LocalReference, SourceProcessingInstruction,
        SourceWordImplementationBuilder,
    };
    use crate::value::Value;
    use crate::word::PublishedWords;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test name should normalize")
    }

    fn local(input: &str, span: SourceSpan) -> LocalBinding {
        LocalBinding::new(name(input), span)
    }

    fn local_ref(input: &str, span: SourceSpan) -> LocalReference {
        LocalReference::new(name(input), span)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn origin(span: SourceSpan) -> SourceInstructionOrigin {
        SourceInstructionOrigin::new(span)
    }

    fn instruction(
        operation: SourceProcessingOperation,
        span: SourceSpan,
    ) -> SourceProcessingInstruction {
        SourceProcessingInstruction::new(operation, origin(span))
    }

    fn lex(text: &str) -> (SourceTexts, SourceId, Vec<Token>) {
        let mut sources = SourceTexts::new();
        let source_id = sources.register(text);
        let mut lexer = Lexer::new(sources.view(), source_id).expect("lexer should construct");
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token().expect("source should lex");
            if token.kind() == TokenKind::Eof {
                break;
            }
            tokens.push(token);
        }
        (sources, source_id, tokens)
    }

    fn complete(
        instructions: impl IntoIterator<Item = SourceProcessingInstruction>,
    ) -> SourceWordImplementation {
        let mut builder = SourceWordImplementationBuilder::new();
        for instruction in instructions {
            builder.push(instruction);
        }
        builder
            .complete()
            .expect("test implementation should validate")
    }

    fn operator_lookup() -> OperatorLookup {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        register_operator_primitives(&mut primitives, &mut words).lookup()
    }

    #[derive(Debug)]
    struct FailingPatchTarget {
        len: usize,
    }

    impl FailingPatchTarget {
        fn new(len: usize) -> Self {
            Self { len }
        }
    }

    impl InstructionBuildTarget for FailingPatchTarget {
        fn current_len(&self) -> usize {
            self.len
        }

        fn append_mapped(
            &mut self,
            _instruction: Instruction,
            _span: SourceSpan,
        ) -> Result<InstructionAddress, InstructionBuildError> {
            unreachable!("patch failure test does not append instructions")
        }

        fn append_unmapped(
            &mut self,
            _instruction: Instruction,
        ) -> Result<InstructionAddress, InstructionBuildError> {
            unreachable!("patch failure test does not append instructions")
        }

        fn append_resolved_mapped(
            &mut self,
            _instruction: Instruction,
            _span: SourceSpan,
        ) -> Result<InstructionAddress, InstructionBuildError> {
            unreachable!("patch failure test does not append instructions")
        }

        fn append_resolved_unmapped(
            &mut self,
            _instruction: Instruction,
        ) -> Result<InstructionAddress, InstructionBuildError> {
            unreachable!("patch failure test does not append instructions")
        }

        fn append_mapped_jump_placeholder(
            &mut self,
            _span: SourceSpan,
        ) -> Result<InstructionAddress, InstructionBuildError> {
            unreachable!("patch failure test does not append instructions")
        }

        fn append_mapped_jump_if_zero_placeholder(
            &mut self,
            _span: SourceSpan,
        ) -> Result<InstructionAddress, InstructionBuildError> {
            unreachable!("patch failure test does not append instructions")
        }

        fn patch_branch_target(
            &mut self,
            branch: InstructionAddress,
            _target: InstructionAddress,
        ) -> Result<(), InstructionBuildError> {
            Err(InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::UnknownBranchPatch { branch },
            })
        }

        fn validate_local_target(
            &self,
            _address: InstructionAddress,
        ) -> Result<(), InstructionBuildError> {
            Ok(())
        }
    }

    #[test]
    fn evaluates_read_name_resolve_var_expression_and_store() {
        let (sources, source_id, tokens) = lex("LET A = 1 + 2");
        let view = sources.view();
        let mut globals = GlobalVariables::new();
        let variable = globals.allocate();
        let mut bindings = Bindings::new();
        bindings
            .insert_new(name("A"), Binding::Variable(variable))
            .expect("variable binding should insert");
        let implementation = complete([
            instruction(
                SourceProcessingOperation::ReadName {
                    bind: local("name", span(view, source_id, 4, 5)),
                },
                span(view, source_id, 0, 3),
            ),
            instruction(
                SourceProcessingOperation::ResolveVariable {
                    name: local_ref("name", span(view, source_id, 4, 5)),
                    bind: local("target", span(view, source_id, 4, 5)),
                },
                span(view, source_id, 0, 3),
            ),
            instruction(
                SourceProcessingOperation::Expect {
                    token: FixedToken::Equal,
                },
                span(view, source_id, 6, 7),
            ),
            instruction(
                SourceProcessingOperation::ReadExpression {
                    bind: local("expr", span(view, source_id, 8, 13)),
                },
                span(view, source_id, 8, 13),
            ),
            instruction(
                SourceProcessingOperation::EmitExpression {
                    expression: local_ref("expr", span(view, source_id, 8, 13)),
                },
                span(view, source_id, 8, 13),
            ),
            instruction(
                SourceProcessingOperation::EmitStore {
                    target: local_ref("target", span(view, source_id, 4, 5)),
                },
                span(view, source_id, 0, 3),
            ),
        ]);
        let operators = operator_lookup();
        let mut code = SourceMappedCode::new();
        {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let mut line_numbers = LocalLineNumberTable::new();
            let mut context =
                UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                    view,
                    source_id,
                    tokens: &tokens,
                    bindings: &bindings,
                    operators: Some(operators),
                    code: &mut builder,
                    line_numbers: &mut line_numbers,
                    capabilities: SourceProcessingCapabilities::statement_runtime(),
                });

            evaluate_source_word(&implementation, &mut context).expect("evaluation should succeed");
            builder.finish().expect("block should complete");
        }

        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(0)),
            Ok(&Instruction::Push(Value::integer(1)))
        );
        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(2)),
            Ok(&Instruction::Call(
                operators.resolve(crate::operator::OperatorSemantic::Add)
            ))
        );
        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(3)),
            Ok(&Instruction::StoreVar(variable))
        );
    }

    #[test]
    fn read_expression_until_leaves_delimiter_for_following_expect() {
        let (sources, source_id, tokens) = lex("BIF 0, 100");
        let view = sources.view();
        let bindings = Bindings::new();
        let implementation = complete([
            instruction(
                SourceProcessingOperation::ReadExpressionUntil {
                    delimiter: FixedToken::Comma,
                    bind: local("condition", span(view, source_id, 4, 5)),
                },
                span(view, source_id, 0, 3),
            ),
            instruction(
                SourceProcessingOperation::Expect {
                    token: FixedToken::Comma,
                },
                span(view, source_id, 5, 6),
            ),
            instruction(
                SourceProcessingOperation::ReadLineNumber {
                    bind: local("line", span(view, source_id, 7, 10)),
                },
                span(view, source_id, 7, 10),
            ),
            instruction(
                SourceProcessingOperation::ExpectEnd,
                span(view, source_id, 10, 10),
            ),
            instruction(
                SourceProcessingOperation::EmitExpression {
                    expression: local_ref("condition", span(view, source_id, 4, 5)),
                },
                span(view, source_id, 4, 5),
            ),
            instruction(
                SourceProcessingOperation::EmitBranchIfFalse {
                    destination: local_ref("line", span(view, source_id, 7, 10)),
                },
                span(view, source_id, 0, 3),
            ),
        ]);
        let mut code = SourceMappedCode::new();
        let target;
        {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let mut line_numbers = LocalLineNumberTable::new();
            let mut context =
                UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                    view,
                    source_id,
                    tokens: &tokens,
                    bindings: &bindings,
                    operators: Some(operator_lookup()),
                    code: &mut builder,
                    line_numbers: &mut line_numbers,
                    capabilities: SourceProcessingCapabilities::statement_runtime(),
                });

            evaluate_source_word(&implementation, &mut context).expect("evaluation should succeed");
            target = builder
                .append_mapped(Instruction::Return, span(view, source_id, 7, 10))
                .expect("target should append");
            line_numbers
                .define(
                    &builder,
                    LocalLineNumber::new(100),
                    target,
                    span(view, source_id, 7, 10),
                )
                .expect("line should define");
            line_numbers
                .resolve(&mut builder)
                .expect("line target should patch");
            builder.finish().expect("block should complete");
        }

        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(1)),
            Ok(&Instruction::JumpIfZero(target))
        );
    }

    #[test]
    fn position_can_feed_explicit_branch_destination() {
        let (sources, source_id, tokens) = lex("LOOP");
        let view = sources.view();
        let bindings = Bindings::new();
        let implementation = complete([
            instruction(
                SourceProcessingOperation::Position {
                    bind: local("start", span(view, source_id, 0, 4)),
                },
                span(view, source_id, 0, 4),
            ),
            instruction(
                SourceProcessingOperation::EmitBranch {
                    destination: local_ref("start", span(view, source_id, 0, 4)),
                },
                span(view, source_id, 0, 4),
            ),
        ]);
        let mut code = SourceMappedCode::new();
        {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let mut line_numbers = LocalLineNumberTable::new();
            let mut context =
                UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                    view,
                    source_id,
                    tokens: &tokens,
                    bindings: &bindings,
                    operators: Some(operator_lookup()),
                    code: &mut builder,
                    line_numbers: &mut line_numbers,
                    capabilities: SourceProcessingCapabilities::statement_runtime(),
                });

            evaluate_source_word(&implementation, &mut context).expect("evaluation should succeed");
            builder.finish().expect("block should complete");
        }

        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(0)),
            Ok(&Instruction::Jump(InstructionAddress::from_index(0)))
        );
    }

    #[test]
    fn rejects_missing_capability_before_reading_or_emitting() {
        let (sources, source_id, tokens) = lex("RETURN");
        let view = sources.view();
        let bindings = Bindings::new();
        let implementation = complete([instruction(
            SourceProcessingOperation::EmitReturn,
            span(view, source_id, 0, 6),
        )]);
        let mut code = SourceMappedCode::new();
        let error = {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let mut line_numbers = LocalLineNumberTable::new();
            let mut context =
                UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                    view,
                    source_id,
                    tokens: &tokens,
                    bindings: &bindings,
                    operators: Some(operator_lookup()),
                    code: &mut builder,
                    line_numbers: &mut line_numbers,
                    capabilities: SourceProcessingCapabilities::empty(),
                });

            evaluate_source_word(&implementation, &mut context)
                .expect_err("missing capability should fail")
        };

        assert!(matches!(
            error,
            SourceWordEvaluationError::CapabilityUnavailable { .. }
        ));
        assert_eq!(code.len(), 0);
    }

    #[test]
    fn failed_resolution_does_not_emit_partial_runtime_code() {
        let (sources, source_id, tokens) = lex("LET MISSING = 1");
        let view = sources.view();
        let bindings = Bindings::new();
        let implementation = complete([
            instruction(
                SourceProcessingOperation::ReadName {
                    bind: local("name", span(view, source_id, 4, 11)),
                },
                span(view, source_id, 0, 3),
            ),
            instruction(
                SourceProcessingOperation::ResolveVariable {
                    name: local_ref("name", span(view, source_id, 4, 11)),
                    bind: local("target", span(view, source_id, 4, 11)),
                },
                span(view, source_id, 0, 3),
            ),
            instruction(
                SourceProcessingOperation::Expect {
                    token: FixedToken::Equal,
                },
                span(view, source_id, 12, 13),
            ),
            instruction(
                SourceProcessingOperation::ReadExpression {
                    bind: local("expr", span(view, source_id, 14, 15)),
                },
                span(view, source_id, 14, 15),
            ),
            instruction(
                SourceProcessingOperation::EmitExpression {
                    expression: local_ref("expr", span(view, source_id, 14, 15)),
                },
                span(view, source_id, 14, 15),
            ),
        ]);
        let mut code = SourceMappedCode::new();
        let error = {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let mut line_numbers = LocalLineNumberTable::new();
            let mut context =
                UserDefinedSourceWordContext::new(UserDefinedSourceWordContextParts {
                    view,
                    source_id,
                    tokens: &tokens,
                    bindings: &bindings,
                    operators: Some(operator_lookup()),
                    code: &mut builder,
                    line_numbers: &mut line_numbers,
                    capabilities: SourceProcessingCapabilities::statement_runtime(),
                });

            evaluate_source_word(&implementation, &mut context)
                .expect_err("undefined variable should fail")
        };

        assert!(matches!(
            error,
            SourceWordEvaluationError::VariableResolution { .. }
        ));
        assert_eq!(code.len(), 0);
    }

    #[test]
    fn failed_structural_branch_patch_keeps_unfinished_patch_state() {
        let (sources, source_id, _tokens) = lex("IF 0");
        let view = sources.view();
        let origin = origin(span(view, source_id, 0, 2));
        let mut state = SourceWordEvaluationState::new();
        let branch = InstructionAddress::from_index(0);
        state.structural_branches.record_following(branch, origin);
        let mut target = FailingPatchTarget::new(1);

        let error = state
            .complete_structural_branches(&mut target)
            .expect_err("failed patch should reject structural completion");

        assert!(matches!(
            error,
            SourceWordEvaluationError::InstructionBuild {
                source: InstructionBuildError::BlockCodeBuild {
                    source: BlockCodeBuildError::UnknownBranchPatch { branch: actual }
                },
                ..
            } if actual == branch
        ));
        assert_eq!(state.structural_branches.following.len(), 1);
        assert_eq!(state.structural_branches.following[0].branch, branch);
        assert!(state.structural_branches.complete.is_empty());
    }
}
