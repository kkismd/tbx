use crate::value::Value;
use crate::word::WordId;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CODE_SPACE_ID: AtomicUsize = AtomicUsize::new(1);

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

/// Opaque identity for one instruction-sequence owner.
///
/// A code-space ID is not an instruction operand and is not a serialized
/// address. It only preserves the owner side of a sequence-local address at
/// crate-internal validation boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CodeSpaceId {
    raw: usize,
}

impl CodeSpaceId {
    fn next() -> Self {
        let raw = NEXT_CODE_SPACE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("code-space ID allocation overflowed");

        Self { raw }
    }
}

/// Owner-qualified instruction position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CodeLocation {
    code_space: CodeSpaceId,
    address: InstructionAddress,
}

impl CodeLocation {
    pub(crate) const fn code_space(self) -> CodeSpaceId {
        self.code_space
    }

    pub(crate) const fn address(self) -> InstructionAddress {
        self.address
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
    InvalidAddress {
        address: InstructionAddress,
    },
    EndAddress {
        address: InstructionAddress,
    },
    AddressOverflow {
        address: InstructionAddress,
    },
    CodeSpaceMismatch {
        expected: CodeSpaceId,
        actual: CodeSpaceId,
        address: InstructionAddress,
    },
}

/// Owner for one instruction sequence.
///
/// The owner is deliberately separate from VM state. Builders may append here,
/// while the future VM receives only `InstructionView` so it cannot replace,
/// truncate, or append instructions through its fetch boundary.
#[derive(Debug)]
pub(crate) struct InstructionSequence {
    code_space: CodeSpaceId,
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
            code_space: self.code_space,
            instructions: &self.instructions,
        }
    }

    pub(crate) const fn code_space(&self) -> CodeSpaceId {
        self.code_space
    }

    pub(crate) fn len(&self) -> usize {
        self.instructions.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

impl Default for InstructionSequence {
    fn default() -> Self {
        Self {
            code_space: CodeSpaceId::next(),
            instructions: Vec::new(),
        }
    }
}

/// Read-only instruction fetch boundary for VM execution.
///
/// `code.len()` may be useful as a builder append position, but it is not a
/// running VM address. Only addresses that point at an existing instruction are
/// accepted by this view.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InstructionView<'a> {
    code_space: CodeSpaceId,
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

    pub(crate) fn get_location(
        self,
        location: CodeLocation,
    ) -> Result<&'a Instruction, InstructionAddressError> {
        let address = self.validate_location(location)?;
        self.get(address)
    }

    pub(crate) fn validate_address(
        self,
        address: InstructionAddress,
    ) -> Result<InstructionAddress, InstructionAddressError> {
        self.get(address).map(|_| address)
    }

    pub(crate) fn location(self, address: InstructionAddress) -> CodeLocation {
        CodeLocation {
            code_space: self.code_space,
            address,
        }
    }

    pub(crate) fn validate_location(
        self,
        location: CodeLocation,
    ) -> Result<InstructionAddress, InstructionAddressError> {
        if location.code_space != self.code_space {
            return Err(InstructionAddressError::CodeSpaceMismatch {
                expected: self.code_space,
                actual: location.code_space,
                address: location.address,
            });
        }

        self.validate_address(location.address)
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

    pub(crate) const fn code_space(self) -> CodeSpaceId {
        self.code_space
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
    fn empty_sequences_receive_distinct_code_space_ids() {
        let first = InstructionSequence::new();
        let second = InstructionSequence::new();

        assert_ne!(first.code_space(), second.code_space());
        assert_ne!(first.view().code_space(), second.view().code_space());
    }

    #[test]
    fn new_and_default_allocate_distinct_code_space_ids() {
        let from_new = InstructionSequence::new();
        let from_default = InstructionSequence::default();

        assert_ne!(from_new.code_space(), from_default.code_space());
    }

    #[test]
    fn views_from_same_sequence_share_code_space_id() {
        let mut code = InstructionSequence::new();
        code.append(Instruction::Halt);

        let first = code.view();
        let second = code.view();

        assert_eq!(first.code_space(), code.code_space());
        assert_eq!(first.code_space(), second.code_space());
    }

    #[test]
    fn cloned_view_preserves_code_space_id() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let view = code.view();
        let clone = view;

        assert_eq!(view.code_space(), clone.code_space());
        assert_eq!(clone.get(entry), Ok(&Instruction::Halt));
    }

    #[test]
    fn same_local_index_locations_differ_between_code_spaces() {
        let mut first = InstructionSequence::new();
        let mut second = InstructionSequence::new();
        let first_address = first.append(push(1));
        let second_address = second.append(push(1));

        assert_eq!(first_address.as_index(), second_address.as_index());
        assert_ne!(
            first.view().location(first_address),
            second.view().location(second_address)
        );
    }

    #[test]
    fn view_constructs_and_validates_same_owner_locations() {
        let mut code = InstructionSequence::new();
        let entry = code.append(push(42));
        let view = code.view();
        let location = view.location(entry);

        assert_eq!(location.code_space(), view.code_space());
        assert_eq!(location.address(), entry);
        assert_eq!(view.validate_location(location), Ok(entry));
        assert_eq!(view.get_location(location), Ok(&push(42)));
    }

    #[test]
    fn cross_owner_location_is_rejected_without_index_fallback() {
        let mut source = InstructionSequence::new();
        let source_address = source.append(push(1));
        let source_location = source.view().location(source_address);

        let mut target = InstructionSequence::new();
        let target_address = target.append(push(99));
        let target_view = target.view();

        assert_eq!(source_address.as_index(), target_address.as_index());
        assert_eq!(
            target_view.get_location(source_location),
            Err(InstructionAddressError::CodeSpaceMismatch {
                expected: target_view.code_space(),
                actual: source_location.code_space(),
                address: source_address,
            })
        );
    }

    #[test]
    fn same_owner_location_rejects_end_and_out_of_range_addresses() {
        let mut code = InstructionSequence::new();
        code.append(Instruction::Halt);
        let view = code.view();
        let end = view.location(address(1));
        let out_of_range = view.location(address(2));

        assert_eq!(
            view.validate_location(end),
            Err(InstructionAddressError::EndAddress {
                address: end.address()
            })
        );
        assert_eq!(
            view.validate_location(out_of_range),
            Err(InstructionAddressError::InvalidAddress {
                address: out_of_range.address()
            })
        );
    }

    #[test]
    fn empty_sequence_rejects_same_owner_location_at_append_position() {
        let code = InstructionSequence::new();
        let view = code.view();
        let location = view.location(address(0));

        assert_eq!(
            view.validate_location(location),
            Err(InstructionAddressError::EndAddress {
                address: location.address()
            })
        );
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
