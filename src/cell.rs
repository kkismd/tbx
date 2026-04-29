/// An entry on the compile-time stack (`VM::compile_stack`).
///
/// `Cell` holds a regular value (address, integer, Xt, …) used by CS_PUSH/CS_POP
/// and related compile-time stack manipulation primitives.
/// `Tag` holds a string label that identifies an open control-structure scope
/// (e.g. `"IF"` or `"WHILE"`); it is pushed by CS_OPEN_TAG and validated/popped
/// by CS_CLOSE_TAG.
/// `CompiledCells` holds a sequence of compiled cells that have been saved for later
/// emission into the dictionary (e.g. the step expression compiled by FOR).
#[derive(Debug, Clone, PartialEq)]
pub enum CompileEntry {
    /// A regular runtime cell stored on the compile stack.
    Cell(Cell),
    /// A string tag that marks an open control-structure scope.
    Tag(String),
    /// A saved sequence of compiled cells to be emitted later, with self-recursive
    /// call-patch offsets relative to the start of the cell sequence.
    CompiledCells(Vec<Cell>, Vec<usize>),
}

impl std::fmt::Display for CompileEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileEntry::Cell(c) => write!(f, "Cell({})", c),
            CompileEntry::Tag(s) => write!(f, "Tag(\"{}\")", s),
            CompileEntry::CompiledCells(cells, _) => {
                write!(f, "CompiledCells(len={})", cells.len())
            }
        }
    }
}

/// Execution token: a type-safe index into `VM::headers` (the word header table).
///
/// Distinct from an index into `VM::dictionary` (the flat code/data array).
/// `lookup()` returns `Option<Xt>`; `register()` returns `Xt`.
///
/// The inner index is intentionally `pub(crate)` to prevent external callers
/// from constructing arbitrary `Xt` values. Valid `Xt` tokens are obtained
/// only through `VM::register()` or `VM::lookup()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xt(pub(crate) usize);

impl Xt {
    /// Returns the raw index into `VM::headers`.
    pub fn index(self) -> usize {
        self.0
    }
}

/// Return frame for the return stack, saving the program counter and base pointer
#[derive(Debug, Clone)]
pub enum ReturnFrame {
    Call { return_pc: usize, saved_bp: usize },
    TopLevel, // Sentinel value for the bottom of the return stack
}

/// Cell is the fundamental value type of the TBX VM.
/// It represents all values that can exist on the stack or in the dictionary.
#[derive(Debug, Clone)]
pub enum Cell {
    /// Signed 64-bit integer
    Int(i64),
    /// 64-bit floating point
    Float(f64),
    /// Address into `VM::dictionary` — used for global variables and heap pointers.
    /// Produced by the address-of operator `&` applied to a global (dictionary-allocated) variable.
    DictAddr(usize),
    /// Address into `VM::data_stack` relative to `VM::bp` — used for local variables.
    /// Produced by the address-of operator `&` applied to a local (stack-frame) variable.
    StackAddr(usize),
    /// Execution token — type-safe index into `VM::headers` (word header array), not into `dictionary`
    Xt(Xt),
    /// Boolean value for logical/comparison operations
    Bool(bool),
    /// Index into the string pool (length-prefixed)
    StringDesc(usize),
    /// Reserved for future array support
    Array,
    None,
    /// Sentinel value placed on the data stack to mark a statement boundary.
    /// Consumed by DROP_TO_MARKER to restore the stack after a statement call.
    Marker,
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cell::Int(n) => write!(f, "{}", n),
            Cell::Float(v) => {
                // Non-finite values (inf, -inf, NaN) are printed as-is.
                // Finite values always include a decimal point to be visually
                // distinct from Int (e.g. 1.0 → "1.0", not "1").
                if v.is_finite() {
                    let s = format!("{v}");
                    if s.contains('.') || s.contains('e') {
                        write!(f, "{s}")
                    } else {
                        write!(f, "{s}.0")
                    }
                } else {
                    write!(f, "{v}")
                }
            }
            Cell::DictAddr(a) => write!(f, "dict:{}", a),
            Cell::StackAddr(a) => write!(f, "stack:{}", a),
            Cell::Xt(x) => write!(f, "xt:{}", x.0),
            Cell::Bool(b) => write!(f, "{}", b),
            Cell::StringDesc(i) => write!(f, "str:{}", i),
            Cell::Array => write!(f, "<array>"),
            Cell::None => write!(f, "<none>"),
            Cell::Marker => write!(f, "<marker>"),
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

    /// Returns the `usize` index if this cell is `DictAddr`, otherwise `None`.
    pub fn as_dict_addr(&self) -> Option<usize> {
        if let Cell::DictAddr(a) = self {
            Some(*a)
        } else {
            None
        }
    }

    /// Returns the `usize` offset if this cell is `StackAddr`, otherwise `None`.
    pub fn as_stack_addr(&self) -> Option<usize> {
        if let Cell::StackAddr(a) = self {
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
            Cell::DictAddr(_) => "DictAddr",
            Cell::StackAddr(_) => "StackAddr",
            Cell::Xt(_) => "Xt",
            Cell::Bool(_) => "Bool",
            Cell::StringDesc(_) => "StringDesc",
            Cell::Array => "Array",
            Cell::None => "None",
            Cell::Marker => "Marker",
        }
    }

    /// Evaluates the cell as a boolean condition.
    ///
    /// - `Bool(true)` → `true`
    /// - `Bool(false)` → `false`
    /// - `Int(0)` → `false`, any other `Int` → `true`
    /// - `Float(0.0)` → `false`, any other `Float` → `true` (NaN is truthy)
    /// - all other variants → `false`
    pub fn is_truthy(&self) -> bool {
        match self {
            Cell::Bool(b) => *b,
            Cell::Int(n) => *n != 0,
            Cell::Float(n) => *n != 0.0,
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
            (Cell::DictAddr(a), Cell::DictAddr(b)) => a == b,
            (Cell::StackAddr(a), Cell::StackAddr(b)) => a == b,
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
        assert_eq!(Cell::Float(3.125).to_string(), "3.125");
        // Integer-valued floats must include a decimal point to be distinct from Int.
        assert_eq!(Cell::Float(1.0).to_string(), "1.0");
        // Non-finite values are printed as-is (no spurious ".0" appended).
        assert_eq!(Cell::Float(f64::INFINITY).to_string(), "inf");
        assert_eq!(Cell::Float(f64::NEG_INFINITY).to_string(), "-inf");
        assert_eq!(Cell::Float(f64::NAN).to_string(), "NaN");
    }

    #[test]
    fn test_display_dict_addr() {
        assert_eq!(Cell::DictAddr(1234).to_string(), "dict:1234");
    }

    #[test]
    fn test_display_stack_addr() {
        assert_eq!(Cell::StackAddr(5).to_string(), "stack:5");
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
    fn test_as_dict_addr() {
        assert_eq!(Cell::DictAddr(100).as_dict_addr(), Some(100));
        assert_eq!(Cell::Int(100).as_dict_addr(), None);
        assert_eq!(Cell::StackAddr(100).as_dict_addr(), None);
    }

    #[test]
    fn test_as_stack_addr() {
        assert_eq!(Cell::StackAddr(8).as_stack_addr(), Some(8));
        assert_eq!(Cell::Int(8).as_stack_addr(), None);
        assert_eq!(Cell::DictAddr(8).as_stack_addr(), None);
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
        assert_eq!(Cell::DictAddr(0).type_name(), "DictAddr");
        assert_eq!(Cell::StackAddr(0).type_name(), "StackAddr");
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
        // Float: non-zero is truthy
        assert!(Cell::Float(1.0).is_truthy());
        assert!(Cell::Float(-1.0).is_truthy());
        assert!(!Cell::Float(0.0).is_truthy());
        // non-Int/Bool/Float variants are falsy
        assert!(!Cell::None.is_truthy());
        assert!(!Cell::Array.is_truthy());
        assert!(!Cell::DictAddr(1).is_truthy());
        assert!(!Cell::StackAddr(1).is_truthy());
    }
}
