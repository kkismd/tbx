use crate::instruction::{CodeLocation, CodeSpaceId, InstructionAddress, InstructionAddressError};
use crate::source::SourceSpan;

/// Owner for source spans keyed by one instruction sequence's code space.
///
/// `spans[index]` describes the instruction at local `InstructionAddress`
/// `index` for this owner. `None` is a valid existing instruction with no
/// source mapping; missing indexes are invalid runtime locations.
#[derive(Debug)]
pub(crate) struct InstructionSourceMapping {
    code_space: CodeSpaceId,
    spans: Vec<Option<SourceSpan>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InstructionSourceMappingView<'a> {
    code_space: CodeSpaceId,
    spans: &'a [Option<SourceSpan>],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceMappingLookup<'a> {
    views: &'a [InstructionSourceMappingView<'a>],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceMappingLookupError {
    UnknownCodeSpace { code_space: CodeSpaceId },
    DuplicateCodeSpace { code_space: CodeSpaceId },
    Address { source: InstructionAddressError },
}

impl InstructionSourceMapping {
    pub(crate) fn new(code_space: CodeSpaceId) -> Self {
        Self {
            code_space,
            spans: Vec::new(),
        }
    }

    pub(crate) fn append_mapped(
        &mut self,
        address: InstructionAddress,
        span: SourceSpan,
    ) -> Result<(), SourceMappingAppendError> {
        self.append(address, Some(span))
    }

    #[cfg(test)]
    pub(crate) fn append_unmapped(
        &mut self,
        address: InstructionAddress,
    ) -> Result<(), SourceMappingAppendError> {
        self.append(address, None)
    }

    pub(crate) fn view(&self) -> InstructionSourceMappingView<'_> {
        InstructionSourceMappingView {
            code_space: self.code_space,
            spans: &self.spans,
        }
    }

    pub(crate) fn code_space(&self) -> CodeSpaceId {
        self.code_space
    }

    fn append(
        &mut self,
        address: InstructionAddress,
        span: Option<SourceSpan>,
    ) -> Result<(), SourceMappingAppendError> {
        let expected = InstructionAddress::from_index(self.spans.len());
        if address != expected {
            return Err(SourceMappingAppendError::OutOfOrder {
                expected,
                actual: address,
            });
        }

        self.spans.push(span);
        Ok(())
    }
}

impl<'a> InstructionSourceMappingView<'a> {
    pub(crate) fn source_span(
        self,
        location: CodeLocation,
    ) -> Result<Option<SourceSpan>, SourceMappingLookupError> {
        self.validate_location(location)?;
        Ok(self.spans[location.address().as_index()])
    }

    pub(crate) fn validate_location(
        self,
        location: CodeLocation,
    ) -> Result<InstructionAddress, SourceMappingLookupError> {
        if location.code_space() != self.code_space {
            return Err(SourceMappingLookupError::Address {
                source: InstructionAddressError::CodeSpaceMismatch {
                    expected: self.code_space,
                    actual: location.code_space(),
                    address: location.address(),
                },
            });
        }

        let address = location.address();
        if address.as_index() < self.spans.len() {
            Ok(address)
        } else {
            Err(SourceMappingLookupError::Address {
                source: self.address_error(address),
            })
        }
    }

    pub(crate) const fn code_space(self) -> CodeSpaceId {
        self.code_space
    }

    pub(crate) fn len(self) -> usize {
        self.spans.len()
    }

    fn address_error(self, address: InstructionAddress) -> InstructionAddressError {
        if address.as_index() == self.spans.len() {
            InstructionAddressError::EndAddress { address }
        } else {
            InstructionAddressError::InvalidAddress { address }
        }
    }
}

impl<'a> SourceMappingLookup<'a> {
    pub(crate) fn new(
        views: &'a [InstructionSourceMappingView<'a>],
    ) -> Result<Self, SourceMappingLookupError> {
        for (index, view) in views.iter().enumerate() {
            if views[index + 1..]
                .iter()
                .any(|other| other.code_space() == view.code_space())
            {
                return Err(SourceMappingLookupError::DuplicateCodeSpace {
                    code_space: view.code_space(),
                });
            }
        }

        Ok(Self { views })
    }

    pub(crate) fn source_span(
        self,
        location: CodeLocation,
    ) -> Result<Option<SourceSpan>, SourceMappingLookupError> {
        self.view_for(location.code_space())?.source_span(location)
    }

    fn view_for(
        self,
        code_space: CodeSpaceId,
    ) -> Result<InstructionSourceMappingView<'a>, SourceMappingLookupError> {
        self.views
            .iter()
            .copied()
            .find(|view| view.code_space() == code_space)
            .ok_or(SourceMappingLookupError::UnknownCodeSpace { code_space })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceMappingAppendError {
    OutOfOrder {
        expected: InstructionAddress,
        actual: InstructionAddress,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::source::{SourceId, SourceTexts, SourceView};

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    #[test]
    fn mapping_owner_keeps_code_space_identity() {
        let code = InstructionSequence::new();
        let mapping = InstructionSourceMapping::new(code.code_space());

        assert_eq!(mapping.code_space(), code.code_space());
        assert_eq!(mapping.view().code_space(), code.code_space());
    }

    #[test]
    fn mapped_location_returns_source_span() {
        let (sources, source_id) = source("10");
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let location = code.view().location(entry);
        let mut mapping = InstructionSourceMapping::new(code.code_space());
        let expected = span(sources.view(), source_id, 0, 2);
        mapping
            .append_mapped(entry, expected)
            .expect("mapping should append");

        assert_eq!(mapping.view().source_span(location), Ok(Some(expected)));
    }

    #[test]
    fn same_local_index_is_separate_per_code_space() {
        let (mut sources, first_source) = source("A");
        let second_source = sources.register("B");
        let mut first_code = InstructionSequence::new();
        let mut second_code = InstructionSequence::new();
        let first_address = first_code.append(Instruction::Halt);
        let second_address = second_code.append(Instruction::Halt);
        let first_span = span(sources.view(), first_source, 0, 1);
        let second_span = span(sources.view(), second_source, 0, 1);
        let mut first_mapping = InstructionSourceMapping::new(first_code.code_space());
        let mut second_mapping = InstructionSourceMapping::new(second_code.code_space());
        first_mapping
            .append_mapped(first_address, first_span)
            .expect("first mapping should append");
        second_mapping
            .append_mapped(second_address, second_span)
            .expect("second mapping should append");
        let views = [first_mapping.view(), second_mapping.view()];
        let lookup = SourceMappingLookup::new(&views).expect("mapping spaces are distinct");

        assert_eq!(first_address.as_index(), second_address.as_index());
        assert_eq!(
            lookup.source_span(first_code.view().location(first_address)),
            Ok(Some(first_span))
        );
        assert_eq!(
            lookup.source_span(second_code.view().location(second_address)),
            Ok(Some(second_span))
        );
    }

    #[test]
    fn lookup_does_not_fallback_to_same_local_index_in_another_space() {
        let (sources, source_id) = source("A");
        let mut registered_code = InstructionSequence::new();
        let registered_address = registered_code.append(Instruction::Halt);
        let mut other_code = InstructionSequence::new();
        let other_address = other_code.append(Instruction::Halt);
        let mut mapping = InstructionSourceMapping::new(registered_code.code_space());
        mapping
            .append_mapped(registered_address, span(sources.view(), source_id, 0, 1))
            .expect("mapping should append");
        let views = [mapping.view()];
        let lookup = SourceMappingLookup::new(&views).expect("single mapping is valid");

        assert_eq!(registered_address.as_index(), other_address.as_index());
        assert_eq!(
            lookup.source_span(other_code.view().location(other_address)),
            Err(SourceMappingLookupError::UnknownCodeSpace {
                code_space: other_code.code_space()
            })
        );
    }

    #[test]
    fn valid_unmapped_location_is_distinct_from_unknown_and_invalid() {
        let mut mapped_code = InstructionSequence::new();
        let mapped = mapped_code.append(Instruction::Halt);
        let unmapped = mapped_code.append(Instruction::Halt);
        let mut unknown_code = InstructionSequence::new();
        let unknown = unknown_code.append(Instruction::Halt);
        let (sources, source_id) = source("A");
        let mut mapping = InstructionSourceMapping::new(mapped_code.code_space());
        mapping
            .append_mapped(mapped, span(sources.view(), source_id, 0, 1))
            .expect("mapped source should append");
        mapping
            .append_unmapped(unmapped)
            .expect("unmapped source should append");
        let views = [mapping.view()];
        let lookup = SourceMappingLookup::new(&views).expect("single mapping is valid");
        let end = mapped_code.view().location(address(2));
        let out_of_range = mapped_code.view().location(address(3));

        assert_eq!(
            lookup.source_span(mapped_code.view().location(unmapped)),
            Ok(None)
        );
        assert_eq!(
            lookup.source_span(unknown_code.view().location(unknown)),
            Err(SourceMappingLookupError::UnknownCodeSpace {
                code_space: unknown_code.code_space()
            })
        );
        assert_eq!(
            lookup.source_span(end),
            Err(SourceMappingLookupError::Address {
                source: InstructionAddressError::EndAddress {
                    address: end.address()
                }
            })
        );
        assert_eq!(
            lookup.source_span(out_of_range),
            Err(SourceMappingLookupError::Address {
                source: InstructionAddressError::InvalidAddress {
                    address: out_of_range.address()
                }
            })
        );
    }

    #[test]
    fn append_requires_instruction_order() {
        let mut code = InstructionSequence::new();
        code.append(Instruction::Halt);
        let (sources, source_id) = source("A");
        let mut mapping = InstructionSourceMapping::new(code.code_space());

        assert_eq!(
            mapping.append_mapped(address(1), span(sources.view(), source_id, 0, 1)),
            Err(SourceMappingAppendError::OutOfOrder {
                expected: address(0),
                actual: address(1),
            })
        );
    }

    #[test]
    fn published_code_mapping_uses_code_owner_not_source_owner() {
        let (mut sources, first_source) = source("A");
        let second_source = sources.register("B");
        let mut published_code = InstructionSequence::new();
        let entry = published_code.append(Instruction::Halt);
        let first_span = span(sources.view(), first_source, 0, 1);
        let second_same_offset = span(sources.view(), second_source, 0, 1);
        let mut mapping = InstructionSourceMapping::new(published_code.code_space());
        mapping
            .append_mapped(entry, first_span)
            .expect("published mapping should append");

        assert_ne!(first_span, second_same_offset);
        assert_eq!(
            mapping
                .view()
                .source_span(published_code.view().location(entry)),
            Ok(Some(first_span))
        );
        assert_ne!(
            mapping
                .view()
                .source_span(published_code.view().location(entry)),
            Ok(Some(second_same_offset))
        );
    }

    #[test]
    fn duplicate_mapping_spaces_are_rejected() {
        let code = InstructionSequence::new();
        let first = InstructionSourceMapping::new(code.code_space());
        let second = InstructionSourceMapping::new(code.code_space());
        let views = [first.view(), second.view()];

        assert_eq!(
            SourceMappingLookup::new(&views).expect_err("duplicate mapping spaces should fail"),
            SourceMappingLookupError::DuplicateCodeSpace {
                code_space: code.code_space()
            }
        );
    }
}
