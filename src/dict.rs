use crate::cell::{Cell, Xt};

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
    pub(crate) prev: Option<Xt>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    /// A dummy primitive function used to obtain a concrete `PrimFn` value in tests.
    fn dummy_prim(_vm: &mut crate::vm::VM) {}

    // --- FLAG_IMMEDIATE constant ---

    #[test]
    fn test_flag_immediate_value() {
        // FLAG_IMMEDIATE must be exactly bit 0 (0b0000_0001).
        assert_eq!(FLAG_IMMEDIATE, 0b0000_0001);
    }

    // --- new_primitive ---

    #[test]
    fn test_new_primitive_name() {
        let entry = WordEntry::new_primitive("dup", dummy_prim);
        assert_eq!(entry.name, "dup");
    }

    #[test]
    fn test_new_primitive_flags_zero() {
        let entry = WordEntry::new_primitive("dup", dummy_prim);
        assert_eq!(entry.flags, 0);
    }

    #[test]
    fn test_new_primitive_prev_none() {
        let entry = WordEntry::new_primitive("dup", dummy_prim);
        assert!(entry.prev.is_none());
    }

    #[test]
    fn test_new_primitive_kind() {
        let entry = WordEntry::new_primitive("dup", dummy_prim);
        // Verify both the variant and that the stored function pointer is dummy_prim.
        // Use fn_addr_eq to avoid the unpredictable_function_pointer_comparisons lint.
        let EntryKind::Primitive(stored_fn) = entry.kind else {
            panic!("expected EntryKind::Primitive");
        };
        assert!(std::ptr::fn_addr_eq(stored_fn, dummy_prim as PrimFn));
    }

    // --- new_word ---

    #[test]
    fn test_new_word_name() {
        let entry = WordEntry::new_word("square", 42);
        assert_eq!(entry.name, "square");
    }

    #[test]
    fn test_new_word_flags_zero() {
        let entry = WordEntry::new_word("square", 42);
        assert_eq!(entry.flags, 0);
    }

    #[test]
    fn test_new_word_prev_none() {
        let entry = WordEntry::new_word("square", 42);
        assert!(entry.prev.is_none());
    }

    #[test]
    fn test_new_word_kind_offset() {
        let entry = WordEntry::new_word("square", 42);
        assert!(matches!(entry.kind, EntryKind::Word(42)));
    }

    // --- new_variable ---

    #[test]
    fn test_new_variable_name() {
        let entry = WordEntry::new_variable("counter", 10);
        assert_eq!(entry.name, "counter");
    }

    #[test]
    fn test_new_variable_flags_zero() {
        let entry = WordEntry::new_variable("counter", 10);
        assert_eq!(entry.flags, 0);
    }

    #[test]
    fn test_new_variable_prev_none() {
        let entry = WordEntry::new_variable("counter", 10);
        assert!(entry.prev.is_none());
    }

    #[test]
    fn test_new_variable_kind_idx() {
        let entry = WordEntry::new_variable("counter", 10);
        assert!(matches!(entry.kind, EntryKind::Variable(10)));
    }

    // --- new_constant ---

    #[test]
    fn test_new_constant_name() {
        let entry = WordEntry::new_constant("MAX", Cell::Int(100));
        assert_eq!(entry.name, "MAX");
    }

    #[test]
    fn test_new_constant_flags_zero() {
        let entry = WordEntry::new_constant("MAX", Cell::Int(100));
        assert_eq!(entry.flags, 0);
    }

    #[test]
    fn test_new_constant_prev_none() {
        let entry = WordEntry::new_constant("MAX", Cell::Int(100));
        assert!(entry.prev.is_none());
    }

    #[test]
    fn test_new_constant_kind_value() {
        let entry = WordEntry::new_constant("MAX", Cell::Int(100));
        assert!(matches!(entry.kind, EntryKind::Constant(Cell::Int(100))));
    }

    // --- is_immediate ---

    #[test]
    fn test_is_immediate_false_by_default() {
        // All constructors set flags = 0, so is_immediate() must return false.
        let entry = WordEntry::new_primitive("dup", dummy_prim);
        assert!(!entry.is_immediate());
    }

    #[test]
    fn test_is_immediate_true_when_flag_set() {
        let mut entry = WordEntry::new_primitive("if", dummy_prim);
        entry.flags |= FLAG_IMMEDIATE;
        assert!(entry.is_immediate());
    }

    #[test]
    fn test_is_immediate_ignores_other_bits() {
        // Setting bits other than FLAG_IMMEDIATE must not affect is_immediate().
        let mut entry = WordEntry::new_primitive("dup", dummy_prim);
        entry.flags = 0b1111_1110; // all bits except bit 0
        assert!(!entry.is_immediate());
    }

    #[test]
    fn test_is_immediate_true_with_other_bits_also_set() {
        // is_immediate() should return true whenever FLAG_IMMEDIATE bit is set,
        // regardless of other bits.
        let mut entry = WordEntry::new_primitive("if", dummy_prim);
        entry.flags = 0b1111_1111; // all bits including bit 0
        assert!(entry.is_immediate());
    }
}
