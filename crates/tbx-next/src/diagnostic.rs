use std::fmt;

use crate::source::{SourceError, SourceSpan, SourceView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UserDiagnostic {
    message: Box<str>,
    primary_span: Option<SourceSpan>,
    target: Option<Box<str>>,
    notes: Vec<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedDiagnostic {
    message: Box<str>,
    primary: Option<RenderedPrimarySpan>,
    target: Option<Box<str>>,
    notes: Vec<Box<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedPrimarySpan {
    display_name: Box<str>,
    line_number: usize,
    column_number: usize,
    source_line: Box<str>,
    highlight_start_column: usize,
    highlight_columns: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagnosticRenderError {
    Source { source: SourceError },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DiagnosticRenderer<'a> {
    sources: SourceView<'a>,
}

#[derive(Debug, Clone, Copy)]
struct LineOrigin<'a> {
    line_number: usize,
    line_start: usize,
    line_end: usize,
    text: &'a str,
}

impl UserDiagnostic {
    pub(crate) fn at_span(span: SourceSpan, message: impl Into<Box<str>>) -> Self {
        Self {
            message: message.into(),
            primary_span: Some(span),
            target: None,
            notes: Vec::new(),
        }
    }

    pub(crate) fn without_source(
        target: impl Into<Box<str>>,
        message: impl Into<Box<str>>,
    ) -> Self {
        Self {
            message: message.into(),
            primary_span: None,
            target: Some(target.into()),
            notes: Vec::new(),
        }
    }

    pub(crate) fn with_note(mut self, note: impl Into<Box<str>>) -> Self {
        self.notes.push(note.into());
        self
    }
}

impl RenderedDiagnostic {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn primary(&self) -> Option<&RenderedPrimarySpan> {
        self.primary.as_ref()
    }

    pub(crate) fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub(crate) fn notes(&self) -> &[Box<str>] {
        &self.notes
    }
}

impl RenderedPrimarySpan {
    pub(crate) fn display_name(&self) -> &str {
        &self.display_name
    }

    pub(crate) fn line_number(&self) -> usize {
        self.line_number
    }

    pub(crate) fn column_number(&self) -> usize {
        self.column_number
    }

    pub(crate) fn source_line(&self) -> &str {
        &self.source_line
    }

    pub(crate) fn highlight_start_column(&self) -> usize {
        self.highlight_start_column
    }

    pub(crate) fn highlight_columns(&self) -> usize {
        self.highlight_columns
    }
}

impl<'a> DiagnosticRenderer<'a> {
    pub(crate) const fn new(sources: SourceView<'a>) -> Self {
        Self { sources }
    }

    pub(crate) fn render(
        self,
        diagnostic: &UserDiagnostic,
    ) -> Result<RenderedDiagnostic, DiagnosticRenderError> {
        let primary = diagnostic
            .primary_span
            .map(|span| self.render_primary_span(span))
            .transpose()?;

        Ok(RenderedDiagnostic {
            message: diagnostic.message.clone(),
            primary,
            target: diagnostic.target.clone(),
            notes: diagnostic.notes.clone(),
        })
    }

    fn render_primary_span(
        self,
        span: SourceSpan,
    ) -> Result<RenderedPrimarySpan, DiagnosticRenderError> {
        let source_id = span.source_id();
        let source = self.sources.source(source_id)?;
        let display_name = self.sources.display_name(source_id)?;
        // #1578 requires display line/column to be derived here from the
        // registered source text, avoiding a second stored position authority.
        let line = line_origin(source, span.start());
        let start_in_line = span.start().min(line.line_end) - line.line_start;
        let end_in_line = span.end().min(line.line_end);
        let end_in_line = end_in_line.max(line.line_start + start_in_line);
        let highlight_columns =
            span_width_on_line(source, line.line_start + start_in_line, end_in_line);

        Ok(RenderedPrimarySpan {
            display_name: display_name.into(),
            line_number: line.line_number,
            column_number: display_width(&line.text[..start_in_line]) + 1,
            source_line: line.text.into(),
            highlight_start_column: display_width(&line.text[..start_in_line]) + 1,
            highlight_columns: highlight_columns.max(1),
        })
    }
}

impl fmt::Display for RenderedDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(primary) = &self.primary {
            writeln!(
                formatter,
                "{}:{}:{}: {}",
                primary.display_name, primary.line_number, primary.column_number, self.message
            )?;
            writeln!(
                formatter,
                "{} | {}",
                primary.line_number, primary.source_line
            )?;
            writeln!(
                formatter,
                "{} | {}{}",
                " ".repeat(decimal_width(primary.line_number)),
                " ".repeat(primary.highlight_start_column - 1),
                "^".repeat(primary.highlight_columns)
            )?;
        } else if let Some(target) = &self.target {
            // #1578 keeps source-less diagnostics source-less; this path must
            // not invent fallback positions such as `<stdin>:1:1`.
            writeln!(formatter, "{}: {}", target, self.message)?;
        } else {
            writeln!(formatter, "{}", self.message)?;
        }

        for note in &self.notes {
            writeln!(formatter, "note: {note}")?;
        }

        Ok(())
    }
}

impl From<SourceError> for DiagnosticRenderError {
    fn from(source: SourceError) -> Self {
        Self::Source { source }
    }
}

fn line_origin(source: &str, offset: usize) -> LineOrigin<'_> {
    debug_assert!(offset <= source.len());
    debug_assert!(source.is_char_boundary(offset));

    let mut line_number = 1;
    let mut line_start = 0;
    let mut cursor = 0;

    while cursor < offset {
        match source.as_bytes()[cursor] {
            b'\n' => {
                line_number += 1;
                line_start = cursor + 1;
                cursor += 1;
            }
            b'\r' => {
                let line_break_end = if source.as_bytes().get(cursor + 1) == Some(&b'\n') {
                    cursor + 2
                } else {
                    cursor + 1
                };
                if offset < line_break_end {
                    break;
                }

                line_number += 1;
                cursor = line_break_end;
                line_start = cursor;
            }
            _ => {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor should point inside source")
                    .len_utf8();
            }
        }
    }

    let line_end = line_end(source, line_start);

    LineOrigin {
        line_number,
        line_start,
        line_end,
        text: &source[line_start..line_end],
    }
}

fn line_end(source: &str, offset: usize) -> usize {
    let mut cursor = offset;
    while cursor < source.len() {
        match source.as_bytes()[cursor] {
            b'\n' | b'\r' => return cursor,
            _ => {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor should point inside source")
                    .len_utf8();
            }
        }
    }

    source.len()
}

fn span_width_on_line(source: &str, start: usize, end: usize) -> usize {
    debug_assert!(start <= end);
    debug_assert!(source.is_char_boundary(start));
    debug_assert!(source.is_char_boundary(end));

    display_width(&source[start..end])
}

fn display_width(text: &str) -> usize {
    text.chars().count()
}

fn decimal_width(mut value: usize) -> usize {
    let mut width = 1;
    while value >= 10 {
        width += 1;
        value /= 10;
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceId, SourceTexts, SourceView};

    fn source(text: &str, display_name: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text, display_name);
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    #[test]
    fn renders_single_line_primary_span_elements() {
        let (sources, source_id) = source("LET X = FOO(1)", "program.tbx");
        let primary = span(sources.view(), source_id, 8, 11);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(primary, "unknown word `FOO`"))
            .expect("diagnostic should render");
        let primary = rendered.primary().expect("primary span should render");

        assert_eq!(rendered.message(), "unknown word `FOO`");
        assert_eq!(primary.display_name(), "program.tbx");
        assert_eq!(primary.line_number(), 1);
        assert_eq!(primary.column_number(), 9);
        assert_eq!(primary.source_line(), "LET X = FOO(1)");
        assert_eq!(primary.highlight_start_column(), 9);
        assert_eq!(primary.highlight_columns(), 3);
        assert!(rendered.to_string().contains("program.tbx:1:9"));
        assert!(rendered.to_string().contains("^^^"));
    }

    #[test]
    fn renders_middle_line_from_multiline_source() {
        let (sources, source_id) = source("10 PRINT 1\n20 LET X = 2\n30 END", "multi.tbx");
        let primary = span(sources.view(), source_id, 18, 19);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(primary, "invalid target"))
            .expect("diagnostic should render");
        let primary = rendered.primary().expect("primary span should render");

        assert_eq!(primary.line_number(), 2);
        assert_eq!(primary.column_number(), 8);
        assert_eq!(primary.source_line(), "20 LET X = 2");
        assert_eq!(primary.highlight_columns(), 1);
    }

    #[test]
    fn renders_start_and_eof_positions() {
        let (sources, source_id) = source("A\nB", "edges.tbx");
        let start = span(sources.view(), source_id, 0, 1);
        let eof = span(sources.view(), source_id, 3, 3);

        let rendered_start = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(start, "start"))
            .expect("start diagnostic should render");
        let rendered_eof = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(eof, "eof"))
            .expect("eof diagnostic should render");

        assert_eq!(
            rendered_start
                .primary()
                .expect("start primary should render")
                .column_number(),
            1
        );
        let eof_primary = rendered_eof
            .primary()
            .expect("EOF primary should render on last physical line");
        assert_eq!(eof_primary.line_number(), 2);
        assert_eq!(eof_primary.column_number(), 2);
        assert_eq!(eof_primary.highlight_columns(), 1);
    }

    #[test]
    fn renders_stdin_display_name_as_display_info() {
        let (sources, source_id) = source("PRINT 1", "<stdin>");
        let primary = span(sources.view(), source_id, 0, 5);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(primary, "print failed"))
            .expect("diagnostic should render");

        assert_eq!(
            rendered
                .primary()
                .expect("primary span should render")
                .display_name(),
            "<stdin>"
        );
        assert!(rendered.to_string().contains("<stdin>:1:1"));
    }

    #[test]
    fn resolved_runtime_source_span_uses_the_same_renderer() {
        let (sources, source_id) = source("10 PRINT 1\n20 CR", "runtime.tbx");
        let resolved_runtime_span = Some(span(sources.view(), source_id, 14, 16));
        let diagnostic = match resolved_runtime_span {
            Some(span) => UserDiagnostic::at_span(span, "runtime output failed"),
            None => UserDiagnostic::without_source("runtime error", "source location unavailable"),
        };

        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&diagnostic)
            .expect("resolved runtime span should render");
        let primary = rendered.primary().expect("primary span should render");

        assert_eq!(primary.display_name(), "runtime.tbx");
        assert_eq!(primary.line_number(), 2);
        assert_eq!(primary.column_number(), 4);
        assert_eq!(primary.source_line(), "20 CR");
    }

    #[test]
    fn unicode_columns_and_highlight_use_the_same_basis() {
        let (sources, source_id) = source("LET あ = βγ", "unicode.tbx");
        let primary = span(sources.view(), source_id, 10, 14);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(primary, "unicode span"))
            .expect("diagnostic should render");
        let primary = rendered.primary().expect("primary span should render");

        assert_eq!(primary.column_number(), 9);
        assert_eq!(primary.highlight_start_column(), primary.column_number());
        assert_eq!(primary.highlight_columns(), 2);
        assert_eq!(primary.source_line(), "LET あ = βγ");
    }

    #[test]
    fn crlf_boundary_byte_does_not_break_line_extraction() {
        let (sources, source_id) = source("A\r\nB", "crlf.tbx");
        let primary = span(sources.view(), source_id, 2, 2);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(primary, "line boundary"))
            .expect("CRLF diagnostic should render");
        let primary = rendered.primary().expect("primary span should render");

        assert_eq!(primary.line_number(), 1);
        assert_eq!(primary.column_number(), 2);
        assert_eq!(primary.source_line(), "A");
        assert_eq!(primary.highlight_columns(), 1);
    }

    #[test]
    fn multiline_span_highlights_only_the_primary_physical_line() {
        let (sources, source_id) = source("A BC\nDEF", "span.tbx");
        let primary = span(sources.view(), source_id, 2, 7);
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(&UserDiagnostic::at_span(primary, "multi"))
            .expect("diagnostic should render");
        let primary = rendered.primary().expect("primary span should render");

        assert_eq!(primary.line_number(), 1);
        assert_eq!(primary.column_number(), 3);
        assert_eq!(primary.source_line(), "A BC");
        assert_eq!(primary.highlight_columns(), 2);
    }

    #[test]
    fn renders_source_less_diagnostic_without_fake_position() {
        let sources = SourceTexts::new();
        let rendered = DiagnosticRenderer::new(sources.view())
            .render(
                &UserDiagnostic::without_source(
                    "failed to read `program.tbx`",
                    "permission denied",
                )
                .with_note("check file permissions"),
            )
            .expect("source-less diagnostic should render");
        let text = rendered.to_string();

        assert_eq!(rendered.target(), Some("failed to read `program.tbx`"));
        assert_eq!(rendered.notes(), &["check file permissions".into()]);
        assert!(text.contains("failed to read `program.tbx`: permission denied"));
        assert!(text.contains("note: check file permissions"));
        assert!(!text.contains(":1:1"));
    }

    #[test]
    fn invalid_source_lookup_does_not_fallback_to_fake_position() {
        let (first_sources, first_source_id) = source("A", "first.tbx");
        let (second_sources, _) = source("B", "second.tbx");
        let foreign_span = span(first_sources.view(), first_source_id, 0, 1);

        let rendered = DiagnosticRenderer::new(second_sources.view())
            .render(&UserDiagnostic::at_span(foreign_span, "foreign"));

        assert!(matches!(
            rendered,
            Err(DiagnosticRenderError::Source {
                source: SourceError::InvalidSourceId { .. }
            })
        ));
    }
}
