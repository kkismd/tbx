use crate::cell::Cell;

/// Function pointer type for native Rust primitives.
pub type PrimFn = fn(&mut crate::vm::VM);

/// How a dictionary entry is executed or accessed.
#[derive(Clone)]
pub enum EntryKind {
    /// Compiled TBX word — usize is the start offset into `dictionary: Vec<Cell>`
    Word(usize),
    /// Native Rust primitive — holds a function pointer
    Primitive(PrimFn),
    /// Global variable — usize is the index of the storage cell in `dictionary`
    Variable(usize),
    /// Constant — value stored directly in this entry
    Constant(Cell),
}

impl std::fmt::Debug for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryKind::Word(offset) => write!(f, "Word({offset})"),
            EntryKind::Primitive(ptr) => write!(f, "Primitive({ptr:p})"),
            EntryKind::Variable(idx) => write!(f, "Variable({idx})"),
            EntryKind::Constant(cell) => write!(f, "Constant({cell:?})"),
        }
    }
}

/// Flag bit: word executes immediately even in compile mode
pub const FLAG_IMMEDIATE: u8 = 0b0000_0001;

/// A single entry in the TBX word header table.
///
/// The header table (`VM::headers`) and the flat code array (`VM::dictionary`)
/// are kept separate. Each `WordEntry` stores how to execute or access the word
/// via `EntryKind`, and links to the previous entry for dictionary search.
#[derive(Debug, Clone)]
pub struct WordEntry {
    /// The name of the word as it appears in source code
    pub name: String,
    /// Attribute flags (e.g. IMMEDIATE)
    pub flags: u8,
    /// How this entry is executed or accessed
    pub kind: EntryKind,
    /// Index of the previous entry in `VM::headers` (linked list for search).
    ///
    /// **Do not set this field directly.** It is automatically managed by
    /// `VM::register()`, which overwrites any value set here.
    pub(crate) prev: Option<usize>,
}

impl WordEntry {
    /// Create a new primitive word entry backed by a Rust function.
    pub fn new_primitive(name: &str, f: PrimFn) -> Self {
        Self {
            name: name.to_string(),
            flags: 0,
            kind: EntryKind::Primitive(f),
            prev: None,
        }
    }

    /// Create a new compiled word entry with a given start offset in `dictionary`.
    pub fn new_word(name: &str, offset: usize) -> Self {
        Self {
            name: name.to_string(),
            flags: 0,
            kind: EntryKind::Word(offset),
            prev: None,
        }
    }

    /// Create a new global variable entry with a given storage index in `dictionary`.
    pub fn new_variable(name: &str, idx: usize) -> Self {
        Self {
            name: name.to_string(),
            flags: 0,
            kind: EntryKind::Variable(idx),
            prev: None,
        }
    }

    /// Create a new constant entry.
    pub fn new_constant(name: &str, value: Cell) -> Self {
        Self {
            name: name.to_string(),
            flags: 0,
            kind: EntryKind::Constant(value),
            prev: None,
        }
    }

    /// Returns true if the IMMEDIATE flag is set.
    pub fn is_immediate(&self) -> bool {
        self.flags & FLAG_IMMEDIATE != 0
    }
}
