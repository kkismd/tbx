use crate::instruction::InstructionAddress;
use crate::value::Value;

// ADR #1366 keeps the two stacks separate even in the initial host VM:
// data-stack cells are language values, while return-stack frames are VM
// control state. Both stacks intentionally use growable containers here; this
// phase does not add a fixed maximum depth or expose capacity as a contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackError {
    DataStackUnderflow,
    ReturnStackUnderflow,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DataStack {
    values: Vec<Value>,
}

impl DataStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, value: Value) {
        self.values.push(value);
    }

    pub(crate) fn pop(&mut self) -> Result<Value, StackError> {
        self.values.pop().ok_or(StackError::DataStackUnderflow)
    }

    pub(crate) fn peek(&self) -> Result<Value, StackError> {
        self.values
            .last()
            .copied()
            .ok_or(StackError::DataStackUnderflow)
    }

    pub(crate) fn depth(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn as_slice(&self) -> &[Value] {
        &self.values
    }

    pub(crate) fn require_depth(&self, required: usize) -> Result<(), StackError> {
        if self.values.len() < required {
            return Err(StackError::DataStackUnderflow);
        }

        Ok(())
    }

    pub(crate) fn pop2(&mut self) -> Result<(Value, Value), StackError> {
        // ADR #1366 keeps multi-operand operations atomic: check the required
        // depth before mutating so underflow cannot partially consume operands.
        self.require_depth(2)?;

        let rhs = self
            .values
            .pop()
            .expect("depth was checked before popping rhs");
        let lhs = self
            .values
            .pop()
            .expect("depth was checked before popping lhs");

        Ok((lhs, rhs))
    }

    pub(crate) fn restore(&mut self, checkpoint: Self) {
        *self = checkpoint;
    }
}

/// Opaque VM-control frame for the return stack.
///
/// ADR #1366 separates language values from VM control state: the data stack
/// stores only `Value`, while return addresses stay unobservable through user
/// data-stack operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReturnFrame {
    return_address: InstructionAddress,
}

impl ReturnFrame {
    pub(crate) const fn new(return_address: InstructionAddress) -> Self {
        Self { return_address }
    }

    pub(crate) const fn return_address(self) -> InstructionAddress {
        self.return_address
    }
}

#[derive(Debug, Default)]
pub(crate) struct ReturnStack {
    frames: Vec<ReturnFrame>,
}

impl ReturnStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, frame: ReturnFrame) {
        self.frames.push(frame);
    }

    pub(crate) fn pop(&mut self) -> Result<ReturnFrame, StackError> {
        self.frames.pop().ok_or(StackError::ReturnStackUnderflow)
    }

    pub(crate) fn peek(&self) -> Result<ReturnFrame, StackError> {
        self.frames
            .last()
            .copied()
            .ok_or(StackError::ReturnStackUnderflow)
    }

    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn as_slice(&self) -> &[ReturnFrame] {
        &self.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Value = Value::integer(i16::MIN);
    const MAX: Value = Value::integer(i16::MAX);

    #[test]
    fn data_stack_starts_empty() {
        let stack = DataStack::new();

        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn data_stack_push_peek_and_pop_single_value() {
        let mut stack = DataStack::new();
        let value = Value::integer(42);

        stack.push(value);

        assert_eq!(stack.depth(), 1);
        assert!(!stack.is_empty());
        assert_eq!(stack.peek(), Ok(value));
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.pop(), Ok(value));
        assert!(stack.is_empty());
    }

    #[test]
    fn data_stack_pops_values_in_lifo_order() {
        let mut stack = DataStack::new();

        stack.push(Value::integer(1));
        stack.push(Value::integer(2));
        stack.push(Value::integer(3));

        assert_eq!(stack.pop(), Ok(Value::integer(3)));
        assert_eq!(stack.pop(), Ok(Value::integer(2)));
        assert_eq!(stack.pop(), Ok(Value::integer(1)));
        assert!(stack.is_empty());
    }

    #[test]
    fn data_stack_preserves_i16_edges() {
        let mut stack = DataStack::new();

        stack.push(MIN);
        stack.push(MAX);

        assert_eq!(stack.pop(), Ok(MAX));
        assert_eq!(stack.pop(), Ok(MIN));
    }

    #[test]
    fn data_stack_empty_pop_and_peek_report_underflow_without_mutation() {
        let mut stack = DataStack::new();

        assert_eq!(stack.pop(), Err(StackError::DataStackUnderflow));
        assert_eq!(stack.peek(), Err(StackError::DataStackUnderflow));
        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn data_stack_require_depth_reports_underflow_without_mutation() {
        let mut stack = DataStack::new();
        let remaining = Value::integer(10);

        assert_eq!(stack.require_depth(2), Err(StackError::DataStackUnderflow));
        assert_eq!(stack.depth(), 0);

        stack.push(remaining);

        assert_eq!(stack.require_depth(2), Err(StackError::DataStackUnderflow));
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.peek(), Ok(remaining));
    }

    #[test]
    fn data_stack_pop2_returns_lhs_then_rhs_for_binary_primitives() {
        let mut stack = DataStack::new();
        let lhs = Value::integer(10);
        let rhs = Value::integer(3);

        stack.push(lhs);
        stack.push(rhs);

        assert_eq!(stack.pop2(), Ok((lhs, rhs)));
        assert!(stack.is_empty());
    }

    #[test]
    fn data_stack_pop2_is_atomic_on_underflow() {
        let mut stack = DataStack::new();
        let existing = Value::integer(7);

        assert_eq!(stack.pop2(), Err(StackError::DataStackUnderflow));
        assert_eq!(stack.depth(), 0);

        stack.push(existing);

        assert_eq!(stack.pop2(), Err(StackError::DataStackUnderflow));
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.pop(), Ok(existing));
        assert!(stack.is_empty());
    }

    #[test]
    fn return_stack_starts_empty() {
        let stack = ReturnStack::new();

        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn return_stack_pushes_and_pops_frames_in_lifo_order() {
        let mut stack = ReturnStack::new();
        let first = ReturnFrame::new(InstructionAddress::from_index(1));
        let second = ReturnFrame::new(InstructionAddress::from_index(2));

        stack.push(first);
        stack.push(second);

        assert_eq!(stack.depth(), 2);
        assert_eq!(stack.pop(), Ok(second));
        assert_eq!(stack.pop(), Ok(first));
        assert!(stack.is_empty());
    }

    #[test]
    fn return_frame_exposes_return_address() {
        let address = InstructionAddress::from_index(4);
        let frame = ReturnFrame::new(address);

        assert_eq!(frame.return_address(), address);
    }

    #[test]
    fn return_stack_underflow_does_not_mutate_state() {
        let mut stack = ReturnStack::new();

        assert_eq!(stack.pop(), Err(StackError::ReturnStackUnderflow));
        assert_eq!(stack.peek(), Err(StackError::ReturnStackUnderflow));
        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn return_stack_peek_does_not_pop_frame() {
        let mut stack = ReturnStack::new();
        let frame = ReturnFrame::new(InstructionAddress::from_index(3));

        stack.push(frame);

        assert_eq!(stack.peek(), Ok(frame));
        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.pop(), Ok(frame));
        assert!(stack.is_empty());
    }

    #[test]
    fn data_stack_and_return_stack_are_independent() {
        let mut data_stack = DataStack::new();
        let mut return_stack = ReturnStack::new();
        let frame = ReturnFrame::new(InstructionAddress::from_index(1));

        data_stack.push(Value::integer(42));
        return_stack.push(frame);

        assert_eq!(data_stack.pop(), Ok(Value::integer(42)));
        assert_eq!(return_stack.depth(), 1);
        assert_eq!(return_stack.pop(), Ok(frame));
        assert!(data_stack.is_empty());
        assert!(return_stack.is_empty());
    }
}
