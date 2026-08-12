use crate::binding::{Binding, BindingInsertError, Bindings};
use crate::expression::{
    parse_expression, ExpressionError, ExpressionStaging, ExpressionVariableErrorKind,
};
use crate::global_variable::GlobalVariables;
use crate::instruction::{Instruction, InstructionSequence};
use crate::lexer::{Token, TokenKind};
use crate::name::{NameError, NormalizedName};
use crate::operator::OperatorLookup;
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::source_mapping::{InstructionSourceMapping, SourceMappingAppendError};
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
pub(crate) enum SourceWordError {
    Source {
        source: SourceError,
    },
    SourceMappingAppend {
        source: SourceMappingAppendError,
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
    tokens: &'source [Token],
    bindings: NativeSourceWordBindingAccess<'state>,
    operators: Option<OperatorLookup>,
    instructions: &'state mut InstructionSequence,
    mapping: &'state mut InstructionSourceMapping,
    local_line_number_prefix: Option<SourceSpan>,
    globals: Option<&'state mut GlobalVariables>,
}

pub(crate) struct NativeSourceWordContextParts<'source, 'state> {
    pub(crate) view: SourceView<'source>,
    pub(crate) source_id: SourceId,
    pub(crate) tokens: &'source [Token],
    pub(crate) bindings: NativeSourceWordBindingAccess<'state>,
    pub(crate) operators: Option<OperatorLookup>,
    pub(crate) instructions: &'state mut InstructionSequence,
    pub(crate) mapping: &'state mut InstructionSourceMapping,
    pub(crate) local_line_number_prefix: Option<SourceSpan>,
    pub(crate) globals: Option<&'state mut GlobalVariables>,
}

impl<'source, 'state> NativeSourceWordContext<'source, 'state> {
    pub(crate) fn new(parts: NativeSourceWordContextParts<'source, 'state>) -> Self {
        Self {
            view: parts.view,
            source_id: parts.source_id,
            tokens: parts.tokens,
            bindings: parts.bindings,
            operators: parts.operators,
            instructions: parts.instructions,
            mapping: parts.mapping,
            local_line_number_prefix: parts.local_line_number_prefix,
            globals: parts.globals,
        }
    }

    pub(crate) const fn view(&self) -> SourceView<'source> {
        self.view
    }

    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub(crate) const fn tokens(&self) -> &'source [Token] {
        self.tokens
    }

    pub(crate) const fn local_line_number_prefix(&self) -> Option<SourceSpan> {
        self.local_line_number_prefix
    }

    pub(crate) fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<(), SourceWordError> {
        let address = self.instructions.append(instruction);
        self.mapping
            .append_mapped(address, span)
            .map_err(|source| SourceWordError::SourceMappingAppend { source })
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

        if bindings.get(&name).is_some() {
            return Err(SourceWordError::VarNameConflict { span });
        }

        let id = globals.allocate();
        // #1370/#1478/#1487 make binding insertion the VAR commit point:
        // after this succeeds, no recoverable fallible work may remain here.
        bindings
            .insert_new(name, Binding::Variable(id))
            .map_err(|source| match source {
                BindingInsertError::NameConflict => {
                    SourceWordError::VarBindingCommitInvariantViolated { span }
                }
            })
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
        staging
            .commit_to(self.instructions, self.mapping)
            .map_err(|source| match source {
                ExpressionError::SourceMappingAppend(source) => {
                    SourceWordError::SourceMappingAppend { source }
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

    let var = context
        .tokens()
        .first()
        .copied()
        .expect("VAR source word requires its leading token");
    let Some(name_token) = context.tokens().get(1).copied() else {
        return Err(SourceWordError::VarSyntax {
            span: var.span(),
            kind: VarSyntaxErrorKind::MissingName,
        });
    };
    if name_token.kind() != TokenKind::Name {
        return Err(SourceWordError::VarSyntax {
            span: name_token.span(),
            kind: VarSyntaxErrorKind::MissingName,
        });
    }
    if let Some(trailing) = context.tokens().get(2).copied() {
        return Err(SourceWordError::VarSyntax {
            span: trailing.span(),
            kind: VarSyntaxErrorKind::TrailingToken {
                kind: trailing.kind(),
            },
        });
    }

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
    let let_token = context
        .tokens()
        .first()
        .copied()
        .expect("LET source word requires its leading token");
    let Some(target_token) = context.tokens().get(1).copied() else {
        return Err(SourceWordError::LetSyntax {
            span: let_token.span(),
            kind: LetSyntaxErrorKind::Target,
        });
    };
    if target_token.kind() != TokenKind::Name {
        return Err(SourceWordError::LetSyntax {
            span: target_token.span(),
            kind: LetSyntaxErrorKind::Target,
        });
    }

    let Some(equal_token) = context.tokens().get(2).copied() else {
        return Err(SourceWordError::LetSyntax {
            span: target_token.span(),
            kind: LetSyntaxErrorKind::Equal,
        });
    };
    if equal_token.kind() != TokenKind::Equal {
        return Err(SourceWordError::LetSyntax {
            span: equal_token.span(),
            kind: LetSyntaxErrorKind::Equal,
        });
    }

    let rhs_tokens = &context.tokens()[3..];
    if rhs_tokens.is_empty() {
        return Err(SourceWordError::LetSyntax {
            span: equal_token.span(),
            kind: LetSyntaxErrorKind::Rhs,
        });
    }

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

    let mut staging = context.stage_expression(rhs_tokens, equal_token.span())?;
    staging.append_mapped_instruction(Instruction::StoreVar(target), target_token.span());
    context.commit_staging(&staging)
}

pub(crate) fn unsupported_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    let first = context
        .tokens()
        .first()
        .copied()
        .expect("source word requires its leading token");
    Err(SourceWordError::UnsupportedSourceWord { span: first.span() })
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
    handlers: Vec<NativeSourceWordHandler>,
}

impl SourceWordRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, handler: NativeSourceWordHandler) -> SourceWordId {
        let id = SourceWordId::from_slot(self.handlers.len());
        self.handlers.push(handler);
        id
    }

    pub(crate) fn lookup(&self) -> SourceWordLookup<'_> {
        SourceWordLookup { registry: self }
    }

    pub(crate) fn len(&self) -> usize {
        self.handlers.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceWordLookup<'a> {
    registry: &'a SourceWordRegistry,
}

impl SourceWordLookup<'_> {
    pub(crate) fn lookup_handler(
        self,
        id: SourceWordId,
    ) -> Result<NativeSourceWordHandler, SourceWordLookupError> {
        self.registry
            .handlers
            .get(id.as_slot())
            .copied()
            .ok_or(SourceWordLookupError::InvalidSourceWordId { id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceTexts;
    use crate::value::Value;

    fn push_one(context: &mut NativeSourceWordContext<'_, '_>) -> Result<(), SourceWordError> {
        let first = context
            .tokens()
            .first()
            .copied()
            .expect("test source word should receive its leading token");
        context.append_mapped(Instruction::Push(Value::integer(1)), first.span())
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
        let mut instructions = InstructionSequence::new();
        let mut mapping = InstructionSourceMapping::new(instructions.code_space());
        let bindings = Bindings::new();
        let mut context = NativeSourceWordContext::new(NativeSourceWordContextParts {
            view: sources.view(),
            source_id,
            tokens: &tokens[..1],
            bindings: NativeSourceWordBindingAccess::Read(&bindings),
            operators: None,
            instructions: &mut instructions,
            mapping: &mut mapping,
            local_line_number_prefix: None,
            globals: None,
        });

        push_one(&mut context).expect("test source word should emit");

        assert_eq!(
            instructions
                .view()
                .get(crate::instruction::InstructionAddress::from_index(0)),
            Ok(&Instruction::Push(Value::integer(1)))
        );
    }
}
