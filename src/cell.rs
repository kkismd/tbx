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
    /// Execution token — index into the dictionary
    Xt(usize),
    /// Boolean value for logical/comparison operations
    Bool(bool),
    /// Empty / null value
    None,
    /// Reserved for future array support
    Array,
    /// Index into the string pool (length-prefixed)
    StringDesc(usize),
}
