/// Crate-internal identifier for one registered, complete source text.
///
/// ADR #1411 makes source identity local to source processing. The backing slot
/// is deliberately private so future storage layout changes cannot become a
/// public or serialized contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceId {
    slot: usize,
}

impl SourceId {
    #[cfg(test)]
    const fn test_invalid(slot: usize) -> Self {
        Self { slot }
    }
}

/// Validated byte span within one registered source text.
///
/// Offsets are UTF-8 byte offsets in the source identified by `source_id`, and
/// the range is always half-open `[start, end)`. Construction goes through a
/// `SourceView` so range and UTF-8 boundary contracts are checked against the
/// owning source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceSpan {
    source_id: SourceId,
    start: usize,
    end: usize,
}

impl SourceSpan {
    pub(crate) const fn source_id(self) -> SourceId {
        self.source_id
    }

    pub(crate) const fn start(self) -> usize {
        self.start
    }

    pub(crate) const fn end(self) -> usize {
        self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceError {
    InvalidSourceId {
        id: SourceId,
    },
    SourceMismatch {
        expected: SourceId,
        actual: SourceId,
    },
    ReversedRange {
        start: usize,
        end: usize,
    },
    OffsetOutOfBounds {
        offset: usize,
        len: usize,
    },
    InvalidUtf8Boundary {
        offset: usize,
    },
}

/// Owner for complete, read-only source texts.
///
/// This boundary owns only source-processing input text. It must not be merged
/// into runtime values, VM state, bindings, or executable word registries.
#[derive(Debug, Default)]
pub(crate) struct SourceTexts {
    sources: Vec<Box<str>>,
}

impl SourceTexts {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, text: impl Into<Box<str>>) -> SourceId {
        let id = SourceId {
            slot: self.sources.len(),
        };
        self.sources.push(text.into());
        id
    }

    pub(crate) fn view(&self) -> SourceView<'_> {
        SourceView {
            sources: &self.sources,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.sources.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

/// Read-only lookup and validation boundary over registered source texts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceView<'a> {
    sources: &'a [Box<str>],
}

impl<'a> SourceView<'a> {
    pub(crate) fn source(self, id: SourceId) -> Result<&'a str, SourceError> {
        self.sources
            .get(id.slot)
            .map(|source| source.as_ref())
            .ok_or(SourceError::InvalidSourceId { id })
    }

    pub(crate) fn span(
        self,
        source_id: SourceId,
        start: usize,
        end: usize,
    ) -> Result<SourceSpan, SourceError> {
        let source = self.source(source_id)?;
        validate_span_range(source, start, end)?;

        Ok(SourceSpan {
            source_id,
            start,
            end,
        })
    }

    pub(crate) fn slice(self, span: SourceSpan) -> Result<&'a str, SourceError> {
        self.slice_in_source(span.source_id(), span)
    }

    pub(crate) fn slice_in_source(
        self,
        source_id: SourceId,
        span: SourceSpan,
    ) -> Result<&'a str, SourceError> {
        if source_id != span.source_id() {
            return Err(SourceError::SourceMismatch {
                expected: source_id,
                actual: span.source_id(),
            });
        }

        let source = self.source(source_id)?;
        validate_span_range(source, span.start(), span.end())?;
        Ok(&source[span.start()..span.end()])
    }

    pub(crate) fn len(self) -> usize {
        self.sources.len()
    }

    pub(crate) fn is_empty(self) -> bool {
        self.sources.is_empty()
    }
}

fn validate_span_range(source: &str, start: usize, end: usize) -> Result<(), SourceError> {
    if start > end {
        return Err(SourceError::ReversedRange { start, end });
    }

    let len = source.len();
    validate_offset(source, start, len)?;
    validate_offset(source, end, len)?;
    Ok(())
}

fn validate_offset(source: &str, offset: usize, len: usize) -> Result<(), SourceError> {
    if offset > len {
        return Err(SourceError::OffsetOutOfBounds { offset, len });
    }

    if !source.is_char_boundary(offset) {
        return Err(SourceError::InvalidUtf8Boundary { offset });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    #[test]
    fn empty_owner_has_no_sources() {
        let sources = SourceTexts::new();
        let view = sources.view();

        assert!(sources.is_empty());
        assert!(view.is_empty());
        assert_eq!(sources.len(), 0);
        assert_eq!(view.len(), 0);
    }

    #[test]
    fn registers_empty_ascii_and_utf8_sources() {
        let mut sources = SourceTexts::new();

        let empty = sources.register("");
        let ascii = sources.register("PRINT 10");
        let utf8 = sources.register("PRINT \"あ\"");
        let view = sources.view();

        assert_eq!(view.source(empty), Ok(""));
        assert_eq!(view.source(ascii), Ok("PRINT 10"));
        assert_eq!(view.source(utf8), Ok("PRINT \"あ\""));
    }

    #[test]
    fn each_registration_receives_distinct_source_id() {
        let mut sources = SourceTexts::new();

        let first = sources.register("A");
        let second = sources.register("A");
        let third = sources.register("B");

        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);
        assert_eq!(sources.view().source(first), Ok("A"));
        assert_eq!(sources.view().source(second), Ok("A"));
        assert_eq!(sources.view().source(third), Ok("B"));
    }

    #[test]
    fn ascii_span_preserves_identity_and_slices_source() {
        let (sources, id) = register("ABC DEF");
        let view = sources.view();

        let actual = view.span(id, 4, 7).expect("ASCII span should validate");

        assert_eq!(actual.source_id(), id);
        assert_eq!(actual.start(), 4);
        assert_eq!(actual.end(), 7);
        assert_eq!(view.slice(actual), Ok("DEF"));
    }

    #[test]
    fn utf8_span_uses_byte_offsets_at_character_boundaries() {
        let (sources, id) = register("AあB");
        let view = sources.view();

        let actual = view.span(id, 1, 4).expect("UTF-8 span should validate");

        assert_eq!(view.slice(actual), Ok("あ"));
    }

    #[test]
    fn empty_and_eof_spans_are_valid() {
        let (sources, id) = register("ABC");
        let view = sources.view();

        let middle_empty = view.span(id, 1, 1).expect("empty span should validate");
        let eof = view.span(id, 3, 3).expect("EOF span should validate");

        assert_eq!(view.slice(middle_empty), Ok(""));
        assert_eq!(view.slice(eof), Ok(""));
    }

    #[test]
    fn empty_source_accepts_only_zero_length_eof_span() {
        let (sources, id) = register("");
        let view = sources.view();

        let eof = view
            .span(id, 0, 0)
            .expect("empty source EOF span should validate");

        assert_eq!(view.slice(eof), Ok(""));
        assert_eq!(
            view.span(id, 0, 1),
            Err(SourceError::OffsetOutOfBounds { offset: 1, len: 0 })
        );
    }

    #[test]
    fn reversed_range_is_rejected_before_boundary_checks() {
        let (sources, id) = register("ABC");

        assert_eq!(
            sources.view().span(id, 2, 1),
            Err(SourceError::ReversedRange { start: 2, end: 1 })
        );
    }

    #[test]
    fn offsets_beyond_source_length_are_rejected() {
        let (sources, id) = register("ABC");

        assert_eq!(
            sources.view().span(id, 0, 4),
            Err(SourceError::OffsetOutOfBounds { offset: 4, len: 3 })
        );
        assert_eq!(
            sources.view().span(id, 4, 4),
            Err(SourceError::OffsetOutOfBounds { offset: 4, len: 3 })
        );
    }

    #[test]
    fn offsets_inside_utf8_characters_are_rejected() {
        let (sources, id) = register("AあB");

        assert_eq!(
            sources.view().span(id, 1, 2),
            Err(SourceError::InvalidUtf8Boundary { offset: 2 })
        );
        assert_eq!(
            sources.view().span(id, 2, 4),
            Err(SourceError::InvalidUtf8Boundary { offset: 2 })
        );
    }

    #[test]
    fn invalid_source_id_does_not_fallback_to_another_source() {
        let (sources, valid) = register("ABC");
        let invalid = SourceId::test_invalid(valid.slot + 1);

        assert_eq!(
            sources.view().source(invalid),
            Err(SourceError::InvalidSourceId { id: invalid })
        );
        assert_eq!(
            sources.view().span(invalid, 0, 0),
            Err(SourceError::InvalidSourceId { id: invalid })
        );
    }

    #[test]
    fn same_offsets_in_different_sources_are_different_spans() {
        let mut sources = SourceTexts::new();
        let first = sources.register("ABC");
        let second = sources.register("XYZ");
        let view = sources.view();

        let first_span = span(view, first, 0, 1);
        let second_span = span(view, second, 0, 1);

        assert_ne!(first_span, second_span);
        assert_eq!(view.slice(first_span), Ok("A"));
        assert_eq!(view.slice(second_span), Ok("X"));
    }

    #[test]
    fn slice_in_source_rejects_mismatched_source_id() {
        let mut sources = SourceTexts::new();
        let first = sources.register("ABC");
        let second = sources.register("XYZ");
        let view = sources.view();
        let first_span = span(view, first, 0, 1);

        assert_eq!(
            view.slice_in_source(second, first_span),
            Err(SourceError::SourceMismatch {
                expected: second,
                actual: first
            })
        );
        assert_eq!(view.slice_in_source(first, first_span), Ok("A"));
    }
}
