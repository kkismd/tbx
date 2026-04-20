use crate::vm::VM;

/// The TBX outer interpreter.
///
/// Wraps a [`VM`] and provides tokenization and word-lookup dispatch.
/// Phase 1 supports immediate execution mode only; compile mode (DEF/END) is Phase 2.
pub struct Interpreter {
    vm: VM,
}

impl Interpreter {
    /// Create a new `Interpreter` wrapping the given VM.
    pub fn new(vm: VM) -> Self {
        Self { vm }
    }

    /// Consume the `Interpreter` and return the inner VM.
    pub fn into_vm(self) -> VM {
        self.vm
    }
}

/// Extract the next token from `source` starting at byte offset `*pos`.
///
/// Advances `*pos` past the returned token. Whitespace (space, tab),
/// newlines (`\n`, `\r`), and semicolons (`;`) are treated as token
/// separators and are skipped before and between tokens.
///
/// Returns `None` when the end of input is reached.
pub fn next_token(source: &str, pos: &mut usize) -> Option<String> {
    let bytes = source.as_bytes();
    let len = bytes.len();

    // skip separators
    while *pos < len && is_separator(bytes[*pos]) {
        *pos += 1;
    }

    if *pos >= len {
        return None;
    }

    // collect non-separator characters
    let start = *pos;
    while *pos < len && !is_separator(bytes[*pos]) {
        *pos += 1;
    }

    Some(source[start..*pos].to_string())
}

fn is_separator(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b';')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_token_basic() {
        let src = "PUTDEC 42";
        let mut pos = 0;
        assert_eq!(next_token(src, &mut pos), Some("PUTDEC".to_string()));
        assert_eq!(next_token(src, &mut pos), Some("42".to_string()));
        assert_eq!(next_token(src, &mut pos), None);
    }

    #[test]
    fn test_next_token_skip_whitespace() {
        let src = "  FOO  BAR  ";
        let mut pos = 0;
        assert_eq!(next_token(src, &mut pos), Some("FOO".to_string()));
        assert_eq!(next_token(src, &mut pos), Some("BAR".to_string()));
        assert_eq!(next_token(src, &mut pos), None);
    }

    #[test]
    fn test_next_token_eof() {
        let src = "";
        let mut pos = 0;
        assert_eq!(next_token(src, &mut pos), None);
    }

    #[test]
    fn test_next_token_newline_separator() {
        let src = "FOO\nBAR";
        let mut pos = 0;
        assert_eq!(next_token(src, &mut pos), Some("FOO".to_string()));
        assert_eq!(next_token(src, &mut pos), Some("BAR".to_string()));
        assert_eq!(next_token(src, &mut pos), None);
    }

    #[test]
    fn test_next_token_semicolon_separator() {
        let src = "FOO;BAR";
        let mut pos = 0;
        assert_eq!(next_token(src, &mut pos), Some("FOO".to_string()));
        assert_eq!(next_token(src, &mut pos), Some("BAR".to_string()));
        assert_eq!(next_token(src, &mut pos), None);
    }
}
