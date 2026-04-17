use crate::dict::WordEntry;
use crate::cell::{Cell, Xt};
use crate::error::TbxError;

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

    /// Seal the system dictionary boundary.
    ///
    /// Records the current `dp` and `headers.len()` as the end of the system
    /// dictionary layer. Call this once all system primitives have been registered.
    pub fn seal_sys(&mut self) {
        self.dp_sys = self.dp;
        self.hdr_sys = self.headers.len();
    }

    /// Seal the standard library dictionary boundary.
    ///
    /// Records the current `dp` and `headers.len()` as the end of the standard
    /// library layer. Call this once the standard library has been loaded.
    pub fn seal_lib(&mut self) {
        self.dp_lib = self.dp;
        self.hdr_lib = self.headers.len();
    }

    /// Seal the user dictionary boundary.
    ///
    /// Records the current `dp` and `headers.len()` as the end of the user
    /// dictionary layer. Call this before accepting user code in a new session.
    pub fn seal_user(&mut self) {
        self.dp_user = self.dp;
        self.hdr_user = self.headers.len();
    }

    /// Intern a string into the string pool using the length-prefix format.
    ///
    /// Appends the string as: two bytes (little-endian `u16`) for the UTF-8
    /// byte length, followed by the raw bytes. Returns the index of the first
    /// length byte in `string_pool`.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StringTooLong)` if `s` is longer than 65535
    /// bytes, since the length prefix is a `u16` (max value 65535).
    /// The parser must enforce this limit before calling this function.
    pub fn intern_string(&mut self, s: &str) -> Result<usize, TbxError> {
        let bytes = s.as_bytes();
        if bytes.len() > u16::MAX as usize {
            return Err(TbxError::StringTooLong { len: bytes.len() });
        }
        let idx = self.string_pool.len();
        self.string_pool.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        self.string_pool.extend_from_slice(bytes);
        Ok(idx)
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

    #[test]
    fn test_seal_sys() {
        let mut vm = VM::new();
        vm.dp = 10;
        vm.register(WordEntry::new_primitive("A", noop));
        vm.register(WordEntry::new_primitive("B", noop));
        vm.seal_sys();
        assert_eq!(vm.dp_sys, 10);
        assert_eq!(vm.hdr_sys, 2);
        // lib and user remain untouched
        assert_eq!(vm.dp_lib, 0);
        assert_eq!(vm.dp_user, 0);
    }

    #[test]
    fn test_seal_lib() {
        let mut vm = VM::new();
        vm.dp = 20;
        vm.register(WordEntry::new_primitive("X", noop));
        vm.seal_lib();
        assert_eq!(vm.dp_lib, 20);
        assert_eq!(vm.hdr_lib, 1);
        assert_eq!(vm.dp_sys, 0);
        assert_eq!(vm.dp_user, 0);
    }

    #[test]
    fn test_seal_user() {
        let mut vm = VM::new();
        vm.dp = 42;
        vm.register(WordEntry::new_primitive("Y", noop));
        vm.seal_user();
        assert_eq!(vm.dp_user, 42);
        assert_eq!(vm.hdr_user, 1);
        assert_eq!(vm.dp_sys, 0);
        assert_eq!(vm.dp_lib, 0);
    }

    #[test]
    fn test_seal_progression() {
        // Simulate registering system -> lib -> user words and sealing each layer.
        let mut vm = VM::new();

        vm.dp = 5;
        vm.register(WordEntry::new_primitive("SYS1", noop));
        vm.seal_sys();

        vm.dp = 15;
        vm.register(WordEntry::new_primitive("LIB1", noop));
        vm.seal_lib();

        vm.dp = 30;
        vm.register(WordEntry::new_primitive("USR1", noop));
        vm.seal_user();

        assert_eq!(vm.dp_sys, 5);
        assert_eq!(vm.hdr_sys, 1);
        assert_eq!(vm.dp_lib, 15);
        assert_eq!(vm.hdr_lib, 2);
        assert_eq!(vm.dp_user, 30);
        assert_eq!(vm.hdr_user, 3);
    }

    #[test]
    fn test_intern_string_basic() {
        let mut vm = VM::new();
        let idx = vm.intern_string("hello").unwrap();
        assert_eq!(idx, 0);
        // length prefix: u16 little-endian, so [5, 0]
        assert_eq!(u16::from_le_bytes([vm.string_pool[0], vm.string_pool[1]]), 5);
        assert_eq!(&vm.string_pool[2..7], b"hello");
    }

    #[test]
    fn test_intern_string_multiple() {
        let mut vm = VM::new();
        let idx1 = vm.intern_string("hi").unwrap();
        let idx2 = vm.intern_string("world").unwrap();
        // "hi" occupies bytes 0..=3 (2 length bytes + 2 data bytes)
        assert_eq!(idx1, 0);
        assert_eq!(idx2, 4); // 2 (len) + 2 (data) = 4
        assert_eq!(u16::from_le_bytes([vm.string_pool[idx1], vm.string_pool[idx1 + 1]]), 2);
        assert_eq!(&vm.string_pool[idx1 + 2..idx1 + 4], b"hi");
        assert_eq!(u16::from_le_bytes([vm.string_pool[idx2], vm.string_pool[idx2 + 1]]), 5);
        assert_eq!(&vm.string_pool[idx2 + 2..idx2 + 7], b"world");
    }

    #[test]
    fn test_intern_string_empty() {
        let mut vm = VM::new();
        let idx = vm.intern_string("").unwrap();
        assert_eq!(idx, 0);
        // zero-length string: 2 length bytes (both 0) + 0 data bytes
        assert_eq!(u16::from_le_bytes([vm.string_pool[0], vm.string_pool[1]]), 0);
        assert_eq!(vm.string_pool.len(), 2);
    }

    #[test]
    fn test_intern_string_too_long() {
        let mut vm = VM::new();
        let long_str = "x".repeat(65536);
        let result = vm.intern_string(&long_str);
        assert!(matches!(result, Err(crate::error::TbxError::StringTooLong { len: 65536 })));
    }

    #[test]
    fn test_intern_string_max_length() {
        let mut vm = VM::new();
        let max_str = "x".repeat(65535);
        let result = vm.intern_string(&max_str);
        assert!(result.is_ok());
        // length prefix: u16 little-endian 65535 = [0xFF, 0xFF]
        assert_eq!(u16::from_le_bytes([vm.string_pool[0], vm.string_pool[1]]), 65535);
        assert_eq!(vm.string_pool.len(), 65537); // 2 length bytes + 65535 data bytes
        assert_eq!(&vm.string_pool[2..], max_str.as_bytes());
    }

    #[test]
    fn test_intern_string_multibyte_utf8() {
        let mut vm = VM::new();
        // "あ" is 3 bytes in UTF-8; the length prefix must record byte length, not char count
        let idx = vm.intern_string("あ").unwrap();
        assert_eq!(u16::from_le_bytes([vm.string_pool[idx], vm.string_pool[idx + 1]]), 3);
        assert_eq!(&vm.string_pool[idx + 2..idx + 5], "あ".as_bytes());
    }
}
