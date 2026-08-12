use crate::instruction::{Instruction, InstructionSequence};
use crate::lexer::Token;
use crate::source::{SourceId, SourceSpan, SourceView};
use crate::source_mapping::{InstructionSourceMapping, SourceMappingAppendError};

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
    fn(&mut NativeSourceWordContext<'_>) -> Result<(), SourceWordError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceWordError {
    SourceMappingAppend { source: SourceMappingAppendError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceWordLookupError {
    InvalidSourceWordId { id: SourceWordId },
}

/// Narrow source-processing capability passed to native source words.
///
/// This is deliberately smaller than a compiler or VM handle. Native source
/// words can inspect the current logical statement and emit mapped temporary
/// instructions, but they cannot mutate bindings, words, globals, runtime VM
/// state, or published code spaces through this context.
pub(crate) struct NativeSourceWordContext<'a> {
    view: SourceView<'a>,
    source_id: SourceId,
    tokens: &'a [Token],
    instructions: &'a mut InstructionSequence,
    mapping: &'a mut InstructionSourceMapping,
}

impl<'a> NativeSourceWordContext<'a> {
    pub(crate) fn new(
        view: SourceView<'a>,
        source_id: SourceId,
        tokens: &'a [Token],
        instructions: &'a mut InstructionSequence,
        mapping: &'a mut InstructionSourceMapping,
    ) -> Self {
        Self {
            view,
            source_id,
            tokens,
            instructions,
            mapping,
        }
    }

    pub(crate) const fn view(&self) -> SourceView<'a> {
        self.view
    }

    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub(crate) const fn tokens(&self) -> &'a [Token] {
        self.tokens
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

    fn push_one(context: &mut NativeSourceWordContext<'_>) -> Result<(), SourceWordError> {
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
        let mut context = NativeSourceWordContext::new(
            sources.view(),
            source_id,
            &tokens[..1],
            &mut instructions,
            &mut mapping,
        );

        push_one(&mut context).expect("test source word should emit");

        assert_eq!(
            instructions
                .view()
                .get(crate::instruction::InstructionAddress::from_index(0)),
            Ok(&Instruction::Push(Value::integer(1)))
        );
    }
}
