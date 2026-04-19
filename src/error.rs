/// Top-level error type for the TBX VM.
#[derive(Debug, Clone, PartialEq)]
pub enum TbxError {
    /// A string was too long to store in the string pool.
    ///
    /// The pool uses a two-byte little-endian length prefix (`u16`), so strings
    /// must be at most 65535 bytes when encoded as UTF-8. This limit applies at
    /// the lexer/parser level before the string reaches the pool.
    StringTooLong {
        len: usize,
    },
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
    IndexOutOfBounds {
        index: usize,
        size: usize,
    },
    DivisionByZero,
    /// The dictionary pointer exceeded the maximum allowed size.
    DictionaryOverflow {
        requested: usize,
        limit: usize,
    },
    /// ALLOT was called with a negative count.
    InvalidAllotCount,
    /// HALT was executed, requesting the VM to stop.
    Halted,
    /// RETURN with a value was executed at the top level (outside any word definition).
    InvalidReturn,
    /// DROP_TO_MARKER executed but no Cell::Marker was found on the data stack.
    MarkerNotFound,
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
            TbxError::IndexOutOfBounds { index, size } => {
                write!(f, "index out of bounds: index {}, size {}", index, size)
            }
            TbxError::DivisionByZero => write!(f, "division by zero"),
            TbxError::DictionaryOverflow { requested, limit } => {
                write!(
                    f,
                    "dictionary overflow: requested {} cells (limit {})",
                    requested, limit
                )
            }
            TbxError::InvalidAllotCount => write!(f, "ALLOT count must be non-negative"),
            TbxError::Halted => write!(f, "execution halted"),
            TbxError::InvalidReturn => write!(f, "RETURN with value at top level is not allowed"),
            TbxError::MarkerNotFound => {
                write!(f, "DROP_TO_MARKER: no marker found on the data stack")
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

    #[test]
    fn test_division_by_zero_display() {
        let e = TbxError::DivisionByZero;
        assert!(e.to_string().contains("division by zero"));
    }
}
