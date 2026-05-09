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
        let mut source_excerpt = String::new();

        loop {
            let mut st = self.lexer.next_token();

            match st.token.clone() {
                Token::Error(message) => {
                    if paren_depth > 0 && recover_continuation_number(self.lexer.source(), &mut st)
                    {
                        end_line = st.pos.line;
                        if tokens.is_empty() {
                            start_line = st.pos.line;
                            start_col = st.pos.col;
                            source_excerpt =
                                line_text_for_offset(self.lexer.source(), st.source_offset);
                        }
                        tokens.push(st);
                        continue;
                    }
                    return Err(TbxError::InvalidExpression {
                        reason: lexer_error_reason(&message),
                    }
                    .into_reader_error(&st, self.lexer.source()));
                }
                Token::Newline => {
                    if paren_depth == 0 {
                        if tokens.is_empty() {
                            continue;
                        }
                        return Ok(Some(LogicalStatement {
                            tokens,
                            start_line,
                            start_col,
                            end_line,
                            source_excerpt,
                            terminator: StatementTerminator::Newline,
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
                    if tokens.is_empty() {
                        continue;
                    }
                    return Ok(Some(LogicalStatement {
                        tokens,
                        start_line,
                        start_col,
                        end_line,
                        source_excerpt,
                        terminator: StatementTerminator::Semicolon,
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
                    if tokens.is_empty() {
                        return Ok(None);
                    }
                    return Ok(Some(LogicalStatement {
                        tokens,
                        start_line,
                        start_col,
                        end_line,
                        source_excerpt,
                        terminator: StatementTerminator::Eof,
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
                Token::LineNum(n) if paren_depth > 0 => {
                    st.token = Token::IntLit(n);
                }
                _ => {}
            }

            if tokens.is_empty() {
                start_line = st.pos.line;
                start_col = st.pos.col;
                source_excerpt = line_text_for_offset(self.lexer.source(), st.source_offset);
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

fn recover_continuation_number(source: &str, st: &mut SpannedToken) -> bool {
    let Token::Error(message) = &st.token else {
        return false;
    };
    if !message.starts_with("invalid line number: ") {
        return false;
    }

    let raw = &source[st.source_offset..st.source_offset + st.source_len];
    match raw.parse::<f64>() {
        Ok(value) => {
            st.token = Token::FloatLit(value);
            true
        }
        Err(_) => false,
    }
}

fn lexer_error_reason(message: &str) -> &'static str {
    match message {
        "unterminated string literal" => "unterminated string literal",
        "unterminated string literal after escape" => "unterminated string literal after escape",
        "unexpected token: '..'" => "unexpected token: '..'",
        _ if message.starts_with("invalid line number: ") => "invalid line number",
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
    fn test_line_num_inside_parens_is_normalized_to_int_lit() {
        let statements = collect_statements("SET &A, TO_ARRAY(\n10, 20,\n30, 40\n)\n").unwrap();
        assert!(
            statements[0]
                .tokens
                .iter()
                .any(|st| matches!(st.token, Token::IntLit(30))),
            "expected continuation-line number token to be normalized to IntLit"
        );
        assert!(
            statements[0]
                .tokens
                .iter()
                .all(|st| !matches!(st.token, Token::LineNum(30))),
            "continuation-line number token must not remain LineNum"
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
}
