/// Execution token: a type-safe index into `VM::headers` (the word header table).
///
/// Distinct from an index into `VM::dictionary` (the flat code/data array).
/// `lookup()` returns `Option<Xt>`; `register()` returns `Xt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xt(pub usize);

impl Xt {
    /// Returns the raw index into `VM::headers`.
    pub fn index(self) -> usize {
        self.0
    }
}

/// Cell is the fundamental value type of the TBX VM.
/// It represents all values that can exist on the stack or in the dictionary.
#[derive(Debug, Clone)]
pub enum Cell {
    /// Signed 64-bit integer
    Int(i64),
    /// 64-bit floating point
    Float(f64),
    /// Memory address (result of address-of operator &)
    Addr(usize),
    /// Execution token — type-safe index into `VM::headers` (word header array), not into `dictionary`
    Xt(Xt),
    /// Boolean value for logical/comparison operations
    Bool(bool),
    /// Empty / null value
    None,
    /// Reserved for future array support
    Array,
    /// Index into the string pool (length-prefixed)
    StringDesc(usize),
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cell::Int(n) => write!(f, "{}", n),
            Cell::Float(v) => write!(f, "{}", v),
            Cell::Addr(a) => write!(f, "addr:{}", a),
            Cell::Xt(x) => write!(f, "xt:{}", x.0),
            Cell::Bool(b) => write!(f, "{}", b),
            Cell::None => write!(f, "<none>"),
            Cell::Array => write!(f, "<array>"),
            Cell::StringDesc(i) => write!(f, "str:{}", i),
        }
    }
}

impl Cell {
    /// Returns the `i64` value if this cell is `Int`, otherwise `None`.
    pub fn as_int(&self) -> Option<i64> {
        if let Cell::Int(n) = self {
            Some(*n)
        } else {
            None
        }
    }

    /// Returns the `f64` value if this cell is `Float`, otherwise `None`.
    pub fn as_float(&self) -> Option<f64> {
        if let Cell::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Returns the `bool` value if this cell is `Bool`, otherwise `None`.
    pub fn as_bool(&self) -> Option<bool> {
        if let Cell::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    /// Returns the `usize` address if this cell is `Addr`, otherwise `None`.
    pub fn as_addr(&self) -> Option<usize> {
        if let Cell::Addr(a) = self {
            Some(*a)
        } else {
            None
        }
    }

    /// Returns the `Xt` value if this cell is `Xt`, otherwise `None`.
    pub fn as_xt(&self) -> Option<Xt> {
        if let Cell::Xt(x) = self {
            Some(*x)
        } else {
            None
        }
    }

    /// Returns the string pool index if this cell is `StringDesc`, otherwise `None`.
    pub fn as_string_desc(&self) -> Option<usize> {
        if let Cell::StringDesc(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    /// Returns a static string naming the variant. Useful for error messages and debugging.
    pub fn type_name(&self) -> &'static str {
        match self {
            Cell::Int(_) => "Int",
            Cell::Float(_) => "Float",
            Cell::Addr(_) => "Addr",
            Cell::Xt(_) => "Xt",
            Cell::Bool(_) => "Bool",
            Cell::None => "None",
            Cell::Array => "Array",
            Cell::StringDesc(_) => "StringDesc",
        }
    }

    /// Evaluates the cell as a boolean condition.
    ///
    /// - `Bool(true)` → `true`
    /// - `Bool(false)` → `false`
    /// - `Int(0)` → `false`
    /// - any other `Int` → `true`
    /// - all other variants → `false`
    pub fn is_truthy(&self) -> bool {
        match self {
            Cell::Bool(b) => *b,
            Cell::Int(n) => *n != 0,
            _ => false,
        }
    }
}

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // For floats, treat NaN as equal to NaN (value identity semantics).
            // IEEE 754 defines NaN != NaN, but for TBX value equality we use
            // structural equality so that NaN cells compare as equal.
            (Cell::Float(a), Cell::Float(b)) => (a.is_nan() && b.is_nan()) || (a == b),
            (Cell::Int(a), Cell::Int(b)) => a == b,
            (Cell::Addr(a), Cell::Addr(b)) => a == b,
            (Cell::Xt(a), Cell::Xt(b)) => a == b,
            (Cell::Bool(a), Cell::Bool(b)) => a == b,
            (Cell::None, Cell::None) => true,
            (Cell::Array, Cell::Array) => true,
            (Cell::StringDesc(a), Cell::StringDesc(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_nan_equals_nan() {
        // TBX uses value identity: NaN should equal NaN
        assert_eq!(Cell::Float(f64::NAN), Cell::Float(f64::NAN));
    }

    #[test]
    fn test_float_nan_not_equal_to_non_nan() {
        assert_ne!(Cell::Float(f64::NAN), Cell::Float(1.0));
        assert_ne!(Cell::Float(1.0), Cell::Float(f64::NAN));
    }

    #[test]
    fn test_float_normal_equality() {
        assert_eq!(Cell::Float(1.0), Cell::Float(1.0));
        assert_ne!(Cell::Float(1.0), Cell::Float(2.0));
    }

    #[test]
    fn test_variant_mismatch_not_equal() {
        assert_ne!(Cell::Int(1), Cell::Float(1.0));
        assert_ne!(Cell::Bool(true), Cell::Int(1));
    }

    // --- Display ---

    #[test]
    fn test_display_int() {
        assert_eq!(Cell::Int(42).to_string(), "42");
        assert_eq!(Cell::Int(-1).to_string(), "-1");
    }

    #[test]
    fn test_display_float() {
        assert_eq!(Cell::Float(3.14).to_string(), "3.14");
    }

    #[test]
    fn test_display_addr() {
        assert_eq!(Cell::Addr(1234).to_string(), "addr:1234");
    }

    #[test]
    fn test_display_xt() {
        assert_eq!(Cell::Xt(Xt(5)).to_string(), "xt:5");
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(Cell::Bool(true).to_string(), "true");
        assert_eq!(Cell::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_display_none() {
        assert_eq!(Cell::None.to_string(), "<none>");
    }

    #[test]
    fn test_display_array() {
        assert_eq!(Cell::Array.to_string(), "<array>");
    }

    #[test]
    fn test_display_string_desc() {
        assert_eq!(Cell::StringDesc(0).to_string(), "str:0");
    }

    // --- Type conversion methods ---

    #[test]
    fn test_as_int() {
        assert_eq!(Cell::Int(7).as_int(), Some(7));
        assert_eq!(Cell::Float(1.0).as_int(), None);
    }

    #[test]
    fn test_as_float() {
        assert_eq!(Cell::Float(2.5).as_float(), Some(2.5));
        assert_eq!(Cell::Int(1).as_float(), None);
    }

    #[test]
    fn test_as_bool() {
        assert_eq!(Cell::Bool(true).as_bool(), Some(true));
        assert_eq!(Cell::Bool(false).as_bool(), Some(false));
        assert_eq!(Cell::Int(1).as_bool(), None);
    }

    #[test]
    fn test_as_addr() {
        assert_eq!(Cell::Addr(100).as_addr(), Some(100));
        assert_eq!(Cell::Int(100).as_addr(), None);
    }

    #[test]
    fn test_as_xt() {
        assert_eq!(Cell::Xt(Xt(3)).as_xt(), Some(Xt(3)));
        assert_eq!(Cell::Int(3).as_xt(), None);
    }

    #[test]
    fn test_as_string_desc() {
        assert_eq!(Cell::StringDesc(2).as_string_desc(), Some(2));
        assert_eq!(Cell::Int(2).as_string_desc(), None);
    }

    #[test]
    fn test_type_name() {
        assert_eq!(Cell::Int(0).type_name(), "Int");
        assert_eq!(Cell::Float(0.0).type_name(), "Float");
        assert_eq!(Cell::Addr(0).type_name(), "Addr");
        assert_eq!(Cell::Xt(Xt(0)).type_name(), "Xt");
        assert_eq!(Cell::Bool(false).type_name(), "Bool");
        assert_eq!(Cell::None.type_name(), "None");
        assert_eq!(Cell::Array.type_name(), "Array");
        assert_eq!(Cell::StringDesc(0).type_name(), "StringDesc");
    }

    #[test]
    fn test_is_truthy() {
        assert!(Cell::Bool(true).is_truthy());
        assert!(!Cell::Bool(false).is_truthy());
        assert!(Cell::Int(1).is_truthy());
        assert!(Cell::Int(-1).is_truthy());
        assert!(!Cell::Int(0).is_truthy());
        // non-Int/Bool variants are falsy
        assert!(!Cell::Float(1.0).is_truthy());
        assert!(!Cell::None.is_truthy());
        assert!(!Cell::Array.is_truthy());
        assert!(!Cell::Addr(1).is_truthy());
    }
}
