use crate::cell::Cell;

/// A single entry in the TBX dictionary.
/// Represents one word — either a primitive (backed by a Rust function)
/// or a compiled word (a sequence of Xts).
#[derive(Debug)]
pub struct WordEntry {
    /// The name of the word as it appears in source code
    pub name: String,
    /// Attribute flags (e.g. IMMEDIATE)
    pub flags: u8,
    /// The compiled body of the word as a sequence of Cells
    pub code: Vec<Cell>,
    /// If true, this word is implemented in Rust (not in TBX bytecode)
    pub is_primitive: bool,
}

/// Flag bit: word executes immediately even in compile mode
pub const FLAG_IMMEDIATE: u8 = 0b0000_0001;

impl WordEntry {
    /// Create a new primitive word entry (no compiled body).
    pub fn new_primitive(name: &str) -> Self {
        Self {
            name: name.to_string(),
            flags: 0,
            code: Vec::new(),
            is_primitive: true,
        }
    }

    /// Create a new compiled word entry with an empty body.
    pub fn new_compiled(name: &str) -> Self {
        Self {
            name: name.to_string(),
            flags: 0,
            code: Vec::new(),
            is_primitive: false,
        }
    }

    /// Returns true if the IMMEDIATE flag is set.
    pub fn is_immediate(&self) -> bool {
        self.flags & FLAG_IMMEDIATE != 0
    }
}
