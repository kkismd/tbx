use crate::cell::{Cell, ReturnFrame, Xt};
use crate::constants::{MAX_DATA_STACK_DEPTH, MAX_DICTIONARY_CELLS, MAX_RETURN_STACK_DEPTH};
use crate::dict::{EntryKind, WordEntry};
use crate::error::TbxError;
use crate::lexer::SpannedToken;
use std::collections::HashMap;
use std::collections::VecDeque;

/// State maintained during compilation of a new word definition (DEF..END).
#[derive(Debug)]
pub struct CompileState {
    /// Name of the word being compiled.
    pub word_name: String,
    /// Dictionary pointer at the start of DEF (for rollback on error).
    dp_at_def: usize,
    /// Header index of the word being compiled.
    /// Used both for rollback (restoring headers) and for self-recursive call detection.
    hdr_len_at_def: usize,
    /// Saved `latest` pointer before DEF (restored on rollback).
    saved_latest: Option<crate::cell::Xt>,
    /// Local variable table: maps variable name to StackAddr index.
    /// Parameters are assigned indices 0..arity, VAR locals start at arity.
    pub(crate) local_table: HashMap<String, usize>,
    /// Number of formal parameters parsed from DEF WORD(X, Y, ...).
    pub(crate) arity: usize,
    /// Number of VAR-declared local variables encountered so far.
    pub(crate) local_count: usize,
    /// Dictionary offsets of the `local_count` placeholder (Int(0)) in CALL instructions
    /// that refer to the currently-compiled word (self-recursive calls).
    /// Patched to the final `local_count` when END is compiled.
    pub(crate) call_patch_list: Vec<usize>,
    /// Maps line-number label to dictionary offset recorded when the label was seen.
    pub(crate) label_table: HashMap<i64, usize>,
    /// (label_number, dict_offset_of_placeholder) waiting to be back-patched.
    pub(crate) patch_list: Vec<(i64, usize)>,
}

impl CompileState {
    /// Create a new `CompileState` for a DEF..END compilation.
    ///
    /// The rollback fields (`dp_at_def`, `hdr_len_at_def`, `saved_latest`) are kept
    /// private; they are only used by `VM::rollback_def()`.
    pub(crate) fn new_for_def(
        word_name: String,
        dp_at_def: usize,
        hdr_len_at_def: usize,
        saved_latest: Option<Xt>,
        local_table: HashMap<String, usize>,
        arity: usize,
    ) -> Self {
        Self {
            word_name,
            dp_at_def,
            hdr_len_at_def,
            saved_latest,
            local_table,
            arity,
            local_count: 0,
            call_patch_list: Vec::new(),
            label_table: HashMap::new(),
            patch_list: Vec::new(),
        }
    }

    /// Return the header-table index of the word currently being compiled.
    ///
    /// This is used for self-recursive call detection and for updating the
    /// word's `local_count` and smudge flag when END finalises the definition.
    pub(crate) fn word_hdr_idx(&self) -> usize {
        self.hdr_len_at_def
    }

    /// Return the rollback information saved at the start of this definition.
    ///
    /// Callers that need to roll back after `compile_state.take()` should capture
    /// this tuple first, then perform the rollback manually.
    pub(crate) fn rollback_info(&self) -> (usize, usize, Option<Xt>) {
        (self.dp_at_def, self.hdr_len_at_def, self.saved_latest)
    }
}

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
    /// Token stream set by the outer interpreter before calling vm.run() for IMMEDIATE words.
    /// Primitives consume tokens one at a time via `next_token()`.
    /// Set to `None` outside of immediate-word execution.
    pub token_stream: Option<VecDeque<SpannedToken>>,
    /// State maintained during compilation of a new word definition (DEF..END).
    /// `None` in execution mode; `Some(...)` while compiling.
    pub(crate) compile_state: Option<CompileState>,
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
            token_stream: None,
            compile_state: None,
        }
    }

    /// Consume the next token from the token stream.
    ///
    /// Returns `TbxError::TokenStreamEmpty` if `token_stream` is `None` or empty.
    ///
    /// Note: when the stream is exhausted, `token_stream` remains `Some([])` rather
    /// than being reset to `None`. The caller (outer interpreter) is responsible for
    /// setting `token_stream` back to `None` after the immediate-word execution
    /// completes, so that `is_some()` reliably indicates "currently in an
    /// immediate-word execution context".
    pub fn next_token(&mut self) -> Result<SpannedToken, TbxError> {
        match &mut self.token_stream {
            Some(stream) => stream.pop_front().ok_or(TbxError::TokenStreamEmpty),
            None => Err(TbxError::TokenStreamEmpty),
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
            // Skip hidden (smudge) entries — they are still being compiled.
            if entry.name == name && entry.flags & crate::dict::FLAG_HIDDEN == 0 {
                return Some(xt);
            }
            current = entry.prev;
        }
        None
    }

    /// Like `lookup`, but also returns the entry with `FLAG_HIDDEN` set **only when**
    /// the name matches `self_word`.
    ///
    /// This supports self-recursive calls during compilation: a word is visible to
    /// itself (for recursion) but still hidden from other lookups (e.g. operator
    /// primitives with the same name).
    pub fn lookup_including_self(&self, name: &str, self_word: Option<&str>) -> Option<Xt> {
        let allow_hidden = self_word.map(|sw| sw == name).unwrap_or(false);
        let mut current = self.latest;
        while let Some(xt) = current {
            if xt.index() >= self.headers.len() {
                break;
            }
            let entry = &self.headers[xt.index()];
            let is_hidden = entry.flags & crate::dict::FLAG_HIDDEN != 0;
            if entry.name == name && (!is_hidden || allow_hidden) {
                return Some(xt);
            }
            current = entry.prev;
        }
        None
    }

    /// Push a value onto the data stack.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::DataStackOverflow)` if the data stack is at or above
    /// `MAX_DATA_STACK_DEPTH`.
    pub fn push(&mut self, cell: Cell) -> Result<(), TbxError> {
        if self.data_stack.len() >= MAX_DATA_STACK_DEPTH {
            return Err(TbxError::DataStackOverflow {
                depth: self.data_stack.len(),
                limit: MAX_DATA_STACK_DEPTH,
            });
        }
        self.data_stack.push(cell);
        Ok(())
    }

    /// Pop a value from the data stack.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the data stack is empty.
    pub fn pop(&mut self) -> Result<Cell, TbxError> {
        self.data_stack.pop().ok_or(TbxError::StackUnderflow)
    }

    /// Pop an `Int` value from the data stack.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the stack is empty.
    /// Returns `Err(TbxError::TypeError)` if the top value is not `Cell::Int`.
    pub fn pop_int(&mut self) -> Result<i64, TbxError> {
        match self.pop()? {
            Cell::Int(n) => Ok(n),
            other => Err(TbxError::TypeError {
                expected: "Int",
                got: other.type_name(),
            }),
        }
    }

    /// Pop a `Bool` value from the data stack.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the stack is empty.
    /// Returns `Err(TbxError::TypeError)` if the top value is not `Cell::Bool`.
    pub fn pop_bool(&mut self) -> Result<bool, TbxError> {
        match self.pop()? {
            Cell::Bool(b) => Ok(b),
            other => Err(TbxError::TypeError {
                expected: "Bool",
                got: other.type_name(),
            }),
        }
    }

    /// Pop a `StringDesc` value from the data stack, returning its pool index.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the stack is empty.
    /// Returns `Err(TbxError::TypeError)` if the top value is not `Cell::StringDesc`.
    pub fn pop_string_desc(&mut self) -> Result<usize, TbxError> {
        match self.pop()? {
            Cell::StringDesc(idx) => Ok(idx),
            other => Err(TbxError::TypeError {
                expected: "StringDesc",
                got: other.type_name(),
            }),
        }
    }

    /// Pop an `Xt` value from the data stack.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the stack is empty.
    /// Returns `Err(TbxError::TypeError)` if the top value is not `Cell::Xt`.
    pub fn pop_xt(&mut self) -> Result<Xt, TbxError> {
        match self.pop()? {
            Cell::Xt(xt) => Ok(xt),
            other => Err(TbxError::TypeError {
                expected: "Xt",
                got: other.type_name(),
            }),
        }
    }

    /// Pop a numeric value (`Int` or `Float`) from the data stack.
    ///
    /// Returns the cell as-is if it is `Cell::Int` or `Cell::Float`.
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::StackUnderflow)` if the stack is empty.
    /// Returns `Err(TbxError::TypeError)` if the top value is neither `Int` nor `Float`.
    pub fn pop_number(&mut self) -> Result<Cell, TbxError> {
        let cell = self.pop()?;
        match &cell {
            Cell::Int(_) | Cell::Float(_) => Ok(cell),
            other => Err(TbxError::TypeError {
                expected: "number",
                got: other.type_name(),
            }),
        }
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

    /// Read the cell at `offset` as a jump target address.
    ///
    /// Expects `Cell::Int`; returns the address as `usize`.
    ///
    /// # Errors
    ///
    /// - `Err(TbxError::IndexOutOfBounds)` if `offset` is beyond the dictionary end.
    /// - `Err(TbxError::TypeError)` if the cell at `offset` is not a `Cell::Int`.
    /// - `Err(TbxError::InvalidJumpTarget)` if the address is negative.
    fn read_jump_target(&self, offset: usize) -> Result<usize, TbxError> {
        let cell = self.dict_read(offset)?;
        let raw = cell.as_int().ok_or_else(|| TbxError::TypeError {
            expected: "Int (jump target)",
            got: cell.type_name(),
        })?;
        if raw < 0 {
            return Err(TbxError::InvalidJumpTarget { address: raw });
        }
        Ok(raw as usize)
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
        let idx = self
            .bp
            .checked_add(local_idx)
            .ok_or(TbxError::IndexOutOfBounds {
                index: usize::MAX,
                size: self.data_stack.len(),
            })?;
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
        let idx = self
            .bp
            .checked_add(local_idx)
            .ok_or(TbxError::IndexOutOfBounds {
                index: usize::MAX,
                size: self.data_stack.len(),
            })?;
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
            let dispatch_cell = self.dict_read(self.pc)?;
            let xt = dispatch_cell.as_xt().ok_or_else(|| TbxError::TypeError {
                expected: "Xt",
                got: dispatch_cell.type_name(),
            })?;
            let entry_kind = self
                .headers
                .get(xt.index())
                .ok_or(TbxError::IndexOutOfBounds {
                    index: xt.index(),
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
                    let xt_cell = self.dict_read(self.pc + 1)?;
                    let target_xt = xt_cell.as_xt().ok_or_else(|| TbxError::TypeError {
                        expected: "Xt",
                        got: xt_cell.type_name(),
                    })?;
                    let arity_cell = self.dict_read(self.pc + 2)?;
                    let arity_raw = arity_cell.as_int().ok_or_else(|| TbxError::TypeError {
                        expected: "Int (arity)",
                        got: arity_cell.type_name(),
                    })?;
                    if arity_raw < 0 {
                        return Err(TbxError::InvalidOperand {
                            name: "arity",
                            value: arity_raw,
                            reason: "must be non-negative",
                        });
                    }
                    let arity = arity_raw as usize;
                    let local_count_cell = self.dict_read(self.pc + 3)?;
                    let local_count_raw =
                        local_count_cell
                            .as_int()
                            .ok_or_else(|| TbxError::TypeError {
                                expected: "Int (local count)",
                                got: local_count_cell.type_name(),
                            })?;
                    if local_count_raw < 0 {
                        return Err(TbxError::InvalidOperand {
                            name: "local_count",
                            value: local_count_raw,
                            reason: "must be non-negative",
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
                                self.push(Cell::Int(0))?;
                            }
                            self.pc = offset;
                        }
                        _ => {
                            // CALL targets only compiled Words. Primitives, Variables, and
                            // Constants are called via direct Xt dispatch, not CALL.
                            return Err(TbxError::TypeError {
                                expected: "Word (CALL target must be a compiled word)",
                                got: "non-Word",
                            });
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
                            self.push(retval)?;
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

                EntryKind::Goto => {
                    let target = self.read_jump_target(self.pc + 1)?;
                    self.pc = target;
                }
                EntryKind::BranchIfFalse => {
                    let cond = self.pop()?;
                    let target = self.read_jump_target(self.pc + 1)?;
                    if !cond.is_truthy() {
                        self.pc = target;
                    } else {
                        self.pc += 2;
                    }
                }
                EntryKind::BranchIfTrue => {
                    let cond = self.pop()?;
                    let target = self.read_jump_target(self.pc + 1)?;
                    if cond.is_truthy() {
                        self.pc = target;
                    } else {
                        self.pc += 2;
                    }
                }

                EntryKind::Lit => {
                    self.pc += 1;
                    let literal = self.dict_read(self.pc)?;
                    self.push(literal)?;
                    self.pc += 1;
                }
                EntryKind::Variable(idx) => {
                    self.push(Cell::DictAddr(idx))?;
                    self.pc += 1;
                }
                EntryKind::Constant(ref c) => {
                    let val = c.clone();
                    self.push(val)?;
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
    /// Roll back a partially-compiled word definition on error.
    ///
    /// Restores `dp`, `headers`, and `latest` to the state captured at DEF time.
    /// Clears `is_compiling` and drops `compile_state`.
    pub(crate) fn rollback_def(&mut self) {
        if let Some(state) = self.compile_state.take() {
            self.dp = state.dp_at_def;
            self.dictionary.truncate(state.dp_at_def);
            self.headers.truncate(state.hdr_len_at_def);
            self.latest = state.saved_latest;
            self.is_compiling = false;
        }
    }

    /// Perform a definition rollback using explicitly supplied snapshot values.
    ///
    /// This is used when `compile_state` has already been taken (via `.take()`) but
    /// an error occurs afterwards — the caller must have saved `rollback_info()` before
    /// calling `.take()`.
    pub(crate) fn rollback_def_explicit(
        &mut self,
        dp_at_def: usize,
        hdr_len_at_def: usize,
        saved_latest: Option<Xt>,
    ) {
        self.dp = dp_at_def;
        self.dictionary.truncate(dp_at_def);
        self.headers.truncate(hdr_len_at_def);
        self.latest = saved_latest;
        self.compile_state = None;
        self.is_compiling = false;
    }

    /// Find the first header entry whose `kind` satisfies `pred`.
    ///
    /// Returns `Some(Xt)` for the first match, or `None` if no entry matches.
    /// Useful for locating runtime instruction entries (e.g. `Goto`, `BranchIfFalse`)
    /// that may be shadowed by IMMEDIATE primitives of the same name.
    pub(crate) fn find_by_kind(&self, pred: impl Fn(&EntryKind) -> bool) -> Option<Xt> {
        self.headers
            .iter()
            .enumerate()
            .find(|(_, e)| pred(&e.kind))
            .map(|(i, _)| Xt(i))
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
    use crate::dict::{EntryKind, WordEntry};

    fn noop(_vm: &mut VM) -> Result<(), crate::error::TbxError> {
        Ok(())
    }

    /// Find the Xt of the first header entry whose kind matches the predicate.
    /// Delegates to `VM::find_by_kind`; panics if the entry is not found.
    /// Used to locate runtime instructions (Goto, BranchIfFalse, BranchIfTrue)
    /// which may be shadowed by IMMEDIATE primitives of the same name.
    fn find_by_kind(vm: &VM, pred: impl Fn(&EntryKind) -> bool) -> Xt {
        vm.find_by_kind(pred).expect("entry not found by kind")
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
        vm.push(Cell::Int(42)).unwrap();
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
    fn test_lookup_skips_hidden_entry() {
        // FLAG_HIDDEN entries must be invisible to lookup.
        // If a system primitive and a hidden user word share a name,
        // lookup must return the system primitive.
        use crate::dict::FLAG_HIDDEN;
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        // "ADD" is already registered as a system primitive.
        let sys_xt = vm.lookup("ADD").unwrap();

        // Register a user word with the same name and smudge it.
        vm.register(WordEntry::new_word("ADD", 999));
        vm.headers.last_mut().unwrap().flags |= FLAG_HIDDEN;

        // lookup("ADD") must still return the system primitive, not the hidden entry.
        assert_eq!(vm.lookup("ADD"), Some(sys_xt));

        // After clearing FLAG_HIDDEN, the user word should shadow the primitive.
        vm.headers.last_mut().unwrap().flags &= !FLAG_HIDDEN;
        assert_ne!(vm.lookup("ADD"), Some(sys_xt));
    }

    #[test]
    fn test_lookup_including_self_finds_hidden_entry_only_for_self() {
        // lookup_including_self must return the hidden entry only when the name matches self_word.
        use crate::dict::FLAG_HIDDEN;
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        // Register two user words and smudge both.
        vm.register(WordEntry::new_word("FACT", 500));
        let fact_xt = vm.lookup("FACT").unwrap();
        vm.headers.last_mut().unwrap().flags |= FLAG_HIDDEN;

        vm.register(WordEntry::new_word("HELPER", 600));
        let helper_xt = vm.lookup("HELPER").unwrap();
        vm.headers.last_mut().unwrap().flags |= FLAG_HIDDEN;

        // Regular lookup returns None for both (both hidden).
        assert_eq!(vm.lookup("FACT"), None);
        assert_eq!(vm.lookup("HELPER"), None);

        // lookup_including_self with self_word="FACT" finds FACT but NOT HELPER.
        assert_eq!(
            vm.lookup_including_self("FACT", Some("FACT")),
            Some(fact_xt)
        );
        assert_eq!(vm.lookup_including_self("HELPER", Some("FACT")), None);

        // With self_word="HELPER", finds HELPER but NOT FACT.
        assert_eq!(
            vm.lookup_including_self("HELPER", Some("HELPER")),
            Some(helper_xt)
        );
        assert_eq!(vm.lookup_including_self("FACT", Some("HELPER")), None);

        // With self_word=None, finds neither.
        assert_eq!(vm.lookup_including_self("FACT", None), None);
        assert_eq!(vm.lookup_including_self("HELPER", None), None);
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

        vm.push(Cell::Int(99)).unwrap();
        vm.run(0).unwrap();

        // DROP should have removed the 99
        assert_eq!(vm.pop(), Err(crate::error::TbxError::StackUnderflow));
    }

    #[test]
    fn test_run_non_xt_at_pc_errors_with_type_name() {
        // Verify that when a non-Xt cell is found at the PC position,
        // TypeError.got reports the actual cell type via type_name().
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        // Place Cell::Int(42) at offset 0 instead of an Xt.
        vm.dict_write(Cell::Int(42)).unwrap();

        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::TypeError {
                expected: "Xt",
                got: "Int",
            })
        );
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

        vm.push(Cell::Int(7)).unwrap(); // argument
        vm.run(0).unwrap();

        // DUP duplicated the arg, but EXIT truncates to bp.
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

        vm.push(Cell::Int(7)).unwrap(); // argument (will be cleaned up)
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

        vm.push(Cell::Int(10)).unwrap(); // arg a
        vm.push(Cell::Int(20)).unwrap(); // arg b
        vm.run(0).unwrap();

        // args (10, 20) and local (0) should be cleaned up.
        // Only return value 42 remains.
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_call_negative_arity_returns_error() {
        // Verify that a negative arity operand in CALL returns an InvalidOperand error.
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
            Err(crate::error::TbxError::InvalidOperand {
                name: "arity",
                value: -1,
                ..
            })
        ));
    }

    #[test]
    fn test_call_negative_local_count_returns_error() {
        // Verify that a negative local_count operand in CALL returns an InvalidOperand error.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Dummy word body: just EXIT
        let word_offset = vm.dp;
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();
        let dummy_xt = vm.register(crate::dict::WordEntry::new_word("DUMMY_LC", word_offset));

        // Top-level: CALL DUMMY_LC arity=0 local_count=-1
        let start = vm.dp;
        vm.dict_write(Cell::Xt(call_xt)).unwrap();
        vm.dict_write(Cell::Xt(dummy_xt)).unwrap();
        vm.dict_write(Cell::Int(0)).unwrap(); // arity=0
        vm.dict_write(Cell::Int(-1)).unwrap(); // negative local_count

        let result = vm.run(start);
        assert!(matches!(
            result,
            Err(crate::error::TbxError::InvalidOperand {
                name: "local_count",
                value: -1,
                ..
            })
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
    fn test_call_target_xt_type_mismatch_returns_type_error() {
        // Verify that a non-Xt value at pc+1 (target_xt position) returns TypeError.
        // This confirms the error type for type mismatch is TypeError, not IndexOutOfBounds.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let call_xt = vm.lookup("CALL").unwrap();

        // Top-level: CALL <Int instead of Xt> arity=0 local_count=0
        let start = vm.dp;
        vm.dict_write(Cell::Xt(call_xt)).unwrap();
        vm.dict_write(Cell::Int(999)).unwrap(); // wrong type: Int instead of Xt
        vm.dict_write(Cell::Int(0)).unwrap();
        vm.dict_write(Cell::Int(0)).unwrap();

        let result = vm.run(start);
        assert!(
            matches!(result, Err(crate::error::TbxError::TypeError { .. })),
            "expected TypeError for non-Xt target_xt, got {:?}",
            result
        );
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

    // --- dict_read tests ---

    #[test]
    fn test_dict_read_ok() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::Int(99));
        vm.dp = 1;
        assert_eq!(vm.dict_read(0), Ok(Cell::Int(99)));
    }

    #[test]
    fn test_dict_read_out_of_bounds() {
        let vm = VM::new();
        let result = vm.dict_read(0);
        assert!(
            matches!(result, Err(crate::error::TbxError::IndexOutOfBounds { .. })),
            "expected IndexOutOfBounds, got {:?}",
            result
        );
    }

    // --- dict_write_at tests ---

    #[test]
    fn test_dict_write_at_ok() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::Int(0));
        vm.dp = 1;
        assert!(vm.dict_write_at(0, Cell::Int(42)).is_ok());
        assert_eq!(vm.dictionary[0], Cell::Int(42));
    }

    #[test]
    fn test_dict_write_at_out_of_bounds() {
        let mut vm = VM::new();
        let result = vm.dict_write_at(0, Cell::Int(42));
        assert!(
            matches!(result, Err(crate::error::TbxError::IndexOutOfBounds { .. })),
            "expected IndexOutOfBounds, got {:?}",
            result
        );
    }

    // --- local_read tests ---

    #[test]
    fn test_local_read_ok() {
        let mut vm = VM::new();
        vm.data_stack.push(Cell::Int(10));
        vm.data_stack.push(Cell::Int(20));
        vm.bp = 1;
        assert_eq!(vm.local_read(0), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_local_read_out_of_bounds() {
        let mut vm = VM::new();
        vm.bp = 0;
        let result = vm.local_read(0);
        assert!(
            matches!(result, Err(crate::error::TbxError::IndexOutOfBounds { .. })),
            "expected IndexOutOfBounds, got {:?}",
            result
        );
    }

    #[test]
    fn test_local_read_overflow() {
        let mut vm = VM::new();
        vm.bp = usize::MAX;
        let result = vm.local_read(1);
        assert!(
            matches!(result, Err(crate::error::TbxError::IndexOutOfBounds { .. })),
            "expected IndexOutOfBounds on overflow, got {:?}",
            result
        );
    }

    // --- local_write tests ---

    #[test]
    fn test_local_write_ok() {
        let mut vm = VM::new();
        vm.data_stack.push(Cell::Int(0));
        vm.bp = 0;
        assert!(vm.local_write(0, Cell::Int(55)).is_ok());
        assert_eq!(vm.data_stack[0], Cell::Int(55));
    }

    #[test]
    fn test_local_write_out_of_bounds() {
        let mut vm = VM::new();
        vm.bp = 0;
        let result = vm.local_write(0, Cell::Int(55));
        assert!(
            matches!(result, Err(crate::error::TbxError::IndexOutOfBounds { .. })),
            "expected IndexOutOfBounds, got {:?}",
            result
        );
    }

    #[test]
    fn test_local_write_overflow() {
        let mut vm = VM::new();
        vm.bp = usize::MAX;
        let result = vm.local_write(1, Cell::Int(55));
        assert!(
            matches!(result, Err(crate::error::TbxError::IndexOutOfBounds { .. })),
            "expected IndexOutOfBounds on overflow, got {:?}",
            result
        );
    }

    // --- pop_int tests ---

    #[test]
    fn test_pop_int_ok() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        assert_eq!(vm.pop_int(), Ok(42));
    }

    #[test]
    fn test_pop_int_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            vm.pop_int(),
            Err(crate::error::TbxError::StackUnderflow)
        ));
    }

    #[test]
    fn test_pop_int_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            vm.pop_int(),
            Err(crate::error::TbxError::TypeError { .. })
        ));
    }

    // --- pop_bool tests ---

    #[test]
    fn test_pop_bool_ok() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert_eq!(vm.pop_bool(), Ok(true));
    }

    #[test]
    fn test_pop_bool_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(
            vm.pop_bool(),
            Err(crate::error::TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_pop_bool_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            vm.pop_bool(),
            Err(crate::error::TbxError::StackUnderflow)
        ));
    }

    // --- pop_string_desc tests ---

    #[test]
    fn test_pop_string_desc_ok() {
        let mut vm = VM::new();
        vm.push(Cell::StringDesc(7)).unwrap();
        assert_eq!(vm.pop_string_desc(), Ok(7));
    }

    #[test]
    fn test_pop_string_desc_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            vm.pop_string_desc(),
            Err(crate::error::TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_pop_string_desc_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            vm.pop_string_desc(),
            Err(crate::error::TbxError::StackUnderflow)
        ));
    }

    // --- pop_xt tests ---

    #[test]
    fn test_pop_xt_ok() {
        let mut vm = VM::new();
        vm.push(Cell::Xt(Xt(3))).unwrap();
        assert_eq!(vm.pop_xt(), Ok(Xt(3)));
    }

    #[test]
    fn test_pop_xt_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3)).unwrap();
        assert!(matches!(
            vm.pop_xt(),
            Err(crate::error::TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_pop_xt_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            vm.pop_xt(),
            Err(crate::error::TbxError::StackUnderflow)
        ));
    }
    // --- pop_number tests ---

    #[test]
    fn test_pop_number_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        assert_eq!(vm.pop_number(), Ok(Cell::Int(10)));
    }

    #[test]
    fn test_pop_number_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        assert_eq!(vm.pop_number(), Ok(Cell::Float(2.5)));
    }

    #[test]
    fn test_pop_number_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false)).unwrap();
        assert!(matches!(
            vm.pop_number(),
            Err(crate::error::TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_pop_number_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            vm.pop_number(),
            Err(crate::error::TbxError::StackUnderflow)
        ));
    }

    // --- integration tests ---

    #[test]
    fn test_run_putdec_outputs_number() {
        // Verify that a program of [LIT 42 PUTDEC EXIT] writes "42" to the output buffer.
        // Uses init_vm() to also validate the full initialization (register_all + seal_sys).
        let mut vm = crate::init_vm();

        let lit_xt = vm.lookup("LIT").unwrap();
        let putdec_xt = vm.lookup("PUTDEC").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        let start = vm.dp;
        vm.dict_write(Cell::Xt(lit_xt)).unwrap();
        vm.dict_write(Cell::Int(42)).unwrap();
        vm.dict_write(Cell::Xt(putdec_xt)).unwrap();
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();

        vm.run(start).unwrap();

        assert_eq!(vm.take_output(), "42");
        assert_eq!(vm.pop(), Err(crate::error::TbxError::StackUnderflow));
    }

    #[test]
    fn test_data_stack_overflow() {
        // Verify that pushing beyond MAX_DATA_STACK_DEPTH returns DataStackOverflow.
        use crate::constants::MAX_DATA_STACK_DEPTH;
        let mut vm = VM::new();
        vm.data_stack.resize(MAX_DATA_STACK_DEPTH, Cell::Int(0));
        let result = vm.push(Cell::Int(0));
        assert!(
            matches!(
                result,
                Err(crate::error::TbxError::DataStackOverflow {
                    depth,
                    limit
                }) if depth == MAX_DATA_STACK_DEPTH && limit == MAX_DATA_STACK_DEPTH
            ),
            "expected DataStackOverflow, got {:?}",
            result
        );
    }

    #[test]
    fn test_call_non_word_target_rejected() {
        // CALL must only target compiled Words; Primitive/Variable/Constant must be rejected.
        use crate::dict::{EntryKind, WordEntry};

        fn try_call(kind: EntryKind) -> Result<(), crate::error::TbxError> {
            let mut vm = VM::new();
            crate::primitives::register_all(&mut vm);
            let call_xt = vm.lookup("CALL").unwrap();
            let exit_xt = vm.lookup("EXIT").unwrap();
            let target_xt = vm.register(WordEntry {
                name: "TARGET".to_string(),
                flags: 0,
                kind,
                local_count: 0,
                prev: None,
            });
            vm.dict_write(Cell::Xt(call_xt)).unwrap();
            vm.dict_write(Cell::Xt(target_xt)).unwrap();
            vm.dict_write(Cell::Int(0)).unwrap(); // arity=0
            vm.dict_write(Cell::Int(0)).unwrap(); // local_count=0
            vm.dict_write(Cell::Xt(exit_xt)).unwrap();
            vm.run(0)
        }

        fn dummy(_vm: &mut VM) -> Result<(), crate::error::TbxError> {
            Ok(())
        }

        // Primitive
        assert!(
            matches!(
                try_call(EntryKind::Primitive(dummy)),
                Err(crate::error::TbxError::TypeError { .. })
            ),
            "expected TypeError for Primitive CALL target"
        );

        // Variable
        assert!(
            matches!(
                try_call(EntryKind::Variable(0)),
                Err(crate::error::TbxError::TypeError { .. })
            ),
            "expected TypeError for Variable CALL target"
        );

        // Constant
        assert!(
            matches!(
                try_call(EntryKind::Constant(Cell::Int(42))),
                Err(crate::error::TbxError::TypeError { .. })
            ),
            "expected TypeError for Constant CALL target"
        );
    }

    // --- GOTO / BIF / BIT ---

    #[test]
    fn test_run_goto() {
        // Verify that GOTO jumps unconditionally to the specified offset.
        //
        // Layout:
        //   [0] Xt(GOTO)     <- GOTO instruction
        //   [1] Int(3)       <- target address = 3
        //   [2] Xt(DUP)      <- should be skipped
        //   [3] Xt(EXIT)     <- landing point; exits immediately
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let goto_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::Goto));
        let dup_xt = vm.lookup("DUP").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(goto_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(3)).unwrap(); // [1] target = 3
        vm.dict_write(Cell::Xt(dup_xt)).unwrap(); // [2] skipped
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [3]

        vm.run(0).unwrap();

        // DUP was skipped, so stack must be empty.
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_run_bif_taken() {
        // BIF: when condition is falsy, branch is taken (jumps to target).
        //
        // Layout:
        //   [0] Xt(BIF)      <- BIF instruction
        //   [1] Int(4)       <- target address = 4
        //   [2] Xt(DUP)      <- fall-through path (should be skipped)
        //   [3] Xt(EXIT)     <- fall-through exit (should be skipped)
        //   [4] Xt(EXIT)     <- branch target; exits immediately
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bif_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfFalse));
        let dup_xt = vm.lookup("DUP").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(bif_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(4)).unwrap(); // [1] target = 4
        vm.dict_write(Cell::Xt(dup_xt)).unwrap(); // [2] skipped
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [3] skipped
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4] branch target

        // Push falsy condition; BIF should branch.
        vm.push(Cell::Bool(false)).unwrap();
        vm.run(0).unwrap();

        // DUP was skipped, so stack must be empty.
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_run_bif_not_taken() {
        // BIF: when condition is truthy, fall-through occurs (no jump).
        //
        // Layout:
        //   [0] Xt(BIF)      <- BIF instruction
        //   [1] Int(5)       <- target address (not taken)
        //   [2] Xt(LIT)      <- fall-through: push Int(42)
        //   [3] Int(42)
        //   [4] Xt(EXIT)
        //   [5] Xt(EXIT)     <- branch target (not reached)
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bif_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfFalse));
        let lit_xt = vm.lookup("LIT").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(bif_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(5)).unwrap(); // [1] target = 5 (not taken)
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [2]
        vm.dict_write(Cell::Int(42)).unwrap(); // [3]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [5] not reached

        // Push truthy condition; BIF should fall through.
        vm.push(Cell::Bool(true)).unwrap();
        vm.run(0).unwrap();

        // Fall-through pushed 42 onto the stack.
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
    }

    #[test]
    fn test_run_bit_taken() {
        // BIT: when condition is truthy, branch is taken (jumps to target).
        //
        // Layout:
        //   [0] Xt(BIT)      <- BIT instruction
        //   [1] Int(4)       <- target address = 4
        //   [2] Xt(DUP)      <- fall-through path (should be skipped)
        //   [3] Xt(EXIT)     <- fall-through exit (should be skipped)
        //   [4] Xt(EXIT)     <- branch target; exits immediately
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bit_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfTrue));
        let dup_xt = vm.lookup("DUP").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(bit_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(4)).unwrap(); // [1] target = 4
        vm.dict_write(Cell::Xt(dup_xt)).unwrap(); // [2] skipped
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [3] skipped
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4] branch target

        // Push truthy condition; BIT should branch.
        vm.push(Cell::Bool(true)).unwrap();
        vm.run(0).unwrap();

        // DUP was skipped, so stack must be empty.
        assert!(vm.data_stack.is_empty());
    }

    #[test]
    fn test_run_bit_not_taken() {
        // BIT: when condition is falsy, fall-through occurs (no jump).
        //
        // Layout:
        //   [0] Xt(BIT)      <- BIT instruction
        //   [1] Int(5)       <- target address (not taken)
        //   [2] Xt(LIT)      <- fall-through: push Int(42)
        //   [3] Int(42)
        //   [4] Xt(EXIT)
        //   [5] Xt(EXIT)     <- branch target (not reached)
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bit_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfTrue));
        let lit_xt = vm.lookup("LIT").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(bit_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(5)).unwrap(); // [1] target = 5 (not taken)
        vm.dict_write(Cell::Xt(lit_xt)).unwrap(); // [2]
        vm.dict_write(Cell::Int(42)).unwrap(); // [3]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [4]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [5] not reached

        // Push falsy condition; BIT should fall through.
        vm.push(Cell::Bool(false)).unwrap();
        vm.run(0).unwrap();

        // Fall-through pushed 42 onto the stack.
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
    }

    // --- error cases for GOTO/BIF/BIT ---

    #[test]
    fn test_run_goto_negative_target_errors() {
        // GOTO with a negative target address must return InvalidJumpTarget.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let goto_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::Goto));

        vm.dict_write(Cell::Xt(goto_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(-1)).unwrap(); // [1] negative target

        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::InvalidJumpTarget { address: -1 })
        );
    }

    #[test]
    fn test_run_goto_non_int_target_errors() {
        // GOTO with a non-Int operand (Cell::Xt) must return TypeError with got = "Xt".
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let goto_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::Goto));
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(goto_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [1] Xt instead of Int

        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::TypeError {
                expected: "Int (jump target)",
                got: "Xt",
            })
        );
    }

    #[test]
    fn test_run_goto_out_of_bounds_target_errors() {
        // GOTO with a target beyond the dictionary end must return IndexOutOfBounds.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let goto_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::Goto));

        vm.dict_write(Cell::Xt(goto_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(9999)).unwrap(); // [1] out-of-bounds target

        let result = vm.run(0);
        assert!(matches!(
            result,
            Err(crate::error::TbxError::IndexOutOfBounds { .. })
        ));
    }

    #[test]
    fn test_run_bif_empty_stack_errors() {
        // BIF with an empty stack must return StackUnderflow.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bif_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfFalse));
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(bif_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(3)).unwrap(); // [1] target
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [2] (not reached)

        // No condition pushed — expect StackUnderflow.
        let result = vm.run(0);
        assert_eq!(result, Err(crate::error::TbxError::StackUnderflow));
    }

    #[test]
    fn test_run_bit_empty_stack_errors() {
        // BIT with an empty stack must return StackUnderflow.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bit_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfTrue));
        let exit_xt = vm.lookup("EXIT").unwrap();

        vm.dict_write(Cell::Xt(bit_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(3)).unwrap(); // [1] target
        vm.dict_write(Cell::Xt(exit_xt)).unwrap(); // [2] (not reached)

        // No condition pushed — expect StackUnderflow.
        let result = vm.run(0);
        assert_eq!(result, Err(crate::error::TbxError::StackUnderflow));
    }

    #[test]
    fn test_run_bif_negative_target_errors() {
        // BIF with a negative target address must return InvalidJumpTarget.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bif_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfFalse));

        vm.dict_write(Cell::Xt(bif_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(-5)).unwrap(); // [1] negative target

        vm.push(Cell::Bool(false)).unwrap(); // falsy → branch would be taken
        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::InvalidJumpTarget { address: -5 })
        );
    }

    #[test]
    fn test_run_bit_negative_target_errors() {
        // BIT with a negative target address must return InvalidJumpTarget.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bit_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfTrue));

        vm.dict_write(Cell::Xt(bit_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(-3)).unwrap(); // [1] negative target

        vm.push(Cell::Bool(true)).unwrap(); // truthy → branch would be taken
        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::InvalidJumpTarget { address: -3 })
        );
    }

    #[test]
    fn test_run_bif_negative_target_errors_on_fallthrough() {
        // BIF with a negative target address must return InvalidJumpTarget
        // even when the condition is truthy (fall-through path).
        // read_jump_target is always evaluated regardless of the condition.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bif_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfFalse));

        vm.dict_write(Cell::Xt(bif_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(-1)).unwrap(); // [1] negative target

        vm.push(Cell::Bool(true)).unwrap(); // truthy → fall-through would be taken
        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::InvalidJumpTarget { address: -1 })
        );
    }

    #[test]
    fn test_run_bit_negative_target_errors_on_fallthrough() {
        // BIT with a negative target address must return InvalidJumpTarget
        // even when the condition is falsy (fall-through path).
        // read_jump_target is always evaluated regardless of the condition.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let bit_xt = find_by_kind(&vm, |k| matches!(k, EntryKind::BranchIfTrue));

        vm.dict_write(Cell::Xt(bit_xt)).unwrap(); // [0]
        vm.dict_write(Cell::Int(-2)).unwrap(); // [1] negative target

        vm.push(Cell::Bool(false)).unwrap(); // falsy → fall-through would be taken
        let result = vm.run(0);
        assert_eq!(
            result,
            Err(crate::error::TbxError::InvalidJumpTarget { address: -2 })
        );
    }

    // ── next_token() tests ────────────────────────────────────────────────────

    fn make_spanned(token: crate::lexer::Token) -> crate::lexer::SpannedToken {
        crate::lexer::SpannedToken {
            token,
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 1,
        }
    }

    #[test]
    fn test_next_token_none_stream() {
        // token_stream is None → TokenStreamEmpty
        let mut vm = VM::new();
        assert_eq!(
            vm.next_token(),
            Err(crate::error::TbxError::TokenStreamEmpty)
        );
    }

    #[test]
    fn test_next_token_empty_stream() {
        // token_stream is an empty VecDeque → TokenStreamEmpty
        let mut vm = VM::new();
        vm.token_stream = Some(VecDeque::new());
        assert_eq!(
            vm.next_token(),
            Err(crate::error::TbxError::TokenStreamEmpty)
        );
    }

    #[test]
    fn test_next_token_single_token() {
        // Providing one token returns Ok(token)
        let mut vm = VM::new();
        let tok = make_spanned(crate::lexer::Token::Ident("HELLO".to_string()));
        vm.token_stream = Some(VecDeque::from([tok.clone()]));
        assert_eq!(vm.next_token(), Ok(tok));
    }

    #[test]
    fn test_next_token_exhausted_after_consume() {
        // After consuming the single token, the next call returns TokenStreamEmpty
        let mut vm = VM::new();
        let tok = make_spanned(crate::lexer::Token::Ident("X".to_string()));
        vm.token_stream = Some(VecDeque::from([tok]));
        let _ = vm.next_token(); // consume
        assert_eq!(
            vm.next_token(),
            Err(crate::error::TbxError::TokenStreamEmpty)
        );
    }

    #[test]
    fn test_next_token_fifo_order() {
        // Two tokens are returned in FIFO (push-back / pop-front) order
        let mut vm = VM::new();
        let tok1 = make_spanned(crate::lexer::Token::IntLit(1));
        let tok2 = make_spanned(crate::lexer::Token::IntLit(2));
        vm.token_stream = Some(VecDeque::from([tok1.clone(), tok2.clone()]));
        assert_eq!(vm.next_token(), Ok(tok1));
        assert_eq!(vm.next_token(), Ok(tok2));
    }
}
