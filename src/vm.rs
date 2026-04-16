use crate::dict::WordEntry;
use crate::cell::{Cell, Xt};

/// The TBX virtual machine.
///
/// The dictionary is split into two layers:
/// - `headers`: word name/flag/kind metadata, forming a linked list via `prev`
/// - `dictionary`: flat `Vec<Cell>` array of compiled code; `pc` indexes into this
#[derive(Debug)]
pub struct VM {
    /// Word header table (linked list via `WordEntry::prev`)
    pub headers: Vec<WordEntry>,
    /// Flat code/data storage; `pc` is an index into this array
    pub dictionary: Vec<Cell>,
    /// String pool: all string data packed as length-prefixed byte sequences
    pub string_pool: Vec<u8>,
    /// Data stack: operand stack for arithmetic and parameter passing
    pub data_stack: Vec<Cell>,
    /// Return stack: saves (pc, bp) pairs on word calls
    pub return_stack: Vec<(usize, usize)>,
    /// Program counter: index into `dictionary` of the currently executing cell
    pub pc: usize,
    /// Base pointer: index into data_stack marking the current stack frame base
    pub bp: usize,
    /// Boundary index in `dictionary` after all system primitives are registered.
    /// Set to `dictionary.len()` once system initialization is complete.
    pub dp_sys: usize,
    /// Boundary index in `dictionary` after the standard library is loaded.
    /// Set to `dictionary.len()` once standard library loading is complete.
    pub dp_lib: usize,
    /// Boundary index in `dictionary` at the start of the current user session.
    /// Set to `dictionary.len()` before user code is accepted.
    pub dp_user: usize,
    /// Boundary index in `headers` after all system primitives are registered.
    /// Mirrors `dp_sys` for the header layer. Updated alongside `dp_sys`.
    /// Used by FORGET to determine the lower bound for header rollback.
    pub hdr_sys: usize,
    /// Boundary index in `headers` after the standard library is loaded.
    /// Mirrors `dp_lib` for the header layer. Updated alongside `dp_lib`.
    pub hdr_lib: usize,
    /// Boundary index in `headers` at the start of the current user session.
    /// Mirrors `dp_user` for the header layer. Updated alongside `dp_user`.
    pub hdr_user: usize,
    /// Dictionary pointer (HERE): index of the next free write position in `dictionary`.
    /// Starts at 0 and advances as words are compiled or `ALLOT` is called.
    pub dp: usize,
    /// Index of the most recently registered entry in `headers` (head of linked list)
    pub latest: Option<Xt>,
}

impl VM {
    /// Create a new VM with empty header table, dictionary, and stacks.
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            dictionary: Vec::new(),
            string_pool: Vec::new(),
            data_stack: Vec::new(),
            return_stack: Vec::new(),
            pc: 0,
            bp: 0,
            dp_sys: 0,
            dp_lib: 0,
            dp_user: 0,
            hdr_sys: 0,
            hdr_lib: 0,
            hdr_user: 0,
            dp: 0,
            latest: None,
        }
    }

    /// Register a word entry in the header table, linking it into the search list.
    /// Returns the `Xt` (execution token) of the newly added entry.
    pub fn register(&mut self, mut entry: WordEntry) -> Xt {
        let xt = Xt(self.headers.len());
        entry.prev = self.latest;
        self.latest = Some(xt);
        self.headers.push(entry);
        xt
    }

    /// Look up a word by name, searching from newest to oldest entry via the linked list.
    /// Returns the `Xt` (header index) if found.
    pub fn lookup(&self, name: &str) -> Option<Xt> {
        let mut current = self.latest;
        while let Some(xt) = current {
            let entry = &self.headers[xt.index()];
            if entry.name == name {
                return Some(xt);
            }
            current = entry.prev;
        }
        None
    }

    /// Push a value onto the data stack.
    pub fn push(&mut self, cell: Cell) {
        self.data_stack.push(cell);
    }

    /// Pop a value from the data stack.
    pub fn pop(&mut self) -> Option<Cell> {
        self.data_stack.pop()
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::WordEntry;

    fn noop(_vm: &mut VM) {}

    #[test]
    fn test_vm_new() {
        let vm = VM::new();
        assert!(vm.headers.is_empty());
        assert!(vm.dictionary.is_empty());
        assert!(vm.data_stack.is_empty());
        assert!(vm.return_stack.is_empty());
        assert!(vm.latest.is_none());
        assert_eq!(vm.dp, 0);
    }

    #[test]
    fn test_push_pop() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42));
        assert_eq!(vm.pop(), Some(Cell::Int(42)));
        assert_eq!(vm.pop(), None);
    }

    #[test]
    fn test_register_and_lookup() {
        let mut vm = VM::new();
        vm.register(WordEntry::new_primitive("HALT", noop));
        vm.register(WordEntry::new_primitive("DROP", noop));

        assert_eq!(vm.lookup("HALT"), Some(Xt(0)));
        assert_eq!(vm.lookup("DROP"), Some(Xt(1)));
        assert_eq!(vm.lookup("MISSING"), None);
    }

    #[test]
    fn test_lookup_shadows_older_entry() {
        let mut vm = VM::new();
        vm.register(WordEntry::new_word("FOO", 0));
        vm.register(WordEntry::new_word("FOO", 10)); // shadows the first

        // Lookup should find the newer (index 1) entry
        assert_eq!(vm.lookup("FOO"), Some(Xt(1)));
    }

    #[test]
    fn test_lookup_linked_list_order() {
        let mut vm = VM::new();
        vm.register(WordEntry::new_primitive("A", noop));
        vm.register(WordEntry::new_primitive("B", noop));
        vm.register(WordEntry::new_primitive("C", noop));

        assert_eq!(vm.lookup("A"), Some(Xt(0)));
        assert_eq!(vm.lookup("B"), Some(Xt(1)));
        assert_eq!(vm.lookup("C"), Some(Xt(2)));
    }
}
