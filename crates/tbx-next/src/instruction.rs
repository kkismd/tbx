use crate::value::Value;
use crate::word::WordId;

/// Absolute address into the shared TBX Next instruction sequence.
///
/// ADR #1367 defines instruction addresses as VM-control identifiers, not
/// runtime values. The concrete backing type is intentionally private and must
/// not become a serialized or public ABI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InstructionAddress {
    index: usize,
}

impl InstructionAddress {
    pub(crate) const fn from_index(index: usize) -> Self {
        Self { index }
    }

    pub(crate) const fn as_index(self) -> usize {
        self.index
    }

    pub(crate) fn checked_next(self) -> Result<Self, InstructionAddressError> {
        self.index
            .checked_add(1)
            .map(Self::from_index)
            .ok_or(InstructionAddressError::AddressOverflow { address: self })
    }
}

/// Crate-internal typed instruction for the future VM execution core.
///
/// This is intentionally a small, orthogonal instruction set. `Call` carries
/// an already-resolved word identifier so runtime VM execution never performs
/// name lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Instruction {
    Push(Value),
    Call(WordId),
    Jump(InstructionAddress),
    JumpIfZero(InstructionAddress),
    Return,
    Halt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstructionAddressError {
    InvalidAddress { address: InstructionAddress },
    EndAddress { address: InstructionAddress },
    AddressOverflow { address: InstructionAddress },
}

/// Owner for the single shared instruction sequence.
///
/// The owner is deliberately separate from VM state. Builders may append here,
/// while the future VM receives only `InstructionView` so it cannot replace,
/// truncate, or append instructions through its fetch boundary.
#[derive(Debug, Default)]
pub(crate) struct InstructionSequence {
    instructions: Vec<Instruction>,
}

impl InstructionSequence {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn append(&mut self, instruction: Instruction) -> InstructionAddress {
        let address = InstructionAddress::from_index(self.instructions.len());
        self.instructions.push(instruction);
        address
    }

    pub(crate) fn view(&self) -> InstructionView<'_> {
        InstructionView {
            instructions: &self.instructions,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.instructions.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

/// Read-only instruction fetch boundary for VM execution.
///
/// `code.len()` may be useful as a builder append position, but it is not a
/// running VM address. Only addresses that point at an existing instruction are
/// accepted by this view.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InstructionView<'a> {
    instructions: &'a [Instruction],
}

impl<'a> InstructionView<'a> {
    pub(crate) fn get(
        self,
        address: InstructionAddress,
    ) -> Result<&'a Instruction, InstructionAddressError> {
        self.instructions
            .get(address.as_index())
            .ok_or_else(|| self.address_error(address))
    }

    pub(crate) fn validate_address(
        self,
        address: InstructionAddress,
    ) -> Result<InstructionAddress, InstructionAddressError> {
        self.get(address).map(|_| address)
    }

    pub(crate) fn checked_next_address(
        self,
        address: InstructionAddress,
    ) -> Result<InstructionAddress, InstructionAddressError> {
        self.validate_address(address)?;
        let next = address.checked_next()?;
        self.validate_address(next)
    }

    pub(crate) fn len(self) -> usize {
        self.instructions.len()
    }

    pub(crate) fn is_empty(self) -> bool {
        self.instructions.is_empty()
    }

    fn address_error(self, address: InstructionAddress) -> InstructionAddressError {
        if address.as_index() == self.instructions.len() {
            InstructionAddressError::EndAddress { address }
        } else {
            InstructionAddressError::InvalidAddress { address }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(value: i16) -> Instruction {
        Instruction::Push(Value::integer(value))
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    #[test]
    fn empty_sequence_rejects_lookup_at_append_position() {
        let code = InstructionSequence::new();
        let view = code.view();
        let zero = address(0);

        assert!(code.is_empty());
        assert!(view.is_empty());
        assert_eq!(code.len(), 0);
        assert_eq!(view.len(), 0);
        assert_eq!(
            view.get(zero),
            Err(InstructionAddressError::EndAddress { address: zero })
        );
    }

    #[test]
    fn append_returns_addresses_for_existing_instructions() {
        let mut code = InstructionSequence::new();

        let first = code.append(push(10));
        let second = code.append(push(20));
        let third = code.append(Instruction::Return);
        let fourth = code.append(Instruction::Halt);
        let view = code.view();

        assert_eq!(first.as_index(), 0);
        assert_eq!(second.as_index(), 1);
        assert_eq!(third.as_index(), 2);
        assert_eq!(fourth.as_index(), 3);
        assert_eq!(view.get(first), Ok(&push(10)));
        assert_eq!(view.get(second), Ok(&push(20)));
        assert_eq!(view.get(third), Ok(&Instruction::Return));
        assert_eq!(view.get(fourth), Ok(&Instruction::Halt));
    }

    #[test]
    fn lookup_rejects_end_and_out_of_range_addresses() {
        let mut code = InstructionSequence::new();
        let first = code.append(push(1));
        let end = address(1);
        let out_of_range = address(2);
        let max = address(usize::MAX);
        let view = code.view();

        assert_eq!(view.get(first), Ok(&push(1)));
        assert_eq!(
            view.get(end),
            Err(InstructionAddressError::EndAddress { address: end })
        );
        assert_eq!(
            view.get(out_of_range),
            Err(InstructionAddressError::InvalidAddress {
                address: out_of_range
            })
        );
        assert_eq!(
            view.get(max),
            Err(InstructionAddressError::InvalidAddress { address: max })
        );
    }

    #[test]
    fn checked_next_address_accepts_only_existing_next_instruction() {
        let mut code = InstructionSequence::new();
        let first = code.append(push(1));
        let second = code.append(push(2));
        let third = code.append(Instruction::Halt);
        let view = code.view();

        assert_eq!(view.checked_next_address(first), Ok(second));
        assert_eq!(view.checked_next_address(second), Ok(third));
        assert_eq!(
            view.checked_next_address(third),
            Err(InstructionAddressError::EndAddress {
                address: address(3)
            })
        );
    }

    #[test]
    fn checked_next_address_rejects_invalid_current_address() {
        let mut code = InstructionSequence::new();
        code.append(Instruction::Halt);
        let invalid = address(usize::MAX);

        assert_eq!(
            code.view().checked_next_address(invalid),
            Err(InstructionAddressError::InvalidAddress { address: invalid })
        );
    }

    #[test]
    fn checked_next_detects_integer_overflow() {
        let max = address(usize::MAX);

        assert_eq!(
            max.checked_next(),
            Err(InstructionAddressError::AddressOverflow { address: max })
        );
    }

    #[test]
    fn jump_instructions_preserve_targets() {
        let mut code = InstructionSequence::new();
        let target = code.append(Instruction::Halt);
        let jump = code.append(Instruction::Jump(target));
        let jump_if_zero = code.append(Instruction::JumpIfZero(target));
        let view = code.view();

        assert_eq!(view.validate_address(target), Ok(target));
        assert_eq!(view.get(jump), Ok(&Instruction::Jump(target)));
        assert_eq!(view.get(jump_if_zero), Ok(&Instruction::JumpIfZero(target)));
    }
}
