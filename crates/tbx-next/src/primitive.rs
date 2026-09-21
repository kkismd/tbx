use crate::random::{RandomError, RandomState};
use crate::runtime_input::{RuntimeInput, RuntimeInputError};
use crate::runtime_output::{RuntimeOutput, RuntimeOutputError};
use crate::stack::{DataStack, StackError};
use crate::value::Value;
use crate::word::PrimitiveId;

pub(crate) type PrimitiveHandler =
    for<'stack, 'cap> fn(&mut PrimitiveContext<'stack, 'cap>) -> Result<(), PrimitiveError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimitiveError {
    DataStackUnderflow { source: StackError },
    DataStackDepthOutOfRange { depth: usize },
    OutputFailed { source: RuntimeOutputError },
    InputFailed { source: RuntimeInputError },
    RandomUnavailable,
    InvalidRandomUpperBound { upper_bound: i16 },
    AsciiOutOfRange { value: i16 },
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimitiveLookupError {
    InvalidPrimitiveId { id: PrimitiveId },
}

/// Limited primitive execution context.
///
/// Handlers receive only data-stack operations and the narrow runtime output
/// capabilities. They cannot observe or mutate the instruction pointer,
/// return stack, halted flag, word tables, bindings, instruction sequence,
/// primitive registry, compiler, or source-processing state.
pub(crate) struct PrimitiveContext<'stack, 'cap> {
    data_stack: &'stack mut DataStack,
    capabilities: PrimitiveCapabilities<'cap>,
}

pub(crate) struct PrimitiveCapabilities<'cap> {
    pub(crate) output: Option<&'cap mut (dyn RuntimeOutput + 'cap)>,
    pub(crate) input: Option<&'cap mut (dyn RuntimeInput + 'cap)>,
    pub(crate) random: Option<&'cap mut RandomState>,
}

impl<'stack, 'cap> PrimitiveContext<'stack, 'cap> {
    pub(crate) fn new(data_stack: &'stack mut DataStack) -> Self {
        Self {
            data_stack,
            capabilities: PrimitiveCapabilities {
                output: None,
                input: None,
                random: None,
            },
        }
    }

    pub(crate) fn with_output(
        data_stack: &'stack mut DataStack,
        output: Option<&'cap mut dyn RuntimeOutput>,
    ) -> Self {
        Self {
            data_stack,
            capabilities: PrimitiveCapabilities {
                output,
                input: None,
                random: None,
            },
        }
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

    pub(crate) fn peek2(&self) -> Result<(Value, Value), PrimitiveError> {
        self.data_stack
            .peek2()
            .map_err(|source| PrimitiveError::DataStackUnderflow { source })
    }

    pub(crate) fn data_stack_is_empty(&self) -> bool {
        self.data_stack.is_empty()
    }

    pub(crate) fn data_stack_depth(&self) -> usize {
        self.data_stack.depth()
    }

    pub(crate) fn write_output(&mut self, text: &str) -> Result<(), PrimitiveError> {
        self.capabilities
            .output
            .as_deref_mut()
            .ok_or(PrimitiveError::OutputFailed {
                source: RuntimeOutputError::Unavailable,
            })?
            .write(text)
            .map_err(|source| PrimitiveError::OutputFailed { source })
    }

    pub(crate) fn read_input(&mut self) -> Result<Option<String>, PrimitiveError> {
        self.capabilities
            .input
            .as_deref_mut()
            .ok_or(PrimitiveError::InputFailed {
                source: RuntimeInputError::Unavailable,
            })?
            .read_line()
            .map_err(|source| PrimitiveError::InputFailed { source })
    }

    pub(crate) fn with_input(mut self, input: Option<&'cap mut dyn RuntimeInput>) -> Self {
        self.capabilities.input = input;
        self
    }

    pub(crate) fn random_inclusive(&mut self, upper_bound: i16) -> Result<i16, PrimitiveError> {
        self.capabilities
            .random
            .as_deref_mut()
            .ok_or(PrimitiveError::RandomUnavailable)?
            .next_inclusive(upper_bound)
            .map_err(|error| match error {
                RandomError::InvalidUpperBound { upper_bound } => {
                    PrimitiveError::InvalidRandomUpperBound { upper_bound }
                }
            })
    }

    pub(crate) fn with_capabilities(
        data_stack: &'stack mut DataStack,
        output: Option<&'cap mut dyn RuntimeOutput>,
        input: Option<&'cap mut dyn RuntimeInput>,
        random: Option<&'cap mut RandomState>,
    ) -> Self {
        Self {
            data_stack,
            capabilities: PrimitiveCapabilities {
                output,
                input,
                random,
            },
        }
    }

    pub(crate) fn into_capabilities(self) -> PrimitiveCapabilities<'cap> {
        self.capabilities
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

    fn push_one(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
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
