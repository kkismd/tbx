use crate::cell::{Cell, ReturnFrame, Xt};
use crate::constants::{MAX_DICTIONARY_CELLS, MAX_RETURN_STACK_DEPTH};
use crate::dict::WordEntry;
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
    pub return_stack: Vec<ReturnFrame>,
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
    /// Output buffer: collects text from PUTSTR / PUTCHR / PUTDEC / PUTHEX.
    /// Flushed to stdout at appropriate points (e.g. end of interpretation cycle).
    pub output_buffer: String,
    /// Compile mode flag: false = execution mode (STATE=0), true = compile mode (STATE=1).
    /// Toggled by DEF (enter compile mode) and END (return to execution mode).
    pub is_compiling: bool,
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
            output_buffer: String::new(),
            is_compiling: false,
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
    ///
    /// Returns `None` if the word is not found or if an out-of-bounds index is
    /// encountered in the linked list (defensive guard against corrupted state).
    pub fn lookup(&self, name: &str) -> Option<Xt> {
        let mut current = self.latest;
        while let Some(xt) = current {
            if xt.index() >= self.headers.len() {
                // Defensive: index is out of bounds; the linked list is corrupted.
                break;
            }
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
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the data stack is empty.
    pub fn pop(&mut self) -> Result<Cell, TbxError> {
        self.data_stack.pop().ok_or(TbxError::StackUnderflow)
    }

    /// Read a cell from the dictionary at the given index, with bounds checking.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::IndexOutOfBounds)` if `idx` is out of range.
    pub fn dict_read(&self, idx: usize) -> Result<Cell, TbxError> {
        let size = self.dictionary.len();
        self.dictionary
            .get(idx)
            .cloned()
            .ok_or(TbxError::IndexOutOfBounds { index: idx, size })
    }

    /// Write a cell to an arbitrary dictionary index, with bounds checking.
    ///
    /// Unlike `dict_write`, this does not advance `dp`; it overwrites an
    /// existing slot identified by `idx`.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::IndexOutOfBounds)` if `idx` is out of range.
    pub fn dict_write_at(&mut self, idx: usize, cell: Cell) -> Result<(), TbxError> {
        let size = self.dictionary.len();
        *self
            .dictionary
            .get_mut(idx)
            .ok_or(TbxError::IndexOutOfBounds { index: idx, size })? = cell;
        Ok(())
    }

    /// Read a local variable from the data stack at `bp + local_idx`.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::IndexOutOfBounds)` if `bp + local_idx` is out of range.
    pub fn local_read(&self, local_idx: usize) -> Result<Cell, TbxError> {
        let idx = self.bp + local_idx;
        let size = self.data_stack.len();
        self.data_stack
            .get(idx)
            .cloned()
            .ok_or(TbxError::IndexOutOfBounds { index: idx, size })
    }

    /// Write a local variable to the data stack at `bp + local_idx`.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::IndexOutOfBounds)` if `bp + local_idx` is out of range.
    pub fn local_write(&mut self, local_idx: usize, cell: Cell) -> Result<(), TbxError> {
        let idx = self.bp + local_idx;
        let size = self.data_stack.len();
        *self
            .data_stack
            .get_mut(idx)
            .ok_or(TbxError::IndexOutOfBounds { index: idx, size })? = cell;
        Ok(())
    }

    /// Write a cell to `dictionary[dp]` and advance dp by 1.
    ///
    /// Maintains the invariant `dp == dictionary.len()` by always using `push`.
    /// Checks the dictionary size limit before writing.
    pub fn dict_write(&mut self, cell: Cell) -> Result<(), TbxError> {
        debug_assert_eq!(
            self.dp,
            self.dictionary.len(),
            "dp must equal dictionary.len()"
        );
        if self.dp >= MAX_DICTIONARY_CELLS {
            return Err(TbxError::DictionaryOverflow {
                requested: self.dp + 1,
                limit: MAX_DICTIONARY_CELLS,
            });
        }
        self.dictionary.push(cell);
        self.dp += 1;
        Ok(())
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

    /// Append text to the output buffer.
    pub fn write_output(&mut self, s: &str) {
        self.output_buffer.push_str(s);
    }

    /// Take the current output buffer contents, leaving it empty.
    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output_buffer)
    }

    /// Run the inner interpreter starting from the given dictionary offset.
    ///
    /// Pushes a `ReturnFrame::TopLevel` sentinel before entering the loop.
    /// Execution ends when EXIT pops the TopLevel frame (normal end) or
    /// when a primitive returns `Err(TbxError::Halted)`.
    pub fn run(&mut self, start_offset: usize) -> Result<(), TbxError> {
        use crate::cell::ReturnFrame;
        use crate::dict::EntryKind;

        self.return_stack.push(ReturnFrame::TopLevel);
        self.pc = start_offset;

        loop {
            let entry_kind = self
                .headers
                .get(
                    self.dictionary
                        .get(self.pc)
                        .and_then(|c: &Cell| c.as_xt())
                        .ok_or(TbxError::TypeError {
                            expected: "Xt",
                            got: "non-Xt",
                        })?
                        .index(),
                )
                .ok_or(TbxError::IndexOutOfBounds {
                    index: self.pc,
                    size: self.headers.len(),
                })?
                .kind
                .clone();

            match entry_kind {
                EntryKind::Primitive(f) => {
                    f(self)?;
                    self.pc += 1;
                }
                EntryKind::Word(offset) => {
                    let return_pc = self.pc + 1;
                    let saved_bp = self.bp;
                    if self.return_stack.len() >= MAX_RETURN_STACK_DEPTH {
                        return Err(TbxError::ReturnStackOverflow {
                            depth: self.return_stack.len(),
                            limit: MAX_RETURN_STACK_DEPTH,
                        });
                    }
                    self.return_stack.push(ReturnFrame::Call {
                        return_pc,
                        saved_bp,
                    });
                    self.bp = self.data_stack.len();
                    self.pc = offset;
                }
                EntryKind::Call => {
                    let target_xt = self
                        .dictionary
                        .get(self.pc + 1)
                        .and_then(|c: &Cell| c.as_xt())
                        .ok_or(TbxError::IndexOutOfBounds {
                            index: self.pc + 1,
                            size: self.dictionary.len(),
                        })?;
                    let arity_raw = self
                        .dictionary
                        .get(self.pc + 2)
                        .and_then(|c: &Cell| c.as_int())
                        .ok_or(TbxError::TypeError {
                            expected: "Int (arity)",
                            got: "non-Int",
                        })?;
                    if arity_raw < 0 {
                        return Err(TbxError::TypeError {
                            expected: "non-negative Int (arity)",
                            got: "negative value",
                        });
                    }
                    let arity = arity_raw as usize;
                    let local_count_raw = self
                        .dictionary
                        .get(self.pc + 3)
                        .and_then(|c: &Cell| c.as_int())
                        .ok_or(TbxError::TypeError {
                            expected: "Int (local count)",
                            got: "non-Int",
                        })?;
                    if local_count_raw < 0 {
                        return Err(TbxError::TypeError {
                            expected: "non-negative Int (local count)",
                            got: "negative value",
                        });
                    }
                    let local_count = local_count_raw as usize;
                    match self
                        .headers
                        .get(target_xt.index())
                        .ok_or(TbxError::IndexOutOfBounds {
                            index: target_xt.index(),
                            size: self.headers.len(),
                        })?
                        .kind
                        .clone()
                    {
                        EntryKind::Word(offset) => {
                            let return_pc = self.pc + 4;
                            let saved_bp = self.bp;
                            if self.data_stack.len() < arity {
                                return Err(TbxError::StackUnderflow);
                            }
                            if self.return_stack.len() >= MAX_RETURN_STACK_DEPTH {
                                return Err(TbxError::ReturnStackOverflow {
                                    depth: self.return_stack.len(),
                                    limit: MAX_RETURN_STACK_DEPTH,
                                });
                            }
                            self.return_stack.push(ReturnFrame::Call {
                                return_pc,
                                saved_bp,
                            });
                            self.bp = self.data_stack.len() - arity;
                            for _ in 0..local_count {
                                self.push(Cell::Int(0));
                            }
                            self.pc = offset;
                        }
                        _ => {
                            return Err(TbxError::TypeError {
                                expected: "Word",
                                got: "non-Word",
                            })
                        }
                    }
                }
                EntryKind::Exit => {
                    match self.return_stack.pop().ok_or(TbxError::StackUnderflow)? {
                        ReturnFrame::Call {
                            return_pc,
                            saved_bp,
                        } => {
                            self.data_stack.truncate(self.bp);
                            self.pc = return_pc;
                            self.bp = saved_bp;
                        }
                        ReturnFrame::TopLevel => break,
                    }
                }
                EntryKind::ReturnVal => {
                    let retval = self.pop()?;
                    match self.return_stack.pop().ok_or(TbxError::StackUnderflow)? {
                        ReturnFrame::Call {
                            return_pc,
                            saved_bp,
                        } => {
                            self.data_stack.truncate(self.bp);
                            self.push(retval);
                            self.pc = return_pc;
                            self.bp = saved_bp;
                        }
                        ReturnFrame::TopLevel => {
                            return Err(TbxError::InvalidReturn);
                        }
                    }
                }
                EntryKind::DropToMarker => {
                    loop {
                        match self.data_stack.pop() {
                            Some(Cell::Marker) => break,
                            Some(_) => {} // discard non-marker cells
                            None => return Err(TbxError::MarkerNotFound),
                        }
                    }
                    self.pc += 1;
                }

                EntryKind::Lit => {
                    self.pc += 1;
                    let literal = self
                        .dictionary
                        .get(self.pc)
                        .ok_or(TbxError::IndexOutOfBounds {
                            index: self.pc,
                            size: self.dictionary.len(),
                        })?
                        .clone();
                    self.push(literal);
                    self.pc += 1;
                }
                EntryKind::Variable(idx) => {
                    self.push(Cell::DictAddr(idx));
                    self.pc += 1;
                }
                EntryKind::Constant(ref c) => {
                    let val = c.clone();
                    self.push(val);
                    self.pc += 1;
                }
            }
        }

        Ok(())
    }

    /// Resolve a StringDesc index to the string stored in the string pool.
    ///
    /// Returns `Err(TbxError::TypeError)` if the index is out of bounds or
    /// the stored data is not valid UTF-8.
    pub fn resolve_string(&self, idx: usize) -> Result<String, TbxError> {
        if idx + 2 > self.string_pool.len() {
            return Err(TbxError::IndexOutOfBounds {
                index: idx,
                size: self.string_pool.len(),
            });
        }
        let len = u16::from_le_bytes([self.string_pool[idx], self.string_pool[idx + 1]]) as usize;
        let start = idx + 2;
        let end = start + len;
        if end > self.string_pool.len() {
            return Err(TbxError::IndexOutOfBounds {
                index: end,
                size: self.string_pool.len(),
            });
        }
        String::from_utf8(self.string_pool[start..end].to_vec()).map_err(|_| TbxError::TypeError {
            expected: "valid UTF-8",
            got: "invalid bytes",
        })
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
        self.string_pool
            .extend_from_slice(&(bytes.len() as u16).to_le_bytes());
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

    fn noop(_vm: &mut VM) -> Result<(), crate::error::TbxError> {
        Ok(())
    }

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
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert_eq!(vm.pop(), Err(crate::error::TbxError::StackUnderflow));
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
        assert_eq!(
            u16::from_le_bytes([vm.string_pool[0], vm.string_pool[1]]),
            5
        );
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
        assert_eq!(
            u16::from_le_bytes([vm.string_pool[idx1], vm.string_pool[idx1 + 1]]),
            2
        );
        assert_eq!(&vm.string_pool[idx1 + 2..idx1 + 4], b"hi");
        assert_eq!(
            u16::from_le_bytes([vm.string_pool[idx2], vm.string_pool[idx2 + 1]]),
            5
        );
        assert_eq!(&vm.string_pool[idx2 + 2..idx2 + 7], b"world");
    }

    #[test]
    fn test_intern_string_empty() {
        let mut vm = VM::new();
        let idx = vm.intern_string("").unwrap();
        assert_eq!(idx, 0);
        // zero-length string: 2 length bytes (both 0) + 0 data bytes
        assert_eq!(
            u16::from_le_bytes([vm.string_pool[0], vm.string_pool[1]]),
            0
        );
        assert_eq!(vm.string_pool.len(), 2);
    }

    #[test]
    fn test_intern_string_too_long() {
        let mut vm = VM::new();
        let long_str = "x".repeat(65536);
        let result = vm.intern_string(&long_str);
        assert!(matches!(
            result,
            Err(crate::error::TbxError::StringTooLong { len: 65536 })
        ));
    }

    #[test]
    fn test_intern_string_max_length() {
        let mut vm = VM::new();
        let max_str = "x".repeat(65535);
        let result = vm.intern_string(&max_str);
        assert!(result.is_ok());
        // length prefix: u16 little-endian 65535 = [0xFF, 0xFF]
        assert_eq!(
            u16::from_le_bytes([vm.string_pool[0], vm.string_pool[1]]),
            65535
        );
        assert_eq!(vm.string_pool.len(), 65537); // 2 length bytes + 65535 data bytes
        assert_eq!(&vm.string_pool[2..], max_str.as_bytes());
    }

    #[test]
    fn test_intern_string_multibyte_utf8() {
        let mut vm = VM::new();
        // "あ" is 3 bytes in UTF-8; the length prefix must record byte length, not char count
        let idx = vm.intern_string("あ").unwrap();
        assert_eq!(
            u16::from_le_bytes([vm.string_pool[idx], vm.string_pool[idx + 1]]),
            3
        );
        assert_eq!(&vm.string_pool[idx + 2..idx + 5], "あ".as_bytes());
    }

    #[test]
    fn test_lookup_out_of_bounds_latest_returns_none() {
        // Simulate corrupted state: vm.latest points to an index beyond headers.len().
        // lookup() must return None instead of panicking.
        let mut vm = VM::new();
        // Directly set latest to an out-of-bounds Xt (index 99, but headers is empty)
        vm.latest = Some(Xt(99));
        // Should return None, not panic
        assert_eq!(vm.lookup("FOO"), None);
    }

    #[test]
    fn test_run_primitive_drop() {
        // Verify that the inner interpreter can execute a primitive (DROP) via an Xt cell.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let drop_xt = vm.lookup("DROP").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Program: [Xt(DROP), Xt(EXIT)]
        vm.dict_write(Cell::Xt(drop_xt)).unwrap();
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();

        vm.push(Cell::Int(99));
        vm.run(0).unwrap();

        // DROP should have removed the 99
        assert_eq!(vm.pop(), Err(crate::error::TbxError::StackUnderflow));
    }

    #[test]
    fn test_run_lit() {
        // Verify that EntryKind::Lit pushes the next cell as a literal value.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_xt = vm.lookup("LIT").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Program: [Xt(LIT), Int(42), Xt(EXIT)]
        vm.dict_write(Cell::Xt(lit_xt)).unwrap();
        vm.dict_write(Cell::Int(42)).unwrap();
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();

        vm.run(0).unwrap();

        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
    }

    #[test]
    fn test_run_word_call() {
        // Verify CALL 4-cell format: [Xt(CALL), Xt(F), arity, local_count]
        // Layout:
        //   [0] Xt(CALL)     <- CALL instruction
        //   [1] Xt(MY_WORD)  <- target word
        //   [2] Int(1)       <- arity = 1
        //   [3] Int(0)       <- local_count = 0
        //   [4] Xt(EXIT)     <- top-level exit
        //   [5] Xt(DUP)      <- MY_WORD body: DUP
        //   [6] Xt(EXIT)     <- MY_WORD body: EXIT (void return)
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let dup_xt = vm.lookup("DUP").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Register MY_WORD pointing to offset 5
        let my_word_xt = vm.register(crate::dict::WordEntry::new_word("MY_WORD", 5));

        vm.dict_write(Cell::Xt(call_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Xt(my_word_xt)).unwrap(); // [1]
        vm.dict_write(Cell::Int(1)).unwrap(); // [2] arity=1
        vm.dict_write(Cell::Int(0)).unwrap(); // [3] local_count=0
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4]
        vm.dict_write(Cell::Xt(dup_xt)).unwrap(); // [5]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [6]

        vm.push(Cell::Int(7)); // argument
        vm.run(0).unwrap();

        // DUP duplicated the arg, but EXIT truncates to bp.
        // arity=1, so bp points at the arg. EXIT truncates everything from bp onward.
        // Result: stack is empty (void return clears args)
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_return_val() {
        // Verify RETURN_VAL returns a value from a word.
        // MY_WORD(x): pushes LIT 100, then RETURN_VAL → returns 100
        // Layout:
        //   [0] Xt(CALL)         <- CALL instruction
        //   [1] Xt(MY_WORD)      <- target word
        //   [2] Int(1)           <- arity = 1
        //   [3] Int(0)           <- local_count = 0
        //   [4] Xt(EXIT)         <- top-level exit
        //   [5] Xt(LIT)          <- MY_WORD body: push literal
        //   [6] Int(100)         <- literal value
        //   [7] Xt(RETURN_VAL)   <- return with value
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();
        let lit_xt = vm.lookup("LIT").unwrap();
        let return_val_xt = vm.lookup("RETURN_VAL").unwrap();

        let my_word_xt = vm.register(crate::dict::WordEntry::new_word("MY_WORD", 5));

        vm.dict_write(Cell::Xt(call_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Xt(my_word_xt)).unwrap(); // [1]
        vm.dict_write(Cell::Int(1)).unwrap(); // [2] arity=1
        vm.dict_write(Cell::Int(0)).unwrap(); // [3] local_count=0
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4]
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [5]
        vm.dict_write(Cell::Int(100)).unwrap(); // [6]
        vm.dict_write(Cell::Xt(return_val_xt)).unwrap(); // [7]

        vm.push(Cell::Int(7)); // argument (will be cleaned up)
        vm.run(0).unwrap();

        // RETURN_VAL should leave only the return value (100) on the stack
        assert_eq!(vm.pop(), Ok(Cell::Int(100)));
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_return_val_with_locals() {
        // Verify RETURN_VAL with arity=2 and local_count=1.
        // MY_WORD(a, b) with VAR z: returns LIT 42 via RETURN_VAL
        // Layout:
        //   [0] Xt(CALL)         <- CALL instruction
        //   [1] Xt(MY_WORD)      <- target word
        //   [2] Int(2)           <- arity = 2
        //   [3] Int(1)           <- local_count = 1
        //   [4] Xt(EXIT)         <- top-level exit
        //   [5] Xt(LIT)          <- MY_WORD body: push literal
        //   [6] Int(42)          <- literal value
        //   [7] Xt(RETURN_VAL)   <- return with value
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();
        let lit_xt = vm.lookup("LIT").unwrap();
        let return_val_xt = vm.lookup("RETURN_VAL").unwrap();

        let my_word_xt = vm.register(crate::dict::WordEntry::new_word("MY_WORD", 5));

        vm.dict_write(Cell::Xt(call_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Xt(my_word_xt)).unwrap(); // [1]
        vm.dict_write(Cell::Int(2)).unwrap(); // [2] arity=2
        vm.dict_write(Cell::Int(1)).unwrap(); // [3] local_count=1
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4]
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [5]
        vm.dict_write(Cell::Int(42)).unwrap(); // [6]
        vm.dict_write(Cell::Xt(return_val_xt)).unwrap(); // [7]

        vm.push(Cell::Int(10)); // arg a
        vm.push(Cell::Int(20)); // arg b
        vm.run(0).unwrap();

        // args (10, 20) and local (0) should be cleaned up.
        // Only return value 42 remains.
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_call_negative_arity_returns_error() {
        // Verify that a negative arity operand in CALL returns a TypeError.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Dummy word body: just EXIT
        let word_offset = vm.dp;
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();
        let dummy_xt = vm.register(crate::dict::WordEntry::new_word("DUMMY", word_offset));

        // Top-level: CALL DUMMY arity=-1 local_count=0
        let start = vm.dp;
        vm.dict_write(Cell::Xt(call_xt)).unwrap();
        vm.dict_write(Cell::Xt(dummy_xt)).unwrap();
        vm.dict_write(Cell::Int(-1)).unwrap(); // negative arity
        vm.dict_write(Cell::Int(0)).unwrap();

        let result = vm.run(start);
        assert!(matches!(
            result,
            Err(crate::error::TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_call_arity_exceeds_stack_returns_underflow() {
        // Verify that calling with arity > stack depth returns StackUnderflow.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Dummy word body: just EXIT
        let word_offset = vm.dp;
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();
        let dummy_xt = vm.register(crate::dict::WordEntry::new_word("DUMMY2", word_offset));

        // Top-level: CALL DUMMY2 arity=5 local_count=0, but stack is empty
        let start = vm.dp;
        vm.dict_write(Cell::Xt(call_xt)).unwrap();
        vm.dict_write(Cell::Xt(dummy_xt)).unwrap();
        vm.dict_write(Cell::Int(5)).unwrap(); // arity=5, stack is empty
        vm.dict_write(Cell::Int(0)).unwrap();

        let result = vm.run(start);
        assert!(matches!(
            result,
            Err(crate::error::TbxError::StackUnderflow)
        ));
    }

    #[test]
    fn test_return_val_at_top_level_returns_error() {
        // Verify that RETURN_VAL executed at top level (outside a word) returns InvalidReturn.
        // Layout: [Xt(LIT), Int(42), Xt(RETURN_VAL)]
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_xt = vm.lookup("LIT").unwrap();
        let return_val_xt = vm.lookup("RETURN_VAL").unwrap();

        let start = vm.dp;
        vm.dict_write(Cell::Xt(lit_xt)).unwrap();
        vm.dict_write(Cell::Int(42)).unwrap();
        vm.dict_write(Cell::Xt(return_val_xt)).unwrap();

        let result = vm.run(start);
        assert!(matches!(result, Err(crate::error::TbxError::InvalidReturn)));
    }

    #[test]
    fn test_drop_to_marker_after_void_statement() {
        // Verifies that DROP_TO_MARKER clears Marker + args after a void return statement.
        //
        // Top-level layout:
        //   [start+0] LIT_MARKER
        //   [start+1] LIT
        //   [start+2] Int(42)       <- argument
        //   [start+3] CALL
        //   [start+4] Xt(STMT)
        //   [start+5] Int(1)        <- arity=1
        //   [start+6] Int(0)        <- local_count=0
        //   [start+7] DROP_TO_MARKER
        //   [start+8] EXIT          <- top-level end
        //   [start+9] EXIT          <- STMT body (void return)
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_marker_xt = vm.lookup("LIT_MARKER").unwrap();
        let lit_xt = vm.lookup("LIT").unwrap();
        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();
        let drop_to_marker_xt = vm.lookup("DROP_TO_MARKER").unwrap();

        let start = vm.dp;
        let stmt_xt = vm.register(crate::dict::WordEntry::new_word("STMT", start + 9));

        vm.dict_write(Cell::Xt(lit_marker_xt)).unwrap(); // [start+0]
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [start+1]
        vm.dict_write(Cell::Int(42)).unwrap(); // [start+2]
        vm.dict_write(Cell::Xt(call_xt)).unwrap(); // [start+3]
        vm.dict_write(Cell::Xt(stmt_xt)).unwrap(); // [start+4]
        vm.dict_write(Cell::Int(1)).unwrap(); // [start+5] arity=1
        vm.dict_write(Cell::Int(0)).unwrap(); // [start+6] local_count=0
        vm.dict_write(Cell::Xt(drop_to_marker_xt)).unwrap(); // [start+7]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [start+8] top-level end
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [start+9] STMT body

        vm.run(start).unwrap();

        // Marker and arg should be gone; stack must be empty.
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_drop_to_marker_after_value_returning_statement() {
        // Verifies that DROP_TO_MARKER discards the return value as well as Marker + args.
        //
        // Top-level layout:
        //   [start+0]  LIT_MARKER
        //   [start+1]  LIT
        //   [start+2]  Int(42)       <- argument
        //   [start+3]  CALL
        //   [start+4]  Xt(STMT2)
        //   [start+5]  Int(1)        <- arity=1
        //   [start+6]  Int(0)        <- local_count=0
        //   [start+7]  DROP_TO_MARKER
        //   [start+8]  EXIT          <- top-level end
        //   [start+9]  LIT           <- STMT2 body: push 99
        //   [start+10] Int(99)
        //   [start+11] RETURN_VAL    <- return with value 99
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_marker_xt = vm.lookup("LIT_MARKER").unwrap();
        let lit_xt = vm.lookup("LIT").unwrap();
        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();
        let drop_to_marker_xt = vm.lookup("DROP_TO_MARKER").unwrap();
        let return_val_xt = vm.lookup("RETURN_VAL").unwrap();

        let start = vm.dp;
        let stmt2_xt = vm.register(crate::dict::WordEntry::new_word("STMT2", start + 9));

        vm.dict_write(Cell::Xt(lit_marker_xt)).unwrap(); // [start+0]
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [start+1]
        vm.dict_write(Cell::Int(42)).unwrap(); // [start+2] arg
        vm.dict_write(Cell::Xt(call_xt)).unwrap(); // [start+3]
        vm.dict_write(Cell::Xt(stmt2_xt)).unwrap(); // [start+4]
        vm.dict_write(Cell::Int(1)).unwrap(); // [start+5] arity=1
        vm.dict_write(Cell::Int(0)).unwrap(); // [start+6] local_count=0
        vm.dict_write(Cell::Xt(drop_to_marker_xt)).unwrap(); // [start+7]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [start+8] top-level end
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [start+9]  STMT2: LIT 99
        vm.dict_write(Cell::Int(99)).unwrap(); // [start+10]
        vm.dict_write(Cell::Xt(return_val_xt)).unwrap(); // [start+11] RETURN_VAL

        vm.run(start).unwrap();

        // return value 99 must also be discarded by DROP_TO_MARKER.
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_drop_to_marker_without_marker_returns_error() {
        // Verifies that DROP_TO_MARKER returns MarkerNotFound when no Marker is on the stack.
        //
        // Top-level layout:
        //   [start+0] LIT
        //   [start+1] Int(1)           <- non-marker value
        //   [start+2] DROP_TO_MARKER   <- should fail: no Marker present
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_xt = vm.lookup("LIT").unwrap();
        let drop_to_marker_xt = vm.lookup("DROP_TO_MARKER").unwrap();

        let start = vm.dp;
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [start+0]
        vm.dict_write(Cell::Int(1)).unwrap(); // [start+1]
        vm.dict_write(Cell::Xt(drop_to_marker_xt)).unwrap(); // [start+2]

        let result = vm.run(start);
        assert!(matches!(
            result,
            Err(crate::error::TbxError::MarkerNotFound)
        ));
    }

    #[test]
    fn test_word_call_return_stack_overflow() {
        // Verify that calling a word via EntryKind::Word when the return stack is at
        // MAX_RETURN_STACK_DEPTH returns ReturnStackOverflow.
        use crate::constants::MAX_RETURN_STACK_DEPTH;

        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let exit_xt = vm.lookup("EXIT").unwrap();

        // Build a simple word body: EXIT
        let word_offset = vm.dp;
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();
        let word_xt = vm.register(crate::dict::WordEntry::new_word("MY_WORD", word_offset));

        // Pre-fill the return stack to its limit with dummy frames
        for _ in 0..MAX_RETURN_STACK_DEPTH {
            vm.return_stack.push(ReturnFrame::Call {
                return_pc: 0,
                saved_bp: 0,
            });
        }

        // Emit top-level: Xt(MY_WORD)
        let start = vm.dp;
        vm.dict_write(Cell::Xt(word_xt)).unwrap();

        let result = vm.run(start);
        assert!(
            matches!(
                result,
                Err(crate::error::TbxError::ReturnStackOverflow { .. })
            ),
            "expected ReturnStackOverflow, got {:?}",
            result
        );
    }

    #[test]
    fn test_call_instruction_return_stack_overflow() {
        // Verify that the CALL instruction (EntryKind::Call) also enforces the return stack limit.
        use crate::constants::MAX_RETURN_STACK_DEPTH;

        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Build word body: EXIT
        let word_offset = vm.dp;
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();
        let word_xt = vm.register(crate::dict::WordEntry::new_word("CALLEE", word_offset));

        // Pre-fill the return stack to its limit
        for _ in 0..MAX_RETURN_STACK_DEPTH {
            vm.return_stack.push(ReturnFrame::Call {
                return_pc: 0,
                saved_bp: 0,
            });
        }

        // Emit top-level: CALL CALLEE arity=0 local_count=0
        let start = vm.dp;
        vm.dict_write(Cell::Xt(call_xt)).unwrap();
        vm.dict_write(Cell::Xt(word_xt)).unwrap();
        vm.dict_write(Cell::Int(0)).unwrap(); // arity=0
        vm.dict_write(Cell::Int(0)).unwrap(); // local_count=0

        let result = vm.run(start);
        assert!(
            matches!(
                result,
                Err(crate::error::TbxError::ReturnStackOverflow { .. })
            ),
            "expected ReturnStackOverflow, got {:?}",
            result
        );
    }
}
