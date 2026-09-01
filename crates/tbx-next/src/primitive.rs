use crate::runtime_output::{RuntimeOutput, RuntimeOutputError};
use crate::stack::{DataStack, StackError};
use crate::value::Value;
use crate::word::PrimitiveId;

pub(crate) type PrimitiveHandler = fn(&mut PrimitiveContext<'_>) -> Result<(), PrimitiveError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimitiveError {
    DataStackUnderflow { source: StackError },
    OutputFailed { source: RuntimeOutputError },
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimitiveLookupError {
    InvalidPrimitiveId { id: PrimitiveId },
}

/// Limited primitive execution context.
///
/// Handlers receive only data-stack operations and the narrow runtime output
/// capability. They cannot observe or mutate the instruction pointer, return
/// stack, halted flag, word tables, bindings, instruction sequence, primitive
/// registry, compiler, or source-processing state.
pub(crate) struct PrimitiveContext<'a> {
    data_stack: &'a mut DataStack,
    output: Option<&'a mut dyn RuntimeOutput>,
}

impl<'a> PrimitiveContext<'a> {
    pub(crate) fn new(data_stack: &'a mut DataStack) -> Self {
        Self {
            data_stack,
            output: None,
        }
    }

    pub(crate) fn with_output(
        data_stack: &'a mut DataStack,
        output: Option<&'a mut dyn RuntimeOutput>,
    ) -> Self {
        Self { data_stack, output }
    }

    pub(crate) fn push(&mut self, value: Value) {
        self.data_stack.push(value);
    }

    pub(crate) fn pop(&mut self) -> Result<Value, PrimitiveError> {
        self.data_stack
            .pop()
            .map_err(|source| PrimitiveError::DataStackUnderflow { source })
    }

    pub(crate) fn pop2(&mut self) -> Result<(Value, Value), PrimitiveError> {
        self.data_stack
            .pop2()
            .map_err(|source| PrimitiveError::DataStackUnderflow { source })
    }

    pub(crate) fn peek(&self) -> Result<Value, PrimitiveError> {
        self.data_stack
            .peek()
            .map_err(|source| PrimitiveError::DataStackUnderflow { source })
    }

    pub(crate) fn write_output(&mut self, text: &str) -> Result<(), PrimitiveError> {
        self.output
            .as_deref_mut()
            .ok_or(PrimitiveError::OutputFailed {
                source: RuntimeOutputError::Unavailable,
            })?
            .write(text)
            .map_err(|source| PrimitiveError::OutputFailed { source })
    }
}

#[derive(Debug, Default)]
pub(crate) struct PrimitiveRegistry {
    handlers: Vec<PrimitiveHandler>,
}

impl PrimitiveRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, handler: PrimitiveHandler) -> PrimitiveId {
        let id = PrimitiveId::from_slot(self.handlers.len());
        self.handlers.push(handler);
        id
    }

    pub(crate) fn lookup(&self) -> PrimitiveLookup<'_> {
        PrimitiveLookup { registry: self }
    }

    pub(crate) fn len(&self) -> usize {
        self.handlers.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PrimitiveLookup<'a> {
    registry: &'a PrimitiveRegistry,
}

impl<'a> PrimitiveLookup<'a> {
    pub(crate) fn lookup_handler(
        self,
        id: PrimitiveId,
    ) -> Result<PrimitiveHandler, PrimitiveLookupError> {
        self.registry
            .handlers
            .get(id.as_slot())
            .copied()
            .ok_or(PrimitiveLookupError::InvalidPrimitiveId { id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_one(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(Value::integer(1));
        Ok(())
    }

    #[test]
    fn registry_allocates_monotonic_primitive_ids() {
        let mut registry = PrimitiveRegistry::new();

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
        let mut registry = PrimitiveRegistry::new();
        let id = registry.register(push_one);
        let mut stack = DataStack::new();
        let handler = registry
            .lookup()
            .lookup_handler(id)
            .expect("handler should be registered");

        handler(&mut PrimitiveContext::new(&mut stack)).expect("handler should succeed");

        assert_eq!(stack.peek(), Ok(Value::integer(1)));
    }

    #[test]
    fn read_only_lookup_rejects_unregistered_primitive_id() {
        let registry = PrimitiveRegistry::new();
        let id = PrimitiveId::from_slot(0);

        assert_eq!(
            registry.lookup().lookup_handler(id),
            Err(PrimitiveLookupError::InvalidPrimitiveId { id })
        );
    }
}
