/// Top-level error type for the TBX VM.
#[derive(Debug, Clone, PartialEq)]
pub enum TbxError {
    /// A string was too long to store in the string pool.
    ///
    /// The pool uses a two-byte little-endian length prefix (`u16`), so strings
    /// must be at most 65535 bytes when encoded as UTF-8. This limit applies at
    /// the lexer/parser level before the string reaches the pool.
    StringTooLong { len: usize },
    /// A pop was attempted on an empty data stack.
    StackUnderflow,
    /// A value of the wrong type was provided.
    ///
    /// `expected` describes the type(s) the operation accepts;
    /// `got` describes the type that was actually on the stack.
    TypeError {
        expected: &'static str,
        got: &'static str,
    },
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
            TbxError::StackUnderflow => write!(f, "stack underflow"),
            TbxError::TypeError { expected, got } => {
                write!(f, "type error: expected {}, got {}", expected, got)
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

    #[test]
    fn test_stack_underflow_display() {
        let e = TbxError::StackUnderflow;
        assert!(e.to_string().contains("stack underflow"));
    }

    #[test]
    fn test_stack_underflow_debug() {
        let e = TbxError::StackUnderflow;
        assert!(format!("{:?}", e).contains("StackUnderflow"));
    }

    #[test]
    fn test_type_error_display() {
        let e = TbxError::TypeError {
            expected: "address",
            got: "Int",
        };
        let msg = e.to_string();
        assert!(msg.contains("address"));
        assert!(msg.contains("Int"));
    }
}
