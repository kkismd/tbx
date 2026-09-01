use std::collections::HashMap;

use crate::lexer::TokenKind;
use crate::name::NormalizedName;
use crate::source::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceWordImplementation {
    instructions: Vec<SourceProcessingInstruction>,
    capabilities: SourceProcessingCapabilities,
}

impl SourceWordImplementation {
    pub(crate) fn from_prevalidated_instructions(
        instructions: Vec<SourceProcessingInstruction>,
    ) -> Self {
        let capabilities = instructions.iter().fold(
            SourceProcessingCapabilities::empty(),
            |mut capabilities, instruction| {
                capabilities.include(instruction.operation.required_capabilities());
                capabilities
            },
        );
        Self {
            instructions,
            capabilities,
        }
    }

    pub(crate) fn instructions(&self) -> &[SourceProcessingInstruction] {
        &self.instructions
    }

    pub(crate) const fn capabilities(&self) -> SourceProcessingCapabilities {
        self.capabilities
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceProcessingInstruction {
    operation: SourceProcessingOperation,
    origin: SourceInstructionOrigin,
}

impl SourceProcessingInstruction {
    pub(crate) const fn new(
        operation: SourceProcessingOperation,
        origin: SourceInstructionOrigin,
    ) -> Self {
        Self { operation, origin }
    }

    pub(crate) const fn operation(&self) -> &SourceProcessingOperation {
        &self.operation
    }

    pub(crate) const fn origin(&self) -> SourceInstructionOrigin {
        self.origin
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceProcessingOperation {
    ReadName {
        bind: LocalBinding,
    },
    Expect {
        token: FixedToken,
    },
    ExpectEnd,
    ReadLineNumber {
        bind: LocalBinding,
    },
    ReadExpression {
        bind: LocalBinding,
    },
    // #1558 keeps delimiter recognition separate from delimiter consumption:
    // evaluation reads the expression before this fixed token, and a later
    // Expect operation must consume the delimiter explicitly.
    ReadExpressionUntil {
        delimiter: FixedToken,
        bind: LocalBinding,
    },
    ResolveVariable {
        name: LocalReference,
        bind: LocalBinding,
    },
    EmitExpression {
        expression: LocalReference,
    },
    EmitStore {
        target: LocalReference,
    },
    EmitReturn,
    Position {
        bind: LocalBinding,
    },
    EmitBranch {
        destination: LocalReference,
    },
    EmitBranchIfFalse {
        destination: LocalReference,
    },
    // #1561 keeps branch placeholders out of user-authored locals. FOLLOWING
    // and COMPLETE are constrained operations so owner-local lowering can keep
    // patch state private instead of exposing a generic TARGET primitive.
    EmitBranchFollowing,
    EmitBranchIfFalseFollowing,
    EmitBranchComplete,
    EmitBranchIfFalseComplete,
}

impl SourceProcessingOperation {
    pub(crate) fn produced_binding_for_validation(&self) -> Option<&LocalBinding> {
        self.produced_binding()
    }

    pub(crate) fn consumed_local_references(&self) -> impl Iterator<Item = &LocalReference> {
        self.consumed_locals()
            .map(|(reference, _consumer)| reference)
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn produced_local_type(&self) -> Option<SourceLocalType> {
        match self {
            Self::ReadName { .. } => Some(SourceLocalType::NameInput),
            Self::ReadLineNumber { .. } => Some(SourceLocalType::LocalLineTarget {
                scope: SourceLineNumberScope::CurrentOwner,
            }),
            Self::ReadExpression { .. } | Self::ReadExpressionUntil { .. } => {
                Some(SourceLocalType::ExpressionArtifact)
            }
            Self::ResolveVariable { .. } => Some(SourceLocalType::VariableTarget),
            Self::Position { .. } => Some(SourceLocalType::OwnerLocalCodePosition {
                code_space: SourceCodeSpace::CurrentOwner,
            }),
            Self::Expect { .. }
            | Self::ExpectEnd
            | Self::EmitExpression { .. }
            | Self::EmitStore { .. }
            | Self::EmitReturn
            | Self::EmitBranch { .. }
            | Self::EmitBranchIfFalse { .. }
            | Self::EmitBranchFollowing
            | Self::EmitBranchIfFalseFollowing
            | Self::EmitBranchComplete
            | Self::EmitBranchIfFalseComplete => None,
        }
    }

    fn produced_binding(&self) -> Option<&LocalBinding> {
        match self {
            Self::ReadName { bind }
            | Self::ReadLineNumber { bind }
            | Self::ReadExpression { bind }
            | Self::ReadExpressionUntil { bind, .. }
            | Self::ResolveVariable { bind, .. }
            | Self::Position { bind } => Some(bind),
            Self::Expect { .. }
            | Self::ExpectEnd
            | Self::EmitExpression { .. }
            | Self::EmitStore { .. }
            | Self::EmitReturn
            | Self::EmitBranch { .. }
            | Self::EmitBranchIfFalse { .. }
            | Self::EmitBranchFollowing
            | Self::EmitBranchIfFalseFollowing
            | Self::EmitBranchComplete
            | Self::EmitBranchIfFalseComplete => None,
        }
    }

    fn consumed_locals(&self) -> impl Iterator<Item = (&LocalReference, LocalConsumer)> {
        let mut locals = Vec::new();
        match self {
            Self::ResolveVariable { name, .. } => {
                locals.push((name, LocalConsumer::Exact(SourceLocalType::NameInput)));
            }
            Self::EmitExpression { expression } => {
                locals.push((
                    expression,
                    LocalConsumer::Exact(SourceLocalType::ExpressionArtifact),
                ));
            }
            Self::EmitStore { target } => {
                locals.push((
                    target,
                    LocalConsumer::Exact(SourceLocalType::VariableTarget),
                ));
            }
            Self::EmitBranch { destination } | Self::EmitBranchIfFalse { destination } => {
                locals.push((destination, LocalConsumer::BranchDestination));
            }
            Self::ReadName { .. }
            | Self::Expect { .. }
            | Self::ExpectEnd
            | Self::ReadLineNumber { .. }
            | Self::ReadExpression { .. }
            | Self::ReadExpressionUntil { .. }
            | Self::EmitReturn
            | Self::Position { .. }
            | Self::EmitBranchFollowing
            | Self::EmitBranchIfFalseFollowing
            | Self::EmitBranchComplete
            | Self::EmitBranchIfFalseComplete => {}
        }
        locals.into_iter()
    }

    fn required_capabilities(&self) -> SourceProcessingCapabilities {
        match self {
            Self::ReadName { .. } => SourceProcessingCapabilities::read_name(),
            Self::Expect { .. } => SourceProcessingCapabilities::expect_fixed_token(),
            Self::ExpectEnd => SourceProcessingCapabilities::expect_end(),
            Self::ReadLineNumber { .. } => SourceProcessingCapabilities::read_line_number(),
            Self::ReadExpression { .. } | Self::ReadExpressionUntil { .. } => {
                SourceProcessingCapabilities::read_expression()
            }
            Self::ResolveVariable { .. } => SourceProcessingCapabilities::resolve_variable(),
            Self::EmitExpression { .. }
            | Self::EmitStore { .. }
            | Self::EmitReturn
            | Self::Position { .. }
            | Self::EmitBranch { .. }
            | Self::EmitBranchIfFalse { .. } => SourceProcessingCapabilities::emit_runtime_code(),
            Self::EmitBranchFollowing
            | Self::EmitBranchIfFalseFollowing
            | Self::EmitBranchComplete
            | Self::EmitBranchIfFalseComplete => {
                SourceProcessingCapabilities::emit_structural_branch()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceInstructionOrigin {
    span: SourceSpan,
}

impl SourceInstructionOrigin {
    pub(crate) const fn new(span: SourceSpan) -> Self {
        Self { span }
    }

    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixedToken {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Comma,
    LeftParen,
    RightParen,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl FixedToken {
    pub(crate) const fn token_kind(self) -> TokenKind {
        match self {
            Self::Plus => TokenKind::Plus,
            Self::Minus => TokenKind::Minus,
            Self::Star => TokenKind::Star,
            Self::Slash => TokenKind::Slash,
            Self::Percent => TokenKind::Percent,
            Self::Comma => TokenKind::Comma,
            Self::LeftParen => TokenKind::LParen,
            Self::RightParen => TokenKind::RParen,
            Self::Equal => TokenKind::Equal,
            Self::NotEqual => TokenKind::NotEqual,
            Self::Less => TokenKind::Less,
            Self::LessEqual => TokenKind::LessEqual,
            Self::Greater => TokenKind::Greater,
            Self::GreaterEqual => TokenKind::GreaterEqual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalBinding {
    name: NormalizedName,
    span: SourceSpan,
}

impl LocalBinding {
    pub(crate) fn new(name: NormalizedName, span: SourceSpan) -> Self {
        Self { name, span }
    }

    pub(crate) fn name(&self) -> &NormalizedName {
        &self.name
    }

    pub(crate) const fn span(&self) -> SourceSpan {
        self.span
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalReference {
    name: NormalizedName,
    span: SourceSpan,
}

impl LocalReference {
    pub(crate) fn new(name: NormalizedName, span: SourceSpan) -> Self {
        Self { name, span }
    }

    pub(crate) fn name(&self) -> &NormalizedName {
        &self.name
    }

    pub(crate) const fn span(&self) -> SourceSpan {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceLocalType {
    NameInput,
    VariableTarget,
    ExpressionArtifact,
    // #1561 requires owner-local code positions and line targets to remain
    // distinct even though both can be consumed as explicit branch destinations.
    OwnerLocalCodePosition { code_space: SourceCodeSpace },
    LocalLineTarget { scope: SourceLineNumberScope },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceCodeSpace {
    CurrentOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceLineNumberScope {
    CurrentOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalConsumer {
    Exact(SourceLocalType),
    BranchDestination,
}

impl LocalConsumer {
    fn accepts(self, local_type: SourceLocalType) -> bool {
        match self {
            Self::Exact(expected) => expected == local_type,
            Self::BranchDestination => matches!(
                local_type,
                SourceLocalType::OwnerLocalCodePosition { .. }
                    | SourceLocalType::LocalLineTarget { .. }
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SourceProcessingCapabilities {
    read_name: bool,
    expect_fixed_token: bool,
    expect_end: bool,
    read_line_number: bool,
    read_expression: bool,
    resolve_variable: bool,
    emit_runtime_code: bool,
    emit_structural_branch: bool,
}

impl SourceProcessingCapabilities {
    const fn read_name() -> Self {
        Self {
            read_name: true,
            ..Self::empty()
        }
    }

    const fn expect_fixed_token() -> Self {
        Self {
            expect_fixed_token: true,
            ..Self::empty()
        }
    }

    const fn expect_end() -> Self {
        Self {
            expect_end: true,
            ..Self::empty()
        }
    }

    const fn read_line_number() -> Self {
        Self {
            read_line_number: true,
            ..Self::empty()
        }
    }

    const fn read_expression() -> Self {
        Self {
            read_expression: true,
            ..Self::empty()
        }
    }

    const fn resolve_variable() -> Self {
        Self {
            resolve_variable: true,
            ..Self::empty()
        }
    }

    const fn emit_runtime_code() -> Self {
        Self {
            emit_runtime_code: true,
            ..Self::empty()
        }
    }

    const fn emit_structural_branch() -> Self {
        Self {
            emit_runtime_code: true,
            emit_structural_branch: true,
            ..Self::empty()
        }
    }

    pub(crate) const fn empty() -> Self {
        Self {
            read_name: false,
            expect_fixed_token: false,
            expect_end: false,
            read_line_number: false,
            read_expression: false,
            resolve_variable: false,
            emit_runtime_code: false,
            emit_structural_branch: false,
        }
    }

    pub(crate) const fn can_read_name(self) -> bool {
        self.read_name
    }

    pub(crate) const fn can_expect_fixed_token(self) -> bool {
        self.expect_fixed_token
    }

    pub(crate) const fn can_expect_end(self) -> bool {
        self.expect_end
    }

    pub(crate) const fn can_read_line_number(self) -> bool {
        self.read_line_number
    }

    pub(crate) const fn can_read_expression(self) -> bool {
        self.read_expression
    }

    pub(crate) const fn can_resolve_variable(self) -> bool {
        self.resolve_variable
    }

    pub(crate) const fn can_emit_runtime_code(self) -> bool {
        self.emit_runtime_code
    }

    pub(crate) const fn can_emit_structural_branch(self) -> bool {
        self.emit_structural_branch
    }

    pub(crate) const fn allows(self, required: Self) -> bool {
        (!required.read_name || self.read_name)
            && (!required.expect_fixed_token || self.expect_fixed_token)
            && (!required.expect_end || self.expect_end)
            && (!required.read_line_number || self.read_line_number)
            && (!required.read_expression || self.read_expression)
            && (!required.resolve_variable || self.resolve_variable)
            && (!required.emit_runtime_code || self.emit_runtime_code)
            && (!required.emit_structural_branch || self.emit_structural_branch)
    }

    pub(crate) const fn statement_runtime() -> Self {
        Self {
            read_name: true,
            expect_fixed_token: true,
            expect_end: true,
            read_line_number: true,
            read_expression: true,
            resolve_variable: true,
            emit_runtime_code: true,
            emit_structural_branch: false,
        }
    }

    pub(crate) const fn structured_runtime() -> Self {
        Self {
            emit_structural_branch: true,
            ..Self::statement_runtime()
        }
    }

    fn include(&mut self, other: Self) {
        self.read_name |= other.read_name;
        self.expect_fixed_token |= other.expect_fixed_token;
        self.expect_end |= other.expect_end;
        self.read_line_number |= other.read_line_number;
        self.read_expression |= other.read_expression;
        self.resolve_variable |= other.resolve_variable;
        self.emit_runtime_code |= other.emit_runtime_code;
        self.emit_structural_branch |= other.emit_structural_branch;
    }
}

#[derive(Debug, Default)]
pub(crate) struct SourceWordImplementationBuilder {
    instructions: Vec<SourceProcessingInstruction>,
}

impl SourceWordImplementationBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, instruction: SourceProcessingInstruction) {
        self.instructions.push(instruction);
    }

    pub(crate) fn complete(self) -> Result<SourceWordImplementation, SourceWordBuildError> {
        let mut locals: HashMap<NormalizedName, LocalDefinition> = HashMap::new();
        let mut capabilities = SourceProcessingCapabilities::empty();

        for instruction in &self.instructions {
            capabilities.include(instruction.operation.required_capabilities());

            for (reference, consumer) in instruction.operation.consumed_locals() {
                let Some(binding) = locals.get(reference.name()) else {
                    return Err(SourceWordBuildError::UndefinedLocal {
                        reference: reference.clone(),
                    });
                };

                if !consumer.accepts(binding.local_type) {
                    return Err(SourceWordBuildError::LocalTypeMismatch {
                        reference: reference.clone(),
                        actual: binding.local_type,
                        expected: ExpectedLocalType::from_consumer(consumer),
                    });
                }
            }

            if let (Some(local_type), Some(binding)) = (
                instruction.operation.produced_local_type(),
                instruction.operation.produced_binding(),
            ) {
                if let Some(existing) = locals.get(binding.name()) {
                    return Err(SourceWordBuildError::DuplicateLocalBinding {
                        name: binding.name().clone(),
                        first_span: existing.span,
                        duplicate_span: binding.span(),
                    });
                }

                locals.insert(
                    binding.name().clone(),
                    LocalDefinition {
                        local_type,
                        span: binding.span(),
                    },
                );
            }
        }

        Ok(SourceWordImplementation {
            instructions: self.instructions,
            capabilities,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceWordBuildError {
    UndefinedLocal {
        reference: LocalReference,
    },
    DuplicateLocalBinding {
        name: NormalizedName,
        first_span: SourceSpan,
        duplicate_span: SourceSpan,
    },
    LocalTypeMismatch {
        reference: LocalReference,
        actual: SourceLocalType,
        expected: ExpectedLocalType,
    },
}

impl SourceWordBuildError {
    pub(crate) const fn primary_span(&self) -> SourceSpan {
        match self {
            Self::UndefinedLocal { reference } | Self::LocalTypeMismatch { reference, .. } => {
                reference.span()
            }
            Self::DuplicateLocalBinding { duplicate_span, .. } => *duplicate_span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpectedLocalType {
    Exact(SourceLocalType),
    BranchDestination,
}

impl ExpectedLocalType {
    const fn from_consumer(consumer: LocalConsumer) -> Self {
        match consumer {
            LocalConsumer::Exact(local_type) => Self::Exact(local_type),
            LocalConsumer::BranchDestination => Self::BranchDestination,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LocalDefinition {
    local_type: SourceLocalType,
    span: SourceSpan,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceId, SourceTexts, SourceView};

    fn setup_source() -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let source_id = sources.register("READ_NAME AS name reference span padding\n", "test.tbx");
        (sources, source_id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn origin(span: SourceSpan) -> SourceInstructionOrigin {
        SourceInstructionOrigin::new(span)
    }

    fn name(input: &str, span: SourceSpan) -> LocalBinding {
        LocalBinding::new(NormalizedName::new(input).expect("valid local name"), span)
    }

    fn reference(input: &str, span: SourceSpan) -> LocalReference {
        LocalReference::new(NormalizedName::new(input).expect("valid local name"), span)
    }

    fn complete(
        instructions: impl IntoIterator<Item = SourceProcessingInstruction>,
    ) -> Result<SourceWordImplementation, SourceWordBuildError> {
        let mut builder = SourceWordImplementationBuilder::new();
        for instruction in instructions {
            builder.push(instruction);
        }
        builder.complete()
    }

    #[test]
    fn builds_distinct_initial_operations_and_preserves_origins() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let bind_span = span(view, source_id, 13, 17);
        let reference_span = span(view, source_id, 18, 22);
        let line_span = span(view, source_id, 0, 4);

        let instructions = [
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadName {
                    bind: name("name", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::Expect {
                    token: FixedToken::Equal,
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(SourceProcessingOperation::ExpectEnd, origin(op_span)),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadLineNumber {
                    bind: name("line", line_span),
                },
                origin(line_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadExpression {
                    bind: name("expr", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadExpressionUntil {
                    delimiter: FixedToken::Comma,
                    bind: name("condition", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ResolveVariable {
                    name: reference("name", reference_span),
                    bind: name("target", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitExpression {
                    expression: reference("expr", reference_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitStore {
                    target: reference("target", reference_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitReturn,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::Position {
                    bind: name("loop_start", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranch {
                    destination: reference("loop_start", reference_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchIfFalse {
                    destination: reference("line", reference_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchFollowing,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchIfFalseFollowing,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchComplete,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchIfFalseComplete,
                origin(op_span),
            ),
        ];

        let implementation = complete(instructions).expect("instruction sequence should validate");

        assert_eq!(implementation.instructions().len(), 17);
        assert_eq!(implementation.instructions()[0].origin().span(), op_span);
        assert_eq!(
            implementation.instructions()[5].operation(),
            &SourceProcessingOperation::ReadExpressionUntil {
                delimiter: FixedToken::Comma,
                bind: name("condition", bind_span),
            }
        );
        assert_eq!(FixedToken::Comma.token_kind(), TokenKind::Comma);
    }

    #[test]
    fn derives_capabilities_from_completed_body() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let bind_span = span(view, source_id, 13, 17);
        let reference_span = span(view, source_id, 18, 22);

        let implementation = complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadName {
                    bind: name("name", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ResolveVariable {
                    name: reference("name", reference_span),
                    bind: name("target", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitStore {
                    target: reference("target", reference_span),
                },
                origin(op_span),
            ),
        ])
        .expect("instruction sequence should validate");

        let capabilities = implementation.capabilities();
        assert!(capabilities.can_read_name());
        assert!(capabilities.can_resolve_variable());
        assert!(capabilities.can_emit_runtime_code());
        assert!(!capabilities.can_read_expression());
        assert!(!capabilities.can_emit_structural_branch());
    }

    #[test]
    fn rejects_use_before_definition() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let reference_span = span(view, source_id, 18, 22);

        let error = complete([SourceProcessingInstruction::new(
            SourceProcessingOperation::EmitExpression {
                expression: reference("expr", reference_span),
            },
            origin(op_span),
        )])
        .expect_err("undefined local should be rejected");

        assert_eq!(
            error,
            SourceWordBuildError::UndefinedLocal {
                reference: reference("expr", reference_span),
            }
        );
        assert_eq!(error.primary_span(), reference_span);
    }

    #[test]
    fn rejects_duplicate_binding_after_first_definition() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let first_span = span(view, source_id, 13, 17);
        let duplicate_span = span(view, source_id, 18, 22);
        let op_span = span(view, source_id, 0, 9);

        let error = complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadName {
                    bind: name("local", first_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadExpression {
                    bind: name("LOCAL", duplicate_span),
                },
                origin(op_span),
            ),
        ])
        .expect_err("duplicate local should be rejected case-insensitively");

        assert_eq!(
            error,
            SourceWordBuildError::DuplicateLocalBinding {
                name: NormalizedName::new("local").expect("valid name"),
                first_span,
                duplicate_span,
            }
        );
        assert_eq!(error.primary_span(), duplicate_span);
    }

    #[test]
    fn validates_producer_consumer_type_connections() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let bind_span = span(view, source_id, 13, 17);
        let reference_span = span(view, source_id, 18, 22);

        complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadExpression {
                    bind: name("expr", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitExpression {
                    expression: reference("expr", reference_span),
                },
                origin(op_span),
            ),
        ])
        .expect("expression artifact should feed EMIT_EXPR");

        complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadName {
                    bind: name("source_name", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ResolveVariable {
                    name: reference("source_name", reference_span),
                    bind: name("target", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitStore {
                    target: reference("target", reference_span),
                },
                origin(op_span),
            ),
        ])
        .expect("name input should resolve to variable target for EMIT_STORE");
    }

    #[test]
    fn rejects_type_mismatch_for_consumers() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let bind_span = span(view, source_id, 13, 17);
        let reference_span = span(view, source_id, 18, 22);

        let error = complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadName {
                    bind: name("not_expr", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitExpression {
                    expression: reference("not_expr", reference_span),
                },
                origin(op_span),
            ),
        ])
        .expect_err("name input is not an expression artifact");

        assert_eq!(
            error,
            SourceWordBuildError::LocalTypeMismatch {
                reference: reference("not_expr", reference_span),
                actual: SourceLocalType::NameInput,
                expected: ExpectedLocalType::Exact(SourceLocalType::ExpressionArtifact),
            }
        );
    }

    #[test]
    fn accepts_distinct_explicit_branch_destination_types() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let position_span = span(view, source_id, 13, 17);
        let line_span = span(view, source_id, 18, 22);

        complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::Position {
                    bind: name("loop_start", position_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadLineNumber {
                    bind: name("line", line_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranch {
                    destination: reference("loop_start", position_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchIfFalse {
                    destination: reference("line", line_span),
                },
                origin(op_span),
            ),
        ])
        .expect("owner-local position and local line target are valid branch targets");
    }

    #[test]
    fn rejects_non_destination_branch_operand() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);
        let bind_span = span(view, source_id, 13, 17);
        let reference_span = span(view, source_id, 18, 22);

        let error = complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::ReadName {
                    bind: name("name", bind_span),
                },
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranch {
                    destination: reference("name", reference_span),
                },
                origin(op_span),
            ),
        ])
        .expect_err("name input should not be a branch target");

        assert_eq!(
            error,
            SourceWordBuildError::LocalTypeMismatch {
                reference: reference("name", reference_span),
                actual: SourceLocalType::NameInput,
                expected: ExpectedLocalType::BranchDestination,
            }
        );
    }

    #[test]
    fn structural_branch_operations_do_not_require_locals() {
        let (sources, source_id) = setup_source();
        let view = sources.view();
        let op_span = span(view, source_id, 0, 9);

        let implementation = complete([
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchFollowing,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchIfFalseFollowing,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchComplete,
                origin(op_span),
            ),
            SourceProcessingInstruction::new(
                SourceProcessingOperation::EmitBranchIfFalseComplete,
                origin(op_span),
            ),
        ])
        .expect("structural branch operations should validate without operand locals");

        let capabilities = implementation.capabilities();
        assert!(capabilities.can_emit_runtime_code());
        assert!(capabilities.can_emit_structural_branch());
    }
}
