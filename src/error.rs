/// Top-level error type for the TBX VM.
#[derive(Debug, Clone, PartialEq)]
pub enum TbxError {
    /// A string was too long to store in the string pool.
    ///
    /// The pool uses a two-byte little-endian length prefix (`u16`), so strings
    /// must be at most 65535 bytes when encoded as UTF-8. This limit applies at
    /// the lexer/parser level before the string reaches the pool.
    StringTooLong { len: usize },
}

impl std::fmt::Display for TbxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TbxError::StringTooLong { len } => {
                write!(
                    f,
                    "string too long for string pool: {} bytes (max 65535)",
                    len
                )
            }
        }
    }
}

impl std::error::Error for TbxError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_too_long_display() {
        let e = TbxError::StringTooLong { len: 300 };
        assert!(e.to_string().contains("300"));
        assert!(e.to_string().contains("65535"));
    }
}
