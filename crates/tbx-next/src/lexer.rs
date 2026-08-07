use crate::source::{SourceError, SourceId, SourceSpan, SourceView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    kind: TokenKind,
    span: SourceSpan,
}

impl Token {
    pub(crate) const fn new(kind: TokenKind, span: SourceSpan) -> Self {
        Self { kind, span }
    }

    pub(crate) const fn kind(self) -> TokenKind {
        self.kind
    }

    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    IntegerLiteral,
    Name,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LineBoundary,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexError {
    Source(SourceError),
    InvalidCharacter {
        span: SourceSpan,
        character: char,
        reason: InvalidCharacterReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidCharacterReason {
    UnexpectedQuestionMark,
    UnsupportedPunctuation,
    UnsupportedControl,
    NonAscii,
}

impl From<SourceError> for LexError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

/// Non-incremental lexer over one complete source text.
///
/// The lexer keeps source identity and byte offsets only. It deliberately does
/// not normalize names, parse integer values, or bind source spelling to runtime
/// values; later phases must use token spans to inspect source text when needed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lexer<'a> {
    view: SourceView<'a>,
    source_id: SourceId,
    source: &'a str,
    offset: usize,
    eof: Token,
    terminal_error: Option<LexError>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(view: SourceView<'a>, source_id: SourceId) -> Result<Self, LexError> {
        let source = view.source(source_id)?;
        let eof_span = view.span(source_id, source.len(), source.len())?;

        Ok(Self {
            view,
            source_id,
            source,
            offset: 0,
            eof: Token::new(TokenKind::Eof, eof_span),
            terminal_error: None,
        })
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, LexError> {
        if let Some(error) = self.terminal_error {
            return Err(error);
        }

        match self.scan_next_token() {
            Ok(token) => Ok(token),
            Err(error) => {
                self.terminal_error = Some(error);
                Err(error)
            }
        }
    }

    fn scan_next_token(&mut self) -> Result<Token, LexError> {
        self.skip_horizontal_whitespace();

        if self.offset == self.source.len() {
            return Ok(self.eof);
        }

        let start = self.offset;
        let byte = self.source.as_bytes()[start];

        match byte {
            b'0'..=b'9' => self.integer_literal(),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.name(),
            b'+' => self.single_byte_token(TokenKind::Plus),
            b'-' => self.single_byte_token(TokenKind::Minus),
            b'*' => self.single_byte_token(TokenKind::Star),
            b'/' => self.single_byte_token(TokenKind::Slash),
            b'%' => self.single_byte_token(TokenKind::Percent),
            b'(' => self.single_byte_token(TokenKind::LParen),
            b')' => self.single_byte_token(TokenKind::RParen),
            b'=' => self.single_byte_token(TokenKind::Equal),
            b'<' => self.less_prefixed_operator(),
            b'>' => self.greater_prefixed_operator(),
            b'\n' => self.line_boundary(1),
            b'\r' => {
                let len = if self.source.as_bytes().get(start + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                self.line_boundary(len)
            }
            b'?' => {
                self.invalid_character(start, '?', InvalidCharacterReason::UnexpectedQuestionMark)
            }
            0x00..=0x1f | 0x7f => {
                let character = self.char_at(start);
                self.invalid_character(start, character, InvalidCharacterReason::UnsupportedControl)
            }
            0x80..=0xff => {
                let character = self.char_at(start);
                self.invalid_character(start, character, InvalidCharacterReason::NonAscii)
            }
            _ => {
                let character = byte as char;
                self.invalid_character(
                    start,
                    character,
                    InvalidCharacterReason::UnsupportedPunctuation,
                )
            }
        }
    }

    fn skip_horizontal_whitespace(&mut self) {
        while matches!(self.source.as_bytes().get(self.offset), Some(b' ' | b'\t')) {
            self.offset += 1;
        }
    }

    fn integer_literal(&mut self) -> Result<Token, LexError> {
        let start = self.offset;
        self.offset += 1;

        while matches!(self.source.as_bytes().get(self.offset), Some(b'0'..=b'9')) {
            self.offset += 1;
        }

        self.token(TokenKind::IntegerLiteral, start, self.offset)
    }

    fn name(&mut self) -> Result<Token, LexError> {
        let start = self.offset;
        self.offset += 1;

        while let Some(&byte) = self.source.as_bytes().get(self.offset) {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' => self.offset += 1,
                b'?' => return self.finish_predicate_name_or_error(start),
                _ if is_token_boundary(byte) => break,
                0x00..=0x1f | 0x7f => {
                    let character = self.char_at(self.offset);
                    return self.invalid_character(
                        self.offset,
                        character,
                        InvalidCharacterReason::UnsupportedControl,
                    );
                }
                0x80..=0xff => {
                    let character = self.char_at(self.offset);
                    return self.invalid_character(
                        self.offset,
                        character,
                        InvalidCharacterReason::NonAscii,
                    );
                }
                _ => {
                    let character = byte as char;
                    return self.invalid_character(
                        self.offset,
                        character,
                        InvalidCharacterReason::UnsupportedPunctuation,
                    );
                }
            }
        }

        self.token(TokenKind::Name, start, self.offset)
    }

    fn finish_predicate_name_or_error(&mut self, start: usize) -> Result<Token, LexError> {
        let question_mark = self.offset;
        let next = self.source.as_bytes().get(question_mark + 1).copied();

        if match next {
            Some(byte) => is_token_boundary(byte),
            None => true,
        } {
            self.offset += 1;
            return self.token(TokenKind::Name, start, self.offset);
        }

        self.invalid_character(
            question_mark,
            '?',
            InvalidCharacterReason::UnexpectedQuestionMark,
        )
    }

    fn single_byte_token(&mut self, kind: TokenKind) -> Result<Token, LexError> {
        let start = self.offset;
        self.offset += 1;
        self.token(kind, start, self.offset)
    }

    fn less_prefixed_operator(&mut self) -> Result<Token, LexError> {
        let start = self.offset;
        self.offset += 1;

        let kind = match self.source.as_bytes().get(self.offset) {
            Some(b'>') => {
                self.offset += 1;
                TokenKind::NotEqual
            }
            Some(b'=') => {
                self.offset += 1;
                TokenKind::LessEqual
            }
            _ => TokenKind::Less,
        };

        self.token(kind, start, self.offset)
    }

    fn greater_prefixed_operator(&mut self) -> Result<Token, LexError> {
        let start = self.offset;
        self.offset += 1;

        let kind = match self.source.as_bytes().get(self.offset) {
            Some(b'=') => {
                self.offset += 1;
                TokenKind::GreaterEqual
            }
            _ => TokenKind::Greater,
        };

        self.token(kind, start, self.offset)
    }

    fn line_boundary(&mut self, len: usize) -> Result<Token, LexError> {
        let start = self.offset;
        self.offset += len;
        self.token(TokenKind::LineBoundary, start, self.offset)
    }

    fn token(&self, kind: TokenKind, start: usize, end: usize) -> Result<Token, LexError> {
        let span = self.view.span(self.source_id, start, end)?;
        Ok(Token::new(kind, span))
    }

    fn invalid_character(
        &self,
        start: usize,
        character: char,
        reason: InvalidCharacterReason,
    ) -> Result<Token, LexError> {
        let span = self
            .view
            .span(self.source_id, start, start + character.len_utf8())?;
        Err(LexError::InvalidCharacter {
            span,
            character,
            reason,
        })
    }

    fn char_at(&self, offset: usize) -> char {
        self.source[offset..]
            .chars()
            .next()
            .expect("lexer offset should point at a source character")
    }
}

fn is_token_boundary(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'('
            | b')'
            | b'='
            | b'<'
            | b'>'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceTexts;

    fn lexer_for(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn lex_all(text: &str) -> (SourceTexts, SourceId, Vec<Token>) {
        let (sources, id) = lexer_for(text);
        let view = sources.view();
        let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token().expect("test source should lex");
            tokens.push(token);

            if token.kind() == TokenKind::Eof {
                break;
            }
        }

        (sources, id, tokens)
    }

    fn kinds(tokens: &[Token]) -> Vec<TokenKind> {
        tokens.iter().map(|token| token.kind()).collect()
    }

    fn slices<'a>(view: SourceView<'a>, tokens: &[Token]) -> Vec<&'a str> {
        tokens
            .iter()
            .map(|token| view.slice(token.span()).expect("token span should slice"))
            .collect()
    }

    fn assert_token(token: Token, kind: TokenKind, source_id: SourceId, start: usize, end: usize) {
        assert_eq!(token.kind(), kind);
        assert_eq!(token.span().source_id(), source_id);
        assert_eq!(token.span().start(), start);
        assert_eq!(token.span().end(), end);
    }

    #[test]
    fn empty_source_returns_only_eof() {
        let (_sources, id, tokens) = lex_all("");

        assert_eq!(tokens.len(), 1);
        assert_token(tokens[0], TokenKind::Eof, id, 0, 0);
    }

    #[test]
    fn integer_literals_keep_only_source_spans() {
        let (sources, id, tokens) = lex_all("123 45");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::IntegerLiteral,
                TokenKind::IntegerLiteral,
                TokenKind::Eof
            ]
        );
        assert_token(tokens[0], TokenKind::IntegerLiteral, id, 0, 3);
        assert_token(tokens[1], TokenKind::IntegerLiteral, id, 4, 6);
        assert_eq!(slices(sources.view(), &tokens), ["123", "45", ""]);
    }

    #[test]
    fn names_accept_ascii_word_shape_without_normalizing_spelling() {
        let (sources, id, tokens) = lex_all("a _ A1_b2 Ready?");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::Name,
                TokenKind::Name,
                TokenKind::Name,
                TokenKind::Name,
                TokenKind::Eof
            ]
        );
        assert_token(tokens[0], TokenKind::Name, id, 0, 1);
        assert_token(tokens[1], TokenKind::Name, id, 2, 3);
        assert_token(tokens[2], TokenKind::Name, id, 4, 9);
        assert_token(tokens[3], TokenKind::Name, id, 10, 16);
        assert_eq!(
            slices(sources.view(), &tokens),
            ["a", "_", "A1_b2", "Ready?", ""]
        );
    }

    #[test]
    fn minus_is_separate_from_integer_literals() {
        let (sources, _id, tokens) = lex_all("-1 2-3");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::Minus,
                TokenKind::IntegerLiteral,
                TokenKind::IntegerLiteral,
                TokenKind::Minus,
                TokenKind::IntegerLiteral,
                TokenKind::Eof
            ]
        );
        assert_eq!(
            slices(sources.view(), &tokens),
            ["-", "1", "2", "-", "3", ""]
        );
    }

    #[test]
    fn single_character_expression_operators_keep_source_spans() {
        let (sources, id, tokens) = lex_all("+ - * / % ( ) = < >");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Equal,
                TokenKind::Less,
                TokenKind::Greater,
                TokenKind::Eof
            ]
        );
        assert_token(tokens[0], TokenKind::Plus, id, 0, 1);
        assert_token(tokens[1], TokenKind::Minus, id, 2, 3);
        assert_token(tokens[2], TokenKind::Star, id, 4, 5);
        assert_token(tokens[3], TokenKind::Slash, id, 6, 7);
        assert_token(tokens[4], TokenKind::Percent, id, 8, 9);
        assert_token(tokens[5], TokenKind::LParen, id, 10, 11);
        assert_token(tokens[6], TokenKind::RParen, id, 12, 13);
        assert_token(tokens[7], TokenKind::Equal, id, 14, 15);
        assert_token(tokens[8], TokenKind::Less, id, 16, 17);
        assert_token(tokens[9], TokenKind::Greater, id, 18, 19);
        assert_eq!(
            slices(sources.view(), &tokens),
            ["+", "-", "*", "/", "%", "(", ")", "=", "<", ">", ""]
        );
    }

    #[test]
    fn multi_character_expression_operators_use_longest_match() {
        let (sources, id, tokens) = lex_all("<> <= >= << >> <=> <>=");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::NotEqual,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::Less,
                TokenKind::Less,
                TokenKind::Greater,
                TokenKind::Greater,
                TokenKind::LessEqual,
                TokenKind::Greater,
                TokenKind::NotEqual,
                TokenKind::Equal,
                TokenKind::Eof
            ]
        );
        assert_token(tokens[0], TokenKind::NotEqual, id, 0, 2);
        assert_token(tokens[1], TokenKind::LessEqual, id, 3, 5);
        assert_token(tokens[2], TokenKind::GreaterEqual, id, 6, 8);
        assert_eq!(
            slices(sources.view(), &tokens),
            ["<>", "<=", ">=", "<", "<", ">", ">", "<=", ">", "<>", "=", ""]
        );
    }

    #[test]
    fn unsupported_expression_punctuation_stays_structured_error() {
        for source in ["!", ",", ":"] {
            let (sources, id) = lexer_for(source);
            let view = sources.view();
            let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");
            let character = source.chars().next().expect("source should not be empty");

            assert_eq!(
                lexer.next_token(),
                Err(LexError::InvalidCharacter {
                    span: view
                        .span(id, 0, character.len_utf8())
                        .expect("punctuation span should validate"),
                    character,
                    reason: InvalidCharacterReason::UnsupportedPunctuation
                }),
                "{source:?} should remain unsupported punctuation"
            );
        }
    }

    #[test]
    fn expression_operators_are_name_boundaries_without_whitespace() {
        let (sources, _id, tokens) = lex_all("1+2 A*(B-3) READY?<>DONE X/Y%Z");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::IntegerLiteral,
                TokenKind::Plus,
                TokenKind::IntegerLiteral,
                TokenKind::Name,
                TokenKind::Star,
                TokenKind::LParen,
                TokenKind::Name,
                TokenKind::Minus,
                TokenKind::IntegerLiteral,
                TokenKind::RParen,
                TokenKind::Name,
                TokenKind::NotEqual,
                TokenKind::Name,
                TokenKind::Name,
                TokenKind::Slash,
                TokenKind::Name,
                TokenKind::Percent,
                TokenKind::Name,
                TokenKind::Eof
            ]
        );
        assert_eq!(
            slices(sources.view(), &tokens),
            [
                "1", "+", "2", "A", "*", "(", "B", "-", "3", ")", "READY?", "<>", "DONE", "X", "/",
                "Y", "%", "Z", ""
            ]
        );
    }

    #[test]
    fn skips_space_and_tab_only() {
        let (sources, _id, tokens) = lex_all(" \tA\t 12");

        assert_eq!(
            kinds(&tokens),
            [TokenKind::Name, TokenKind::IntegerLiteral, TokenKind::Eof]
        );
        assert_eq!(slices(sources.view(), &tokens), ["A", "12", ""]);
    }

    #[test]
    fn line_boundaries_preserve_lf_crlf_cr_empty_and_consecutive_lines() {
        let (sources, id, tokens) = lex_all("A\n\r\n\r\n\n\rB");

        assert_eq!(
            kinds(&tokens),
            [
                TokenKind::Name,
                TokenKind::LineBoundary,
                TokenKind::LineBoundary,
                TokenKind::LineBoundary,
                TokenKind::LineBoundary,
                TokenKind::LineBoundary,
                TokenKind::Name,
                TokenKind::Eof
            ]
        );
        assert_token(tokens[1], TokenKind::LineBoundary, id, 1, 2);
        assert_token(tokens[2], TokenKind::LineBoundary, id, 2, 4);
        assert_token(tokens[3], TokenKind::LineBoundary, id, 4, 6);
        assert_token(tokens[4], TokenKind::LineBoundary, id, 6, 7);
        assert_token(tokens[5], TokenKind::LineBoundary, id, 7, 8);
        assert_eq!(
            slices(sources.view(), &tokens),
            ["A", "\n", "\r\n", "\r\n", "\n", "\r", "B", ""]
        );
    }

    #[test]
    fn eof_repeats_after_it_is_reached() {
        let (sources, id) = lexer_for("A");
        let view = sources.view();
        let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");

        assert_eq!(
            lexer.next_token().expect("name should lex").kind(),
            TokenKind::Name
        );
        let first_eof = lexer.next_token().expect("first EOF should lex");
        let second_eof = lexer.next_token().expect("second EOF should lex");

        assert_eq!(first_eof, second_eof);
        assert_token(first_eof, TokenKind::Eof, id, 1, 1);
    }

    #[test]
    fn invalid_source_id_is_reported_as_source_error() {
        let (sources, valid) = lexer_for("A");
        let invalid = valid.test_next_slot();

        let Err(error) = Lexer::new(sources.view(), invalid) else {
            panic!("invalid source id should not build a lexer");
        };

        assert_eq!(
            error,
            LexError::Source(SourceError::InvalidSourceId { id: invalid })
        );
    }

    #[test]
    fn non_ascii_character_reports_utf8_character_span() {
        let (sources, id) = lexer_for("AあB");
        let view = sources.view();
        let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");

        assert_eq!(
            lexer.next_token(),
            Err(LexError::InvalidCharacter {
                span: view.span(id, 1, 4).expect("UTF-8 span should validate"),
                character: 'あ',
                reason: InvalidCharacterReason::NonAscii
            })
        );
    }

    #[test]
    fn invalid_question_mark_positions_do_not_return_partial_names() {
        for (source, start, end) in [("?", 0, 1), ("A?B", 1, 2), ("READY??", 5, 6)] {
            let (sources, id) = lexer_for(source);
            let view = sources.view();
            let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");

            assert_eq!(
                lexer.next_token(),
                Err(LexError::InvalidCharacter {
                    span: view
                        .span(id, start, end)
                        .expect("question mark span should validate"),
                    character: '?',
                    reason: InvalidCharacterReason::UnexpectedQuestionMark
                }),
                "{source:?} should reject an invalid question mark"
            );
        }
    }

    #[test]
    fn unsupported_punctuation_is_structured_error() {
        let (sources, id) = lexer_for("A,B");
        let view = sources.view();
        let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");

        assert_eq!(
            lexer.next_token(),
            Err(LexError::InvalidCharacter {
                span: view
                    .span(id, 1, 2)
                    .expect("punctuation span should validate"),
                character: ',',
                reason: InvalidCharacterReason::UnsupportedPunctuation
            })
        );
    }

    #[test]
    fn error_is_terminal_and_does_not_advance_to_later_tokens() {
        let (sources, id) = lexer_for("@ A");
        let view = sources.view();
        let mut lexer = Lexer::new(view, id).expect("test source should build a lexer");
        let expected = LexError::InvalidCharacter {
            span: view
                .span(id, 0, 1)
                .expect("punctuation span should validate"),
            character: '@',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        };

        assert_eq!(lexer.next_token(), Err(expected));
        assert_eq!(lexer.next_token(), Err(expected));
    }

    #[test]
    fn same_offsets_in_different_sources_keep_distinct_source_ids() {
        let mut sources = SourceTexts::new();
        let first = sources.register("A");
        let second = sources.register("B");
        let view = sources.view();
        let mut first_lexer = Lexer::new(view, first).expect("first source should lex");
        let mut second_lexer = Lexer::new(view, second).expect("second source should lex");

        let first_token = first_lexer.next_token().expect("first token should lex");
        let second_token = second_lexer.next_token().expect("second token should lex");

        assert_eq!(view.slice(first_token.span()), Ok("A"));
        assert_eq!(view.slice(second_token.span()), Ok("B"));
        assert_ne!(
            first_token.span().source_id(),
            second_token.span().source_id()
        );
        assert_eq!(first_token.span().start(), second_token.span().start());
        assert_eq!(first_token.span().end(), second_token.span().end());
    }
}
