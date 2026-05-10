//! Read TBX source as logical statements rather than physical lines.

use crate::error::TbxError;
use crate::lexer::{Lexer, Position, SpannedToken, Token};

/// A statement extracted from the source stream.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalStatement {
    pub tokens: Vec<SpannedToken>,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub source_excerpt: String,
    pub terminator: StatementTerminator,
    /// Line-number label parsed off the head of this statement, if any.
    ///
    /// `StatementReader` strips a leading `Token::IntLit` at `paren_depth == 0`
    /// from `tokens` and stores its value here. The interpreter uses this for
    /// GOTO/BIF/BIT label registration during DEF compilation.
    pub label: Option<i64>,
}

/// Error produced while reading logical statements.
#[derive(Debug, Clone, PartialEq)]
pub struct StatementReaderError {
    pub line: usize,
    pub col: usize,
    pub source_excerpt: String,
    pub kind: TbxError,
}

/// The delimiter that ended a logical statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementTerminator {
    Newline,
    Semicolon,
    Eof,
}

/// Read logical statements from a token stream.
pub struct StatementReader<'a> {
    lexer: Lexer<'a>,
}

impl<'a> StatementReader<'a> {
    /// Create a new reader for the given source.
    pub fn new(source: &'a str) -> Self {
        Self {
            lexer: Lexer::new(source),
        }
    }

    /// Return the next non-empty logical statement, or `None` at EOF.
    pub fn next_statement(&mut self) -> Result<Option<LogicalStatement>, StatementReaderError> {
        let mut tokens = Vec::new();
        let mut paren_depth = 0usize;
        let mut open_parens: Vec<Position> = Vec::new();
        let mut start_line = 0usize;
        let mut start_col = 0usize;
        let mut end_line = 0usize;
        let mut label: Option<i64> = None;
        let mut label_seen = false;

        loop {
            let st = self.lexer.next_token();

            match st.token.clone() {
                Token::Error(message) => {
                    return Err(TbxError::InvalidExpression {
                        reason: lexer_error_reason(&message),
                    }
                    .into_reader_error(&st, self.lexer.source()));
                }
                Token::Newline => {
                    if paren_depth == 0 {
                        if tokens.is_empty() && label.is_none() {
                            continue;
                        }
                        return Ok(Some(LogicalStatement {
                            tokens,
                            start_line,
                            start_col,
                            end_line,
                            source_excerpt: source_excerpt_for_lines(
                                self.lexer.source(),
                                start_line,
                                end_line,
                            ),
                            terminator: StatementTerminator::Newline,
                            label,
                        }));
                    }
                    continue;
                }
                Token::Semicolon => {
                    if paren_depth > 0 {
                        return Err(TbxError::InvalidExpression {
                            reason: "semicolon is not allowed inside parentheses",
                        }
                        .into_reader_error(&st, self.lexer.source()));
                    }
                    if tokens.is_empty() && label.is_none() {
                        continue;
                    }
                    return Ok(Some(LogicalStatement {
                        tokens,
                        start_line,
                        start_col,
                        end_line,
                        source_excerpt: source_excerpt_for_lines(
                            self.lexer.source(),
                            start_line,
                            end_line,
                        ),
                        terminator: StatementTerminator::Semicolon,
                        label,
                    }));
                }
                Token::Eof => {
                    if paren_depth > 0 {
                        let error_pos = open_parens.last().unwrap_or(&st.pos);
                        return Err(TbxError::InvalidExpression {
                            reason: "unmatched '(' in statement",
                        }
                        .into_reader_error_at(error_pos, self.lexer.source()));
                    }
                    if tokens.is_empty() && label.is_none() {
                        return Ok(None);
                    }
                    return Ok(Some(LogicalStatement {
                        tokens,
                        start_line,
                        start_col,
                        end_line,
                        source_excerpt: source_excerpt_for_lines(
                            self.lexer.source(),
                            start_line,
                            end_line,
                        ),
                        terminator: StatementTerminator::Eof,
                        label,
                    }));
                }
                Token::RParen => {
                    if paren_depth == 0 {
                        return Err(TbxError::InvalidExpression {
                            reason: "unmatched ')' in statement",
                        }
                        .into_reader_error(&st, self.lexer.source()));
                    }
                    paren_depth -= 1;
                    open_parens.pop();
                }
                Token::LParen => {
                    paren_depth += 1;
                    open_parens.push(st.pos.clone());
                }
                _ => {}
            }

            // At paren_depth == 0, the very first meaningful token of a logical
            // statement is treated as a line-number label if it is an integer.
            // The label is stripped from `tokens` and stored on the statement.
            if !label_seen && paren_depth == 0 && tokens.is_empty() {
                label_seen = true;
                if let Token::IntLit(n) = st.token {
                    label = Some(n);
                    start_line = st.pos.line;
                    start_col = st.pos.col;
                    end_line = st.pos.line;
                    continue;
                }
            }

            if tokens.is_empty() && label.is_none() {
                start_line = st.pos.line;
                start_col = st.pos.col;
            }
            end_line = st.pos.line;
            tokens.push(st);
        }
    }
}

trait IntoStatementReaderError {
    fn into_reader_error(self, st: &SpannedToken, source: &str) -> StatementReaderError;
    fn into_reader_error_at(self, pos: &Position, source: &str) -> StatementReaderError;
}

impl IntoStatementReaderError for TbxError {
    fn into_reader_error(self, st: &SpannedToken, source: &str) -> StatementReaderError {
        StatementReaderError {
            line: st.pos.line,
            col: st.pos.col,
            source_excerpt: line_text_for_offset(source, st.source_offset),
            kind: self,
        }
    }

    fn into_reader_error_at(self, pos: &Position, source: &str) -> StatementReaderError {
        StatementReaderError {
            line: pos.line,
            col: pos.col,
            source_excerpt: line_text_for_line(source, pos.line),
            kind: self,
        }
    }
}

fn line_text_for_offset(source: &str, offset: usize) -> String {
    let start = source[..offset].rfind('\n').map_or(0, |idx| idx + 1);
    let end = source[offset..]
        .find('\n')
        .map_or(source.len(), |idx| offset + idx);
    source[start..end].trim_end_matches('\r').to_string()
}

fn line_text_for_line(source: &str, line: usize) -> String {
    source
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string()
}

fn source_excerpt_for_lines(source: &str, start_line: usize, end_line: usize) -> String {
    if start_line == 0 || end_line == 0 || end_line < start_line {
        return String::new();
    }

    (start_line..=end_line)
        .map(|line| line_text_for_line(source, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn lexer_error_reason(message: &str) -> &'static str {
    match message {
        "unterminated string literal" => "unterminated string literal",
        "unterminated string literal after escape" => "unterminated string literal after escape",
        "unexpected token: '..'" => "unexpected token: '..'",
        _ if message.starts_with("integer literal out of range: ") => {
            "integer literal out of range"
        }
        _ if message.starts_with("float literal out of range: ") => "float literal out of range",
        _ if message.starts_with("unexpected character: ") => "unexpected character",
        _ => "lexer error in statement reader",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_statements(src: &str) -> Result<Vec<LogicalStatement>, StatementReaderError> {
        let mut reader = StatementReader::new(src);
        let mut statements = Vec::new();
        while let Some(stmt) = reader.next_statement()? {
            statements.push(stmt);
        }
        Ok(statements)
    }

    #[test]
    fn test_single_statement() {
        let statements = collect_statements("PUTDEC 1").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].start_line, 1);
        assert_eq!(statements[0].start_col, 1);
        assert_eq!(statements[0].end_line, 1);
        assert_eq!(statements[0].source_excerpt, "PUTDEC 1");
        assert_eq!(statements[0].terminator, StatementTerminator::Eof);
    }

    #[test]
    fn test_empty_lines_are_skipped() {
        let statements = collect_statements("\n  \nPUTDEC 1\n").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].tokens[0].pos.line, 3);
        assert_eq!(statements[0].source_excerpt, "PUTDEC 1");
    }

    #[test]
    fn test_semicolon_splits_two_statements() {
        let statements = collect_statements("PUTDEC 1; PUTDEC 2").unwrap();
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].terminator, StatementTerminator::Semicolon);
        assert_eq!(statements[1].terminator, StatementTerminator::Eof);
    }

    #[test]
    fn test_empty_segments_from_semicolons_are_skipped() {
        let statements = collect_statements("; PUTDEC 1;; PUTDEC 2;").unwrap();
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].source_excerpt, "; PUTDEC 1;; PUTDEC 2;");
        assert_eq!(statements[1].source_excerpt, "; PUTDEC 1;; PUTDEC 2;");
    }

    #[test]
    fn test_multiline_parenthesized_statement_is_single_statement() {
        let statements = collect_statements("SET &A, TO_ARRAY(\n  1, 2,\n  3, 4\n)\n").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].start_line, 1);
        assert_eq!(statements[0].end_line, 4);
        assert_eq!(statements[0].terminator, StatementTerminator::Newline);
        assert_eq!(
            statements[0].source_excerpt,
            "SET &A, TO_ARRAY(\n  1, 2,\n  3, 4\n)"
        );
    }

    #[test]
    fn test_newline_inside_parens_is_not_emitted() {
        let statements = collect_statements("SET &A, TO_ARRAY(\n  1, 2\n)\n").unwrap();
        assert!(
            statements[0]
                .tokens
                .iter()
                .all(|st| !matches!(st.token, Token::Newline)),
            "logical statement must not contain Newline tokens inside parentheses"
        );
    }

    #[test]
    fn test_int_inside_parens_stays_int_lit() {
        let statements = collect_statements("SET &A, TO_ARRAY(\n10, 20,\n30, 40\n)\n").unwrap();
        assert!(
            statements[0]
                .tokens
                .iter()
                .any(|st| matches!(st.token, Token::IntLit(30))),
            "expected continuation-line integer token to be IntLit"
        );
    }

    #[test]
    fn test_float_at_start_of_continuation_line_is_recovered() {
        let statements = collect_statements("SET &A, TO_ARRAY(\n1.5, 2.5,\n3.5, 4.5\n)\n").unwrap();
        assert!(
            statements[0]
                .tokens
                .iter()
                .any(|st| matches!(st.token, Token::FloatLit(value) if value == 1.5)),
            "expected continuation-line float token to be recovered as FloatLit"
        );
    }

    #[test]
    fn test_semicolon_inside_parens_is_error() {
        let err = collect_statements("SET &A, TO_ARRAY(1; 2)").unwrap_err();
        assert!(matches!(err.kind, TbxError::InvalidExpression { .. }));
    }

    #[test]
    fn test_unmatched_open_paren_is_error() {
        let err = collect_statements("SET &A, TO_ARRAY(1, 2").unwrap_err();
        assert!(matches!(
            err.kind,
            TbxError::InvalidExpression {
                reason: "unmatched '(' in statement"
            }
        ));
        assert_eq!(err.line, 1);
        assert_eq!(err.col, 17);
    }

    #[test]
    fn test_unmatched_close_paren_is_error() {
        let err = collect_statements("PUTDEC 1)").unwrap_err();
        assert!(matches!(
            err.kind,
            TbxError::InvalidExpression {
                reason: "unmatched ')' in statement"
            }
        ));
    }

    #[test]
    fn test_hash_comment_inside_parens_can_continue() {
        let statements =
            collect_statements("SET &A, TO_ARRAY(\n  1, 2, # first row\n  3, 4\n)\n").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].end_line, 4);
    }

    #[test]
    fn test_rem_line_is_read_as_single_statement() {
        let statements = collect_statements("REM hello world\nPUTDEC 1\n").unwrap();
        assert_eq!(statements.len(), 2);
        assert!(matches!(
            statements[0].tokens.first().map(|st| &st.token),
            Some(Token::Ident(name)) if name == "REM"
        ));
        assert_eq!(statements[0].terminator, StatementTerminator::Newline);
    }

    #[test]
    fn test_lexer_error_reason_is_preserved_for_unterminated_string() {
        let err = collect_statements("PUTSTR \"unterminated\n").unwrap_err();
        assert!(matches!(
            err.kind,
            TbxError::InvalidExpression {
                reason: "unterminated string literal"
            }
        ));
    }

    #[test]
    fn test_leading_int_at_paren_depth_zero_is_extracted_as_label() {
        // A leading integer at the start of a logical statement is recognized
        // as a line-number label and stripped from `tokens`.
        let statements = collect_statements("10 PUTDEC 42").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].label, Some(10));
        // The leading `10` must NOT remain in the token list.
        assert!(matches!(
            statements[0].tokens.first().map(|st| &st.token),
            Some(Token::Ident(name)) if name == "PUTDEC"
        ));
    }

    #[test]
    fn test_no_leading_int_means_label_is_none() {
        let statements = collect_statements("PUTDEC 42").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].label, None);
    }

    #[test]
    fn test_bare_label_line_yields_statement_with_no_tokens() {
        // `10` alone on a line still produces a statement so that the
        // interpreter can register the label inside a DEF body.
        let statements = collect_statements("10\n").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].label, Some(10));
        assert!(statements[0].tokens.is_empty());
    }

    #[test]
    fn test_int_after_first_token_is_not_a_label() {
        // The integer 99 here is the second meaningful token of the statement
        // and must remain a regular IntLit (no label).
        let statements = collect_statements("PUTDEC 99").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].label, None);
        assert!(matches!(
            statements[0].tokens.last().map(|st| &st.token),
            Some(Token::IntLit(99))
        ));
    }

    #[test]
    fn test_continuation_line_integer_is_not_a_label() {
        // A multi-line parenthesized statement: the `30` on the second
        // continuation line must stay a plain IntLit, not a label.
        let statements = collect_statements("SET &A, TO_ARRAY(\n10, 20,\n30, 40\n)\n").unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].label, None);
    }

    #[test]
    fn test_continuation_line_float_is_floatlit() {
        // After issue #534, floats at the start of a continuation line are
        // emitted by the lexer as plain FloatLit tokens (no recovery needed).
        let statements = collect_statements("SET &A, TO_ARRAY(\n1.5, 2.5,\n3.5, 4.5\n)\n").unwrap();
        assert!(
            statements[0]
                .tokens
                .iter()
                .any(|st| matches!(st.token, Token::FloatLit(value) if value == 1.5)),
            "expected continuation-line float to be FloatLit"
        );
    }

    #[test]
    fn test_label_with_trailing_semicolon() {
        // `10; PUTDEC 1` → first statement has label 10 with empty tokens,
        // second statement has no label and `PUTDEC 1` tokens.
        let statements = collect_statements("10; PUTDEC 1").unwrap();
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].label, Some(10));
        assert!(statements[0].tokens.is_empty());
        assert_eq!(statements[1].label, None);
    }
}
