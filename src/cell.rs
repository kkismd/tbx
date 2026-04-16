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
#[derive(Debug, Clone, PartialEq)]
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
