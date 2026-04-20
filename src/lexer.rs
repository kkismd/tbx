use std::iter::Peekable;
use std::str::CharIndices;

// TOKEN kind codes (mirrors blueprint-language.md "トークン・ディスクリプタ")
pub const TOK_ID: i64 = 0;
pub const TOK_NUM: i64 = 1;
pub const TOK_OP: i64 = 2;
pub const TOK_DELIM: i64 = 3;
pub const TOK_STR: i64 = 4;

/// Lexical token produced by the TBX lexer.
///
/// `Newline`, `Eof`, and `Error` are outer-interpreter-only tokens;
/// they are never exposed through the `TOKEN` primitive.
/// All other variants map directly to one of the five TOKEN kind codes.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Identifier — word name or variable name. Maps to TOK_ID.
    Ident(String),
    /// Integer literal. Maps to TOK_NUM.
    IntLit(i64),
    /// Floating-point literal. Maps to TOK_NUM.
    FloatLit(f64),
    /// String literal with escape sequences already expanded. Maps to TOK_STR.
    StringLit(String),
    /// Operator string: `+` `-` `*` `/` `%` `=` `<>` `<` `>` `<=` `>=` `|` `||` `&&`.
    /// Maps to TOK_OP.
    Op(String),
    /// Comma `,`. Acts as a low-priority binary operator (argument separator).
    /// Maps to TOK_OP.
    Comma,
    /// Semicolon `;`. Statement terminator or inline comment start.
    /// The outer interpreter decides which; maps to TOK_DELIM.
    Semicolon,
    /// Ampersand `&`. Used as both unary reference and binary bitwise AND.
    /// The SYA parser determines unary vs binary from context. Maps to TOK_OP.
    Ampersand,
    /// Left parenthesis `(`. Maps to TOK_OP.
    LParen,
    /// Right parenthesis `)`. Maps to TOK_OP.
    RParen,
    /// Integer label at the start of a line (before any identifier).
    /// Used by GOTO/BIF/BIT as jump targets. Outer-interpreter-only.
    LineNum(i64),
    /// Line terminator (newline or end of statement). Outer-interpreter-only.
    Newline,
    /// End of input. Outer-interpreter-only.
    Eof,
    /// Lexer error (e.g. unterminated string literal). Outer-interpreter-only.
    Error(String),
}

impl Token {
    /// Returns the TOKEN kind code for use by the TBX `TOKEN` primitive.
    ///
    /// Returns `None` for outer-interpreter-only tokens (`Newline`, `Eof`, `Error`, `LineNum`).
    pub fn kind_code(&self) -> Option<i64> {
        match self {
            Token::Ident(_) => Some(TOK_ID),
            Token::IntLit(_) | Token::FloatLit(_) => Some(TOK_NUM),
            Token::Op(_) | Token::Comma | Token::Ampersand | Token::LParen | Token::RParen => {
                Some(TOK_OP)
            }
            Token::Semicolon => Some(TOK_DELIM),
            Token::StringLit(_) => Some(TOK_STR),
            Token::LineNum(_) | Token::Newline | Token::Eof | Token::Error(_) => None,
        }
    }
}

/// Source position (1-based line and column numbers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// Line number, starting at 1.
    pub line: usize,
    /// Column number (character index within the line), starting at 1.
    pub col: usize,
}

impl Position {
    fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// A token together with its source location and raw-text extents.
///
/// `source_offset` and `source_len` are byte offsets into the original source
/// string passed to `Lexer::new`. They are used by the future `TOKEN` primitive
/// to return `Cell::DictAddr(source_offset)` (for identifiers/operators) or
/// `Cell::Int(len)` for the `len` field.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    /// Position of the first character of the token.
    pub pos: Position,
    /// Byte offset of the first character of the raw token text in the source string.
    pub source_offset: usize,
    /// Byte length of the raw token text in the source string.
    pub source_len: usize,
}

/// The TBX lexer: converts a source string into a stream of `SpannedToken`s.
///
/// Usage:
/// ```ignore
/// let mut lex = Lexer::new("PUTDEC 42\n");
/// let tok = lex.next_token();   // consumes
/// let tok = lex.peek_token();   // does not consume
/// ```
pub struct Lexer<'a> {
    source: &'a str,
    chars: Peekable<CharIndices<'a>>,
    /// Current line number (1-based).
    line: usize,
    /// Current column number (1-based, character index).
    col: usize,
    /// True at the start of each line (before any non-whitespace token on that line).
    /// When true, an integer token is classified as `LineNum` rather than `IntLit`.
    at_line_start: bool,
    /// Set to true after emitting `Ident("REM")`.
    /// The next call to `next_token_inner()` will skip to end-of-line and return `Newline`.
    rem_pending: bool,
    /// One-token lookahead buffer for `peek_token()`.
    peeked: Option<SpannedToken>,
}

impl<'a> Lexer<'a> {
    /// Create a new `Lexer` for the given source string.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            line: 1,
            col: 1,
            at_line_start: true,
            rem_pending: false,
            peeked: None,
        }
    }

    /// Return the source string that was passed to `new`.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Current position in the source (points to the character that will be
    /// read by the next scan operation).
    pub fn position(&self) -> Position {
        Position::new(self.line, self.col)
    }

    /// Consume and return the next token.
    pub fn next_token(&mut self) -> SpannedToken {
        if let Some(t) = self.peeked.take() {
            return t;
        }
        self.next_token_inner()
    }

    /// Peek at the next token without consuming it.
    /// Repeated calls return the same token.
    pub fn peek_token(&mut self) -> &SpannedToken {
        if self.peeked.is_none() {
            let t = self.next_token_inner();
            self.peeked = Some(t);
        }
        self.peeked.as_ref().unwrap()
    }

    // -----------------------------------------------------------------------
    // Internal scanning helpers
    // -----------------------------------------------------------------------

    /// Peek at the next raw character without consuming it.
    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    /// Consume the next raw character and advance position counters.
    fn advance(&mut self) -> Option<(usize, char)> {
        let result = self.chars.next();
        if let Some((_, c)) = result {
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        result
    }

    /// Peek at the byte offset of the next character without consuming it.
    fn peek_offset(&mut self) -> usize {
        self.chars
            .peek()
            .map(|(off, _)| *off)
            .unwrap_or(self.source.len())
    }

    /// Skip over space and tab characters, updating position.
    /// Does NOT skip newlines.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(' ') | Some('\t')) {
            self.advance();
        }
    }

    /// Core token scanner.  Called by `next_token` and internally to fill `peeked`.
    fn next_token_inner(&mut self) -> SpannedToken {
        // Flush REM-pending state: skip rest of line, consume the newline, emit Newline.
        if self.rem_pending {
            self.rem_pending = false;
            let start_off = self.peek_offset();
            let start_pos = self.position();
            // Skip non-newline characters.
            while !matches!(self.peek_char(), None | Some('\n') | Some('\r')) {
                self.advance();
            }
            // Consume the actual newline character(s) so the next token starts on the next line.
            if self.peek_char() == Some('\r') {
                self.advance();
            }
            if self.peek_char() == Some('\n') {
                self.advance();
            }
            let end_off = self.peek_offset();
            return self.emit_newline(start_pos, start_off, end_off - start_off);
        }

        self.skip_whitespace();

        let start_off = self.peek_offset();
        let start_pos = self.position();

        match self.peek_char() {
            None => SpannedToken {
                token: Token::Eof,
                pos: start_pos,
                source_offset: start_off,
                source_len: 0,
            },

            Some('\r') | Some('\n') => self.scan_newline(start_pos, start_off),

            Some('"') => self.scan_string(start_pos, start_off),

            Some(c) if c.is_ascii_digit() => self.scan_number(start_pos, start_off),

            Some(c) if c.is_alphabetic() || c == '_' => self.scan_ident(start_pos, start_off),

            Some(_) => self.scan_operator(start_pos, start_off),
        }
    }

    /// Scan and consume a newline (`\r\n` counts as one).
    fn scan_newline(&mut self, start_pos: Position, start_off: usize) -> SpannedToken {
        if self.peek_char() == Some('\r') {
            self.advance();
        }
        if self.peek_char() == Some('\n') {
            self.advance();
        }
        let end_off = self.peek_offset();
        let raw_len = end_off - start_off;
        self.emit_newline(start_pos, start_off, raw_len)
    }

    /// Build a `Newline` token and reset `at_line_start`.
    fn emit_newline(&mut self, pos: Position, offset: usize, len: usize) -> SpannedToken {
        self.at_line_start = true;
        SpannedToken {
            token: Token::Newline,
            pos,
            source_offset: offset,
            source_len: len,
        }
    }

    /// Scan an integer or float literal, or a line-number label.
    fn scan_number(&mut self, start_pos: Position, start_off: usize) -> SpannedToken {
        let is_line_num = self.at_line_start;
        self.at_line_start = false;

        // Consume digit sequence.
        while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            self.advance();
        }

        // Check for decimal point (but not `..` or `.WORD`).
        let is_float = if self.peek_char() == Some('.') {
            // Look two characters ahead: if next-next is a digit, it's a float.
            let snapshot: Vec<_> = self.chars.clone().take(2).collect();
            matches!(snapshot.get(1), Some((_, c)) if c.is_ascii_digit())
        } else {
            false
        };

        if is_float {
            self.advance(); // consume '.'
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                self.advance();
            }
            // Optional exponent: e or E followed by optional sign and digits.
            if matches!(self.peek_char(), Some('e') | Some('E')) {
                self.advance();
                if matches!(self.peek_char(), Some('+') | Some('-')) {
                    self.advance();
                }
                while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                    self.advance();
                }
            }
        }

        let end_off = self.peek_offset();
        let raw = &self.source[start_off..end_off];
        let raw_len = end_off - start_off;

        // A float at line-start is not a valid line number label.
        if is_line_num && is_float {
            return SpannedToken {
                token: Token::Error(format!("invalid line number: {raw}")),
                pos: start_pos,
                source_offset: start_off,
                source_len: raw_len,
            };
        }

        if is_line_num {
            match raw.parse::<i64>() {
                Ok(n) => {
                    return SpannedToken {
                        token: Token::LineNum(n),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: raw_len,
                    }
                }
                Err(_) => {
                    return SpannedToken {
                        token: Token::Error(format!("integer literal out of range: {raw}")),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: raw_len,
                    }
                }
            }
        }

        if is_float {
            match raw.parse::<f64>() {
                Ok(f) => SpannedToken {
                    token: Token::FloatLit(f),
                    pos: start_pos,
                    source_offset: start_off,
                    source_len: raw_len,
                },
                Err(_) => SpannedToken {
                    token: Token::Error(format!("float literal out of range: {raw}")),
                    pos: start_pos,
                    source_offset: start_off,
                    source_len: raw_len,
                },
            }
        } else {
            match raw.parse::<i64>() {
                Ok(n) => SpannedToken {
                    token: Token::IntLit(n),
                    pos: start_pos,
                    source_offset: start_off,
                    source_len: raw_len,
                },
                Err(_) => SpannedToken {
                    token: Token::Error(format!("integer literal out of range: {raw}")),
                    pos: start_pos,
                    source_offset: start_off,
                    source_len: raw_len,
                },
            }
        }
    }

    /// Scan an identifier.  If the identifier is `REM`, set `rem_pending`.
    fn scan_ident(&mut self, start_pos: Position, start_off: usize) -> SpannedToken {
        self.at_line_start = false;
        while matches!(self.peek_char(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.advance();
        }
        let end_off = self.peek_offset();
        let name = self.source[start_off..end_off].to_string();
        let raw_len = end_off - start_off;

        if name == "REM" {
            self.rem_pending = true;
        }

        SpannedToken {
            token: Token::Ident(name),
            pos: start_pos,
            source_offset: start_off,
            source_len: raw_len,
        }
    }

    /// Scan a double-quoted string literal, expanding escape sequences.
    ///
    /// The opening `"` must already be the next character.
    /// Returns `Token::Error` if the string is unterminated.
    fn scan_string(&mut self, start_pos: Position, start_off: usize) -> SpannedToken {
        self.at_line_start = false;
        self.advance(); // consume opening '"'

        let mut value = String::new();
        loop {
            match self.peek_char() {
                None | Some('\n') | Some('\r') => {
                    let end_off = self.peek_offset();
                    return SpannedToken {
                        token: Token::Error("unterminated string literal".to_string()),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off - start_off,
                    };
                }
                Some('"') => {
                    self.advance(); // consume closing '"'
                    break;
                }
                Some('\\') => {
                    self.advance(); // consume '\'
                    match self.peek_char() {
                        Some('n') => {
                            self.advance();
                            value.push('\n');
                        }
                        Some('t') => {
                            self.advance();
                            value.push('\t');
                        }
                        Some('\\') => {
                            self.advance();
                            value.push('\\');
                        }
                        Some('"') => {
                            self.advance();
                            value.push('"');
                        }
                        Some(c) => {
                            // Unknown escape: keep the backslash and the character.
                            self.advance();
                            value.push('\\');
                            value.push(c);
                        }
                        None => {
                            let end_off = self.peek_offset();
                            return SpannedToken {
                                token: Token::Error(
                                    "unterminated string literal after escape".to_string(),
                                ),
                                pos: start_pos,
                                source_offset: start_off,
                                source_len: end_off - start_off,
                            };
                        }
                    }
                }
                Some(c) => {
                    self.advance();
                    value.push(c);
                }
            }
        }

        let end_off = self.peek_offset();
        SpannedToken {
            token: Token::StringLit(value),
            pos: start_pos,
            source_offset: start_off,
            source_len: end_off - start_off,
        }
    }

    /// Scan an operator or punctuation character.
    fn scan_operator(&mut self, start_pos: Position, start_off: usize) -> SpannedToken {
        self.at_line_start = false;

        // Helper macro to build SpannedToken after consuming n chars.
        // (Not a real macro — we build inline.)

        let first = self.advance().map(|(_, c)| c).unwrap_or('\0');
        let end_off = self.peek_offset();

        match first {
            ',' => SpannedToken {
                token: Token::Comma,
                pos: start_pos,
                source_offset: start_off,
                source_len: end_off - start_off,
            },
            ';' => SpannedToken {
                token: Token::Semicolon,
                pos: start_pos,
                source_offset: start_off,
                source_len: end_off - start_off,
            },
            '(' => SpannedToken {
                token: Token::LParen,
                pos: start_pos,
                source_offset: start_off,
                source_len: end_off - start_off,
            },
            ')' => SpannedToken {
                token: Token::RParen,
                pos: start_pos,
                source_offset: start_off,
                source_len: end_off - start_off,
            },
            '+' => self.op1("+", start_pos, start_off),
            '*' => self.op1("*", start_pos, start_off),
            '/' => self.op1("/", start_pos, start_off),
            '%' => self.op1("%", start_pos, start_off),
            '=' => self.op1("=", start_pos, start_off),
            '-' => self.op1("-", start_pos, start_off),
            '&' => {
                if self.peek_char() == Some('&') {
                    self.advance();
                    let end_off2 = self.peek_offset();
                    SpannedToken {
                        token: Token::Op("&&".to_string()),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off2 - start_off,
                    }
                } else {
                    SpannedToken {
                        token: Token::Ampersand,
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off - start_off,
                    }
                }
            }
            '|' => {
                if self.peek_char() == Some('|') {
                    self.advance();
                    let end_off2 = self.peek_offset();
                    SpannedToken {
                        token: Token::Op("||".to_string()),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off2 - start_off,
                    }
                } else {
                    self.op1_with_end("|", start_pos, start_off, end_off)
                }
            }
            '<' => match self.peek_char() {
                Some('>') => {
                    self.advance();
                    let end_off2 = self.peek_offset();
                    SpannedToken {
                        token: Token::Op("<>".to_string()),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off2 - start_off,
                    }
                }
                Some('=') => {
                    self.advance();
                    let end_off2 = self.peek_offset();
                    SpannedToken {
                        token: Token::Op("<=".to_string()),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off2 - start_off,
                    }
                }
                _ => self.op1_with_end("<", start_pos, start_off, end_off),
            },
            '>' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    let end_off2 = self.peek_offset();
                    SpannedToken {
                        token: Token::Op(">=".to_string()),
                        pos: start_pos,
                        source_offset: start_off,
                        source_len: end_off2 - start_off,
                    }
                } else {
                    self.op1_with_end(">", start_pos, start_off, end_off)
                }
            }
            '!' => self.op1_with_end("!", start_pos, start_off, end_off),
            c => {
                let end_off2 = self.peek_offset();
                SpannedToken {
                    token: Token::Error(format!("unexpected character: {:?}", c)),
                    pos: start_pos,
                    source_offset: start_off,
                    source_len: end_off2 - start_off,
                }
            }
        }
    }

    /// Build a single-char `Op` token using the current `peek_offset()` as the end.
    fn op1(&mut self, s: &str, pos: Position, start_off: usize) -> SpannedToken {
        let end_off = self.peek_offset();
        self.op1_with_end(s, pos, start_off, end_off)
    }

    fn op1_with_end(
        &self,
        s: &str,
        pos: Position,
        start_off: usize,
        end_off: usize,
    ) -> SpannedToken {
        SpannedToken {
            token: Token::Op(s.to_string()),
            pos,
            source_offset: start_off,
            source_len: end_off - start_off,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<Token> {
        let mut lex = Lexer::new(src);
        let mut result = Vec::new();
        loop {
            let t = lex.next_token();
            let done = t.token == Token::Eof;
            result.push(t.token);
            if done {
                break;
            }
        }
        result
    }

    fn spanned(src: &str) -> Vec<SpannedToken> {
        let mut lex = Lexer::new(src);
        let mut result = Vec::new();
        loop {
            let t = lex.next_token();
            let done = t.token == Token::Eof;
            result.push(t);
            if done {
                break;
            }
        }
        result
    }

    // --- Token::kind_code ---

    #[test]
    fn test_kind_code_ident() {
        assert_eq!(Token::Ident("X".to_string()).kind_code(), Some(TOK_ID));
    }

    #[test]
    fn test_kind_code_intlit() {
        assert_eq!(Token::IntLit(42).kind_code(), Some(TOK_NUM));
    }

    #[test]
    fn test_kind_code_floatlit() {
        assert_eq!(Token::FloatLit(1.25).kind_code(), Some(TOK_NUM));
    }

    #[test]
    fn test_kind_code_op() {
        assert_eq!(Token::Op("+".to_string()).kind_code(), Some(TOK_OP));
    }

    #[test]
    fn test_kind_code_comma() {
        assert_eq!(Token::Comma.kind_code(), Some(TOK_OP));
    }

    #[test]
    fn test_kind_code_ampersand() {
        assert_eq!(Token::Ampersand.kind_code(), Some(TOK_OP));
    }

    #[test]
    fn test_kind_code_lparen() {
        assert_eq!(Token::LParen.kind_code(), Some(TOK_OP));
    }

    #[test]
    fn test_kind_code_rparen() {
        assert_eq!(Token::RParen.kind_code(), Some(TOK_OP));
    }

    #[test]
    fn test_kind_code_semicolon() {
        assert_eq!(Token::Semicolon.kind_code(), Some(TOK_DELIM));
    }

    #[test]
    fn test_kind_code_string() {
        assert_eq!(Token::StringLit("x".to_string()).kind_code(), Some(TOK_STR));
    }

    #[test]
    fn test_kind_code_outer_only_tokens_return_none() {
        assert_eq!(Token::Newline.kind_code(), None);
        assert_eq!(Token::Eof.kind_code(), None);
        assert_eq!(Token::Error("x".to_string()).kind_code(), None);
        assert_eq!(Token::LineNum(10).kind_code(), None);
    }

    // --- identifier ---

    #[test]
    fn test_ident_simple() {
        assert_eq!(
            tokens("PUTDEC"),
            vec![Token::Ident("PUTDEC".to_string()), Token::Eof]
        );
    }

    #[test]
    fn test_ident_with_digits_and_underscore() {
        assert_eq!(
            tokens("MY_VAR2"),
            vec![Token::Ident("MY_VAR2".to_string()), Token::Eof]
        );
    }

    // --- integer literal ---

    #[test]
    fn test_intlit_simple() {
        // A number that is NOT at line start should be IntLit.
        assert_eq!(
            tokens("X 42"),
            vec![Token::Ident("X".to_string()), Token::IntLit(42), Token::Eof]
        );
    }

    #[test]
    fn test_intlit_zero() {
        assert_eq!(
            tokens("X 0"),
            vec![Token::Ident("X".to_string()), Token::IntLit(0), Token::Eof]
        );
    }

    // --- float literal ---

    #[test]
    fn test_floatlit_simple() {
        assert_eq!(
            tokens("X 1.25"),
            vec![
                Token::Ident("X".to_string()),
                Token::FloatLit(1.25),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_floatlit_exponent() {
        let toks = tokens("X 1.5e2");
        assert_eq!(toks[1], Token::FloatLit(150.0));
    }

    #[test]
    fn test_floatlit_exponent_negative() {
        let toks = tokens("X 1.0e-1");
        if let Token::FloatLit(f) = &toks[1] {
            assert!((f - 0.1).abs() < 1e-10);
        } else {
            panic!("expected FloatLit, got {:?}", toks[1]);
        }
    }

    // --- string literal ---

    #[test]
    fn test_string_simple() {
        assert_eq!(
            tokens(r#""Hello""#),
            vec![Token::StringLit("Hello".to_string()), Token::Eof]
        );
    }

    #[test]
    fn test_string_escape_newline() {
        assert_eq!(
            tokens(r#""\n""#),
            vec![Token::StringLit("\n".to_string()), Token::Eof]
        );
    }

    #[test]
    fn test_string_escape_tab() {
        assert_eq!(
            tokens(r#""\t""#),
            vec![Token::StringLit("\t".to_string()), Token::Eof]
        );
    }

    #[test]
    fn test_string_escape_backslash() {
        assert_eq!(
            tokens(r#""\\""#),
            vec![Token::StringLit("\\".to_string()), Token::Eof]
        );
    }

    #[test]
    fn test_string_escape_quote() {
        assert_eq!(
            tokens(r#""\"""#),
            vec![Token::StringLit("\"".to_string()), Token::Eof]
        );
    }

    #[test]
    fn test_string_unterminated() {
        let toks = tokens("\"Hello");
        assert!(matches!(toks[0], Token::Error(_)));
    }

    // --- operators ---

    #[test]
    fn test_op_plus() {
        assert_eq!(tokens("+"), vec![Token::Op("+".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_lt() {
        assert_eq!(tokens("<"), vec![Token::Op("<".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_le() {
        assert_eq!(tokens("<="), vec![Token::Op("<=".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_neq() {
        assert_eq!(tokens("<>"), vec![Token::Op("<>".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_ge() {
        assert_eq!(tokens(">="), vec![Token::Op(">=".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_logical_and() {
        assert_eq!(tokens("&&"), vec![Token::Op("&&".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_logical_or() {
        assert_eq!(tokens("||"), vec![Token::Op("||".to_string()), Token::Eof]);
    }

    #[test]
    fn test_op_bitwise_or() {
        assert_eq!(tokens("|"), vec![Token::Op("|".to_string()), Token::Eof]);
    }

    // --- & vs && ---

    #[test]
    fn test_ampersand_single() {
        assert_eq!(tokens("&"), vec![Token::Ampersand, Token::Eof]);
    }

    #[test]
    fn test_ampersand_double_is_logical_and() {
        assert_eq!(tokens("&&"), vec![Token::Op("&&".to_string()), Token::Eof]);
    }

    #[test]
    fn test_ampersand_before_ident() {
        assert_eq!(
            tokens("&I"),
            vec![Token::Ampersand, Token::Ident("I".to_string()), Token::Eof]
        );
    }

    // --- comma, semicolon, parens ---

    #[test]
    fn test_comma() {
        assert_eq!(tokens(","), vec![Token::Comma, Token::Eof]);
    }

    #[test]
    fn test_semicolon() {
        assert_eq!(tokens(";"), vec![Token::Semicolon, Token::Eof]);
    }

    #[test]
    fn test_parens() {
        assert_eq!(tokens("()"), vec![Token::LParen, Token::RParen, Token::Eof]);
    }

    // --- LineNum vs IntLit ---

    #[test]
    fn test_linenum_at_line_start() {
        assert_eq!(
            tokens("10 PUTDEC 42"),
            vec![
                Token::LineNum(10),
                Token::Ident("PUTDEC".to_string()),
                Token::IntLit(42),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_intlit_not_at_line_start() {
        // The first token on the line is an identifier, so the number after it is IntLit.
        assert_eq!(
            tokens("PUTDEC 42"),
            vec![
                Token::Ident("PUTDEC".to_string()),
                Token::IntLit(42),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_linenum_at_second_line() {
        // After a Newline, at_line_start resets, so `99` on the second line is also LineNum.
        let toks = tokens("PUTDEC 1\n99 PUTDEC 2");
        assert_eq!(
            toks,
            vec![
                Token::Ident("PUTDEC".to_string()),
                Token::IntLit(1),
                Token::Newline,
                Token::LineNum(99),
                Token::Ident("PUTDEC".to_string()),
                Token::IntLit(2),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_linenum_with_leading_spaces() {
        // Leading spaces do not break at_line_start.
        let toks = tokens("  10  PUTDEC");
        assert_eq!(
            toks,
            vec![
                Token::LineNum(10),
                Token::Ident("PUTDEC".to_string()),
                Token::Eof,
            ]
        );
    }

    // --- REM ---

    #[test]
    fn test_rem_skips_to_newline() {
        let toks = tokens("REM this is a comment\nPUTDEC 1");
        assert_eq!(
            toks,
            vec![
                Token::Ident("REM".to_string()),
                Token::Newline,
                Token::Ident("PUTDEC".to_string()),
                Token::IntLit(1),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_rem_at_end_of_input() {
        let toks = tokens("REM no newline at end");
        // REM is emitted, then next_token_inner skips rest and emits Newline, then Eof.
        assert_eq!(
            toks,
            vec![Token::Ident("REM".to_string()), Token::Newline, Token::Eof,]
        );
    }

    #[test]
    fn test_rem_with_linenum_before() {
        let toks = tokens("10 REM a comment");
        assert_eq!(
            toks,
            vec![
                Token::LineNum(10),
                Token::Ident("REM".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    // --- newline ---

    #[test]
    fn test_newline_lf() {
        assert_eq!(
            tokens("A\nB"),
            vec![
                Token::Ident("A".to_string()),
                Token::Newline,
                Token::Ident("B".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_newline_crlf_counts_as_one() {
        assert_eq!(
            tokens("A\r\nB"),
            vec![
                Token::Ident("A".to_string()),
                Token::Newline,
                Token::Ident("B".to_string()),
                Token::Eof,
            ]
        );
    }

    // --- EOF ---

    #[test]
    fn test_empty_input() {
        assert_eq!(tokens(""), vec![Token::Eof]);
    }

    #[test]
    fn test_whitespace_only() {
        assert_eq!(tokens("   "), vec![Token::Eof]);
    }

    // --- position tracking ---

    #[test]
    fn test_position_first_token() {
        let st = spanned("PUTDEC");
        assert_eq!(st[0].pos, Position::new(1, 1));
    }

    #[test]
    fn test_position_after_space() {
        let st = spanned("A B");
        assert_eq!(st[1].pos, Position::new(1, 3));
    }

    #[test]
    fn test_position_second_line() {
        let st = spanned("A\nB");
        // 'B' is on line 2, col 1.
        assert_eq!(st[2].pos, Position::new(2, 1));
    }

    // --- source_offset and source_len ---

    #[test]
    fn test_source_offset_first_token() {
        let st = spanned("HELLO");
        assert_eq!(st[0].source_offset, 0);
        assert_eq!(st[0].source_len, 5);
    }

    #[test]
    fn test_source_offset_after_space() {
        let st = spanned("A B");
        assert_eq!(st[1].source_offset, 2);
        assert_eq!(st[1].source_len, 1);
    }

    #[test]
    fn test_source_offset_string_includes_quotes() {
        let st = spanned(r#""hi""#);
        // Raw text is `"hi"` = 4 bytes.
        assert_eq!(st[0].source_len, 4);
    }

    // --- peek_token idempotence ---

    #[test]
    fn test_peek_idempotent() {
        let mut lex = Lexer::new("ABC");
        let p1 = lex.peek_token().token.clone();
        let p2 = lex.peek_token().token.clone();
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_peek_then_next_same_token() {
        let mut lex = Lexer::new("ABC");
        let p = lex.peek_token().token.clone();
        let n = lex.next_token().token;
        assert_eq!(p, n);
    }

    // --- complex statement ---

    #[test]
    fn test_full_statement_putdec() {
        assert_eq!(
            tokens("PUTDEC 1 + 2"),
            vec![
                Token::Ident("PUTDEC".to_string()),
                Token::IntLit(1),
                Token::Op("+".to_string()),
                Token::IntLit(2),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_multi_arg_statement() {
        assert_eq!(
            tokens("PLOT X, Y, COLOR"),
            vec![
                Token::Ident("PLOT".to_string()),
                Token::Ident("X".to_string()),
                Token::Comma,
                Token::Ident("Y".to_string()),
                Token::Comma,
                Token::Ident("COLOR".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_reference_operator() {
        assert_eq!(
            tokens("LET &I, 42"),
            vec![
                Token::Ident("LET".to_string()),
                Token::Ampersand,
                Token::Ident("I".to_string()),
                Token::Comma,
                Token::IntLit(42),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_function_call_syntax() {
        assert_eq!(
            tokens("MYFUNC(1, 2)"),
            vec![
                Token::Ident("MYFUNC".to_string()),
                Token::LParen,
                Token::IntLit(1),
                Token::Comma,
                Token::IntLit(2),
                Token::RParen,
                Token::Eof,
            ]
        );
    }

    // --- integer overflow → Error ---

    #[test]
    fn test_intlit_overflow_yields_error() {
        let toks = tokens("X 99999999999999999999");
        assert!(
            matches!(toks[1], Token::Error(_)),
            "expected Error, got {:?}",
            toks[1]
        );
    }

    #[test]
    fn test_linenum_overflow_yields_error() {
        let toks = tokens("99999999999999999999 PRINT");
        assert!(
            matches!(toks[0], Token::Error(_)),
            "expected Error, got {:?}",
            toks[0]
        );
    }

    // --- float at line-start → Error ---

    #[test]
    fn test_float_at_line_start_yields_error() {
        let toks = tokens("1.25 X");
        assert!(
            matches!(toks[0], Token::Error(_)),
            "expected Error, got {:?}",
            toks[0]
        );
    }
}
