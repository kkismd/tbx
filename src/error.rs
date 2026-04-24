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
    /// An array index was out of the valid range `[0, size)`.
    ArrayIndexOutOfBounds {
        index: i64,
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
    /// The return stack depth exceeded the maximum allowed limit.
    ReturnStackOverflow {
        depth: usize,
        limit: usize,
    },
    /// The data stack depth exceeded the maximum allowed limit.
    DataStackOverflow {
        depth: usize,
        limit: usize,
    },
    /// An integer arithmetic operation produced a result outside the `i64` range.
    IntegerOverflow,
    /// A jump target address is negative and therefore invalid.
    InvalidJumpTarget {
        address: i64,
    },
    /// A symbol name was not found in the dictionary.
    UndefinedSymbol {
        name: String,
    },
    /// An expression is syntactically invalid.
    ///
    /// `reason` describes the specific error (e.g. mismatched parentheses,
    /// unknown operator).
    InvalidExpression {
        reason: &'static str,
    },
    /// A GOTO/BIF/BIT referenced a label that was never defined in the current word.
    UndefinedLabel {
        label: i64,
    },
    /// The same line-number label was defined more than once in the same word.
    DuplicateLabel {
        label: i64,
    },
    /// An operand value is outside the valid range.
    ///
    /// The operand has the correct type but its value is not acceptable
    /// (e.g., a negative arity or local_count in a CALL instruction).
    InvalidOperand {
        name: &'static str,
        value: i64,
        reason: &'static str,
    },
    /// The token stream is empty or has not been set on the VM.
    ///
    /// Returned by `VM::next_token()` when `token_stream` is `None` or the
    /// `VecDeque` has been fully consumed.
    TokenStreamEmpty,
    /// compile_stack has leftover items when END is executed.
    ///
    /// Word definition is incomplete — some compile-time values were not consumed.
    CompileStackNotEmpty {
        count: usize,
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
            TbxError::IndexOutOfBounds { index, size } => {
                write!(f, "index out of bounds: index {}, size {}", index, size)
            }
            TbxError::ArrayIndexOutOfBounds { index, size } => {
                write!(f, "array index out of bounds: index {index}, size {size}")
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
            TbxError::ReturnStackOverflow { depth, limit } => {
                write!(
                    f,
                    "return stack overflow: depth {} reached or exceeded limit {}",
                    depth, limit
                )
            }
            TbxError::DataStackOverflow { depth, limit } => {
                write!(
                    f,
                    "data stack overflow: depth {} reached or exceeded limit {}",
                    depth, limit
                )
            }
            TbxError::IntegerOverflow => write!(f, "integer overflow"),
            TbxError::InvalidJumpTarget { address } => {
                write!(f, "invalid jump target: negative address {address}")
            }
            TbxError::UndefinedSymbol { name } => write!(f, "undefined symbol: '{name}'"),
            TbxError::InvalidExpression { reason } => {
                write!(f, "invalid expression: {reason}")
            }
            TbxError::UndefinedLabel { label } => write!(f, "undefined label: {label}"),
            TbxError::DuplicateLabel { label } => write!(f, "duplicate label: {label}"),
            TbxError::InvalidOperand {
                name,
                value,
                reason,
            } => {
                write!(f, "invalid operand '{name}' (value: {value}): {reason}")
            }
            TbxError::TokenStreamEmpty => write!(f, "token stream is empty or not set"),
            TbxError::CompileStackNotEmpty { count } => {
                write!(
                    f,
                    "compile stack has {count} unpatched item(s) at END; word definition is incomplete"
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

    #[test]
    fn test_marker_not_found_display() {
        let e = TbxError::MarkerNotFound;
        assert!(e.to_string().contains("marker"));
    }

    #[test]
    fn test_integer_overflow_display() {
        let e = TbxError::IntegerOverflow;
        assert!(e.to_string().contains("integer overflow"));
    }

    #[test]
    fn test_invalid_jump_target_display() {
        let e = TbxError::InvalidJumpTarget { address: -7 };
        let msg = e.to_string();
        assert!(msg.contains("-7"));
        assert!(msg.contains("negative"));
    }

    #[test]
    fn test_invalid_operand_display() {
        let e = TbxError::InvalidOperand {
            name: "arity",
            value: -1,
            reason: "must be non-negative",
        };
        let msg = e.to_string();
        assert!(msg.contains("invalid operand"));
        assert!(msg.contains("arity"));
        assert!(msg.contains("-1"));
        assert!(msg.contains("must be non-negative"));
    }

    #[test]
    fn test_token_stream_empty_display() {
        let e = TbxError::TokenStreamEmpty;
        assert!(e.to_string().contains("token stream"));
    }
}
