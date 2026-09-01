use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_SOURCE_OWNER_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SourceOwnerId {
    id: usize,
}

impl SourceOwnerId {
    fn next() -> Self {
        let id = NEXT_SOURCE_OWNER_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("source owner id space exhausted");

        Self { id }
    }
}

/// Crate-internal identifier for one registered, complete source text.
///
/// ADR #1411 makes source identity local to source processing. The backing
/// owner and slot are deliberately private so future storage layout changes
/// cannot become a public or serialized contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceId {
    owner: SourceOwnerId,
    slot: usize,
}

impl SourceId {
    #[cfg(test)]
    const fn test_invalid(owner: SourceOwnerId, slot: usize) -> Self {
        Self { owner, slot }
    }

    #[cfg(test)]
    pub(crate) const fn test_next_slot(self) -> Self {
        Self {
            owner: self.owner,
            slot: self.slot + 1,
        }
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
#[derive(Debug)]
pub(crate) struct SourceTexts {
    owner: SourceOwnerId,
    sources: Vec<SourceRecord>,
}

/// Complete source payload registered under one `SourceId`.
///
/// ADR #1529 requires source text and user-facing display information to share
/// the same registration and lifetime so later diagnostics cannot lose their
/// association while source mappings may still refer to this `SourceId`.
#[derive(Debug)]
struct SourceRecord {
    text: Box<str>,
    display_name: Box<str>,
}

impl SourceTexts {
    pub(crate) fn new() -> Self {
        Self {
            owner: SourceOwnerId::next(),
            sources: Vec::new(),
        }
    }

    pub(crate) fn register(
        &mut self,
        text: impl Into<Box<str>>,
        display_name: impl Into<Box<str>>,
    ) -> SourceId {
        let id = SourceId {
            owner: self.owner,
            slot: self.sources.len(),
        };
        self.sources.push(SourceRecord {
            text: text.into(),
            display_name: display_name.into(),
        });
        id
    }

    pub(crate) fn view(&self) -> SourceView<'_> {
        SourceView {
            owner: self.owner,
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
    owner: SourceOwnerId,
    sources: &'a [SourceRecord],
}

impl<'a> SourceView<'a> {
    pub(crate) fn source(self, id: SourceId) -> Result<&'a str, SourceError> {
        self.record(id).map(|source| source.text.as_ref())
    }

    pub(crate) fn display_name(self, id: SourceId) -> Result<&'a str, SourceError> {
        self.record(id).map(|source| source.display_name.as_ref())
    }

    fn record(self, id: SourceId) -> Result<&'a SourceRecord, SourceError> {
        if id.owner != self.owner {
            return Err(SourceError::InvalidSourceId { id });
        }

        self.sources
            .get(id.slot)
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
        let id = sources.register(text, "test.tbx");
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

        let empty = sources.register("", "empty.tbx");
        let ascii = sources.register("PRINT 10", "ascii.tbx");
        let utf8 = sources.register("PRINT \"あ\"", "utf8.tbx");
        let view = sources.view();

        assert_eq!(view.source(empty), Ok(""));
        assert_eq!(view.source(ascii), Ok("PRINT 10"));
        assert_eq!(view.source(utf8), Ok("PRINT \"あ\""));
        assert_eq!(view.display_name(empty), Ok("empty.tbx"));
        assert_eq!(view.display_name(ascii), Ok("ascii.tbx"));
        assert_eq!(view.display_name(utf8), Ok("utf8.tbx"));
    }

    #[test]
    fn each_registration_receives_distinct_source_id() {
        let mut sources = SourceTexts::new();

        let first = sources.register("A", "same-name.tbx");
        let second = sources.register("A", "same-name.tbx");
        let third = sources.register("B", "different-name.tbx");

        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);
        assert_eq!(sources.view().source(first), Ok("A"));
        assert_eq!(sources.view().source(second), Ok("A"));
        assert_eq!(sources.view().source(third), Ok("B"));
        assert_eq!(sources.view().display_name(first), Ok("same-name.tbx"));
        assert_eq!(sources.view().display_name(second), Ok("same-name.tbx"));
        assert_eq!(sources.view().display_name(third), Ok("different-name.tbx"));
    }

    #[test]
    fn each_registration_keeps_text_and_display_name_together() {
        let mut sources = SourceTexts::new();

        let empty = sources.register("", "<stdin>");
        let ascii = sources.register("PRINT 10", "program.tbx");
        let utf8 = sources.register("PRINT \"あ\"", "unicode.tbx");
        let same_display_name = sources.register("PRINT 20", "program.tbx");
        let same_text = sources.register("PRINT 10", "copy.tbx");
        let view = sources.view();

        assert_eq!(view.source(empty), Ok(""));
        assert_eq!(view.display_name(empty), Ok("<stdin>"));
        assert_eq!(view.source(ascii), Ok("PRINT 10"));
        assert_eq!(view.display_name(ascii), Ok("program.tbx"));
        assert_eq!(view.source(utf8), Ok("PRINT \"あ\""));
        assert_eq!(view.display_name(utf8), Ok("unicode.tbx"));
        assert_eq!(view.source(same_display_name), Ok("PRINT 20"));
        assert_eq!(view.display_name(same_display_name), Ok("program.tbx"));
        assert_eq!(view.source(same_text), Ok("PRINT 10"));
        assert_eq!(view.display_name(same_text), Ok("copy.tbx"));

        assert_ne!(ascii, same_display_name);
        assert_ne!(ascii, same_text);
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
        let invalid = SourceId::test_invalid(valid.owner, valid.slot + 1);

        assert_eq!(
            sources.view().source(invalid),
            Err(SourceError::InvalidSourceId { id: invalid })
        );
        assert_eq!(
            sources.view().display_name(invalid),
            Err(SourceError::InvalidSourceId { id: invalid })
        );
        assert_eq!(
            sources.view().span(invalid, 0, 0),
            Err(SourceError::InvalidSourceId { id: invalid })
        );
    }

    #[test]
    fn source_ids_from_different_owners_do_not_collide_at_the_same_slot() {
        let mut first_owner = SourceTexts::new();
        let first_id = first_owner.register("ABC", "first.tbx");
        let first_span = span(first_owner.view(), first_id, 0, 1);

        let mut second_owner = SourceTexts::new();
        let second_id = second_owner.register("XYZ", "second.tbx");

        assert_eq!(first_id.slot, second_id.slot);
        assert_ne!(first_id, second_id);
        assert_eq!(first_owner.view().slice(first_span), Ok("A"));
        assert_eq!(
            second_owner.view().source(first_id),
            Err(SourceError::InvalidSourceId { id: first_id })
        );
        assert_eq!(
            second_owner.view().display_name(first_id),
            Err(SourceError::InvalidSourceId { id: first_id })
        );
        assert_eq!(
            second_owner.view().slice(first_span),
            Err(SourceError::InvalidSourceId { id: first_id })
        );
    }

    #[test]
    fn same_offsets_in_different_sources_are_different_spans() {
        let mut sources = SourceTexts::new();
        let first = sources.register("ABC", "first.tbx");
        let second = sources.register("XYZ", "second.tbx");
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
        let first = sources.register("ABC", "first.tbx");
        let second = sources.register("XYZ", "second.tbx");
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
