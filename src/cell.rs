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
}
