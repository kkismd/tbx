//! Outer interpreter: tokenizes source text and executes statements via the inner interpreter.

use std::collections::HashMap;

use crate::cell::{Cell, Xt};
use crate::dict::FLAG_SYSTEM;
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::init_vm;
use crate::lexer::{Lexer, SpannedToken, Token};
use crate::vm::VM;

#[allow(dead_code)]
struct CompileState {
    /// Name of the word being compiled.
    word_name: String,
    /// Dictionary pointer at the start of DEF (for rollback on error).
    dp_at_def: usize,
    /// Header count at the start of DEF (for rollback on error).
    hdr_len_at_def: usize,
    saved_latest: Option<crate::cell::Xt>,
    /// Local variable table: maps variable name to StackAddr index.
    /// Parameters are assigned indices 0..arity, VAR locals start at arity.
    local_table: HashMap<String, usize>,
    /// Number of formal parameters parsed from DEF WORD(X, Y, ...).
    arity: usize,
    /// Number of VAR-declared local variables encountered so far.
    local_count: usize,
    /// Dictionary offsets of the `local_count` placeholder (Int(0)) in CALL instructions
    /// that refer to the currently-compiled word (self-recursive calls).
    /// Patched to the final `local_count` when END is compiled.
    call_patch_list: Vec<usize>,
    /// Maps line-number label to dictionary offset recorded when the label was seen.
    label_table: HashMap<i64, usize>,
    /// (label_number, dict_offset_of_placeholder) waiting to be back-patched.
    patch_list: Vec<(i64, usize)>,
}

/// Error produced by the outer interpreter, including source location information.
pub struct InterpreterError {
    pub line: usize,
    pub col: usize,
    pub source_line: String,
    pub kind: TbxError,
}

impl InterpreterError {
    /// Construct a new `InterpreterError` with the given location and error kind.
    fn new(line: usize, col: usize, source_line: &str, kind: TbxError) -> Self {
        InterpreterError {
            line,
            col,
            source_line: source_line.to_string(),
            kind,
        }
    }
}

impl std::fmt::Debug for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}:{}: {:?}\n  {}",
            self.line, self.col, self.kind, self.source_line
        )
    }
}

impl std::fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.col, self.kind)
    }
}

/// The TBX outer interpreter.
///
/// Processes source text line by line, compiling and executing each statement
/// via the inner interpreter (`vm.run()`).
///
// TODO(#144): HEADER primitive needs to consume tokens from the outer interpreter.
// Future design: expose a token feed mechanism so that primitives registered in
// the dictionary can read the next token from the current input stream
// (e.g., via a VM-level pending token buffer).
pub struct Interpreter {
    vm: VM,
    compile_state: Option<CompileState>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    /// Create a new `Interpreter` backed by a fully initialized VM.
    pub fn new() -> Self {
        Self {
            vm: init_vm(),
            compile_state: None,
        }
    }

    /// Look up a required symbol by name, returning an `InterpreterError` if not found.
    fn lookup_required(
        &self,
        name: &str,
        line: usize,
        col: usize,
        source_line: &str,
    ) -> Result<Xt, InterpreterError> {
        self.vm.lookup(name).ok_or_else(|| {
            InterpreterError::new(
                line,
                col,
                source_line,
                TbxError::UndefinedSymbol {
                    name: name.to_string(),
                },
            )
        })
    }

    /// Execute a single source line.
    ///
    /// Tokenizes `line`, resolves the statement word, builds a temporary code buffer,
    /// and runs it through the inner interpreter.
    ///
    /// Returns `Ok(())` on success, or an `InterpreterError` containing position
    /// and error details on failure.
    pub fn exec_line(&mut self, line: &str) -> Result<(), InterpreterError> {
        let mut lex = Lexer::new(line);

        // Collect all tokens on this line.
        let mut tokens: Vec<SpannedToken> = Vec::new();
        loop {
            let st = lex.next_token();
            match &st.token {
                Token::Newline | Token::Eof => break,
                _ => tokens.push(st),
            }
        }

        if tokens.is_empty() {
            return Ok(());
        }

        let mut idx = 0;

        // Skip optional line number; in compile mode, register it as a label.
        if let Token::LineNum(n) = tokens[idx].token {
            if self.compile_state.is_some() {
                let label_n = n;
                let ln_line = tokens[idx].pos.line;
                let ln_col = tokens[idx].pos.col;
                self.register_label(label_n, line, ln_line, ln_col)
                    .inspect_err(|_e| {
                        self.rollback_def();
                    })?;
            }
            idx += 1;
            if idx >= tokens.len() {
                return Ok(());
            }
        }

        // Extract statement name.
        let stmt_tok = &tokens[idx];
        let stmt_name = match &stmt_tok.token {
            Token::Ident(name) => name.clone(),
            _ => return Ok(()), // not an identifier — skip
        };
        let stmt_pos_line = stmt_tok.pos.line;
        let stmt_pos_col = stmt_tok.pos.col;
        idx += 1;

        // Handle REM: skip the rest of the line.
        if stmt_name.eq_ignore_ascii_case("REM") {
            return Ok(());
        }

        // Handle DEF: begin compiling a new word.
        if stmt_name.eq_ignore_ascii_case("DEF") {
            return self.handle_def(&tokens[idx..], line, stmt_pos_line, stmt_pos_col);
        }

        // Handle END: finish compiling the current word.
        if stmt_name.eq_ignore_ascii_case("END") && self.compile_state.is_some() {
            return self.handle_end(line, stmt_pos_line, stmt_pos_col);
        }

        // Handle VAR in compile mode: register a local variable (no code emitted).
        if stmt_name.eq_ignore_ascii_case("VAR") && self.compile_state.is_some() {
            return self.handle_var(&tokens[idx..], line, stmt_pos_line, stmt_pos_col);
        }

        // Handle VAR at top level: declare a global variable (allocates a dictionary slot).
        if stmt_name.eq_ignore_ascii_case("VAR") && self.compile_state.is_none() {
            return self.handle_global_var(&tokens[idx..], line, stmt_pos_line, stmt_pos_col);
        }

        // If we are in compile mode, write this statement to the dictionary instead of executing it.
        if self.compile_state.is_some() {
            // Handle GOTO in compile mode: emit Xt(GOTO) Int(target).
            if stmt_name.eq_ignore_ascii_case("GOTO") {
                let result = self.compile_goto(&tokens[idx..], line, stmt_pos_line, stmt_pos_col);
                if result.is_err() {
                    self.rollback_def();
                }
                return result;
            }

            // Handle BIF in compile mode: branch if false.
            if stmt_name.eq_ignore_ascii_case("BIF") {
                let result =
                    self.compile_branch(false, &tokens[idx..], line, stmt_pos_line, stmt_pos_col);
                if result.is_err() {
                    self.rollback_def();
                }
                return result;
            }

            // Handle BIT in compile mode: branch if true.
            if stmt_name.eq_ignore_ascii_case("BIT") {
                let result =
                    self.compile_branch(true, &tokens[idx..], line, stmt_pos_line, stmt_pos_col);
                if result.is_err() {
                    self.rollback_def();
                }
                return result;
            }

            // Handle RETURN in compile mode: emit EXIT (void) or [expr] RETURN_VAL.
            if stmt_name.eq_ignore_ascii_case("RETURN") {
                let result = self.compile_return(&tokens[idx..], line, stmt_pos_line, stmt_pos_col);
                if result.is_err() {
                    self.rollback_def();
                }
                return result;
            }

            let result = self.write_stmt_to_dict(
                &stmt_name,
                &tokens[idx..],
                stmt_pos_line,
                stmt_pos_col,
                line,
            );
            if result.is_err() {
                self.rollback_def();
            }
            return result;
        }

        // Helper closure for wrapping TbxError into InterpreterError.
        let make_err = |e: TbxError| InterpreterError::new(stmt_pos_line, stmt_pos_col, line, e);

        // Save the current dictionary pointer to use as the buffer start.
        let buf_start = self.vm.dp;

        // Write statement and arguments to the dictionary (LIT_MARKER … DROP_TO_MARKER).
        // On failure, reset dp so subsequent exec_line calls start from a clean state.
        if let Err(e) = self.write_stmt_to_dict(
            &stmt_name,
            &tokens[idx..],
            stmt_pos_line,
            stmt_pos_col,
            line,
        ) {
            self.vm.dp = buf_start;
            self.vm.dictionary.truncate(buf_start);
            return Err(e);
        }

        // Append EXIT to terminate the temporary code buffer.
        // On failure, reset dp before returning.
        let exit_xt = match self.lookup_required("EXIT", stmt_pos_line, stmt_pos_col, line) {
            Ok(xt) => xt,
            Err(e) => {
                self.vm.dp = buf_start;
                self.vm.dictionary.truncate(buf_start);
                return Err(e);
            }
        };
        if let Err(e) = self.vm.dict_write(Cell::Xt(exit_xt)) {
            self.vm.dp = buf_start;
            self.vm.dictionary.truncate(buf_start);
            return Err(make_err(e));
        }

        // Save VM state snapshots for rollback on error.
        let saved_data_stack_len = self.vm.data_stack.len();
        let saved_return_stack_len = self.vm.return_stack.len();
        let saved_bp = self.vm.bp;

        // Execute the temporary buffer.
        let run_result = self.vm.run(buf_start);

        // Reset the dictionary pointer to discard the temporary buffer.
        self.vm.dp = buf_start;
        self.vm.dictionary.truncate(buf_start);

        // On error, restore stacks and bp to their pre-run state so that
        // subsequent exec_line calls start from a clean VM state.
        if run_result.is_err() {
            self.vm.data_stack.truncate(saved_data_stack_len);
            self.vm.return_stack.truncate(saved_return_stack_len);
            self.vm.bp = saved_bp;
        }

        run_result.map_err(make_err)
    }

    fn handle_def(
        &mut self,
        arg_tokens: &[SpannedToken],
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        if self.compile_state.is_some() {
            return Err(make_err(TbxError::InvalidExpression {
                reason: "nested DEF is not allowed",
            }));
        }

        // Next token must be the word name.
        let name = match arg_tokens.first() {
            Some(st) => match &st.token {
                Token::Ident(n) => n.clone(),
                _ => {
                    return Err(make_err(TbxError::InvalidExpression {
                        reason: "expected word name after DEF",
                    }))
                }
            },
            None => {
                return Err(make_err(TbxError::InvalidExpression {
                    reason: "expected word name after DEF",
                }))
            }
        };

        // Parse optional formal parameter list: DEF WORD(X, Y, ...)
        let mut local_table: HashMap<String, usize> = HashMap::new();
        let mut arity: usize = 0;
        let rest = &arg_tokens[1..];
        if rest
            .first()
            .map(|st| matches!(st.token, Token::LParen))
            .unwrap_or(false)
        {
            let mut idx = 1; // skip '('
            while idx < rest.len() {
                match &rest[idx].token {
                    Token::RParen => {
                        break;
                    }
                    Token::Ident(param) => {
                        local_table.insert(param.clone(), arity);
                        arity += 1;
                        idx += 1;
                        // Skip optional comma.
                        if rest
                            .get(idx)
                            .map(|st| matches!(st.token, Token::Comma))
                            .unwrap_or(false)
                        {
                            idx += 1;
                        }
                    }
                    _ => {
                        return Err(make_err(TbxError::InvalidExpression {
                            reason: "expected identifier or ')' in parameter list",
                        }))
                    }
                }
            }
        }

        // Snapshot for rollback.
        let dp_at_def = self.vm.dp;
        let hdr_len_at_def = self.vm.headers.len();

        let saved_latest = self.vm.latest;
        // Register the new word immediately (forward calls within the body will resolve).
        let entry = crate::dict::WordEntry::new_word(&name, self.vm.dp);
        self.vm.register(entry);
        // Smudge: hide the word from lookup until END completes, so that operator primitives
        // with the same name (e.g. ADD, MUL) are not shadowed during body compilation.
        self.vm.headers[hdr_len_at_def].flags |= crate::dict::FLAG_HIDDEN;

        self.vm.is_compiling = true;
        self.compile_state = Some(CompileState {
            word_name: name,
            dp_at_def,
            hdr_len_at_def,
            saved_latest,
            local_table,
            arity,
            local_count: 0,
            call_patch_list: Vec::new(),
            label_table: HashMap::new(),
            patch_list: Vec::new(),
        });

        Ok(())
    }

    fn handle_end(
        &mut self,
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        // Write EXIT to terminate the word body.
        let exit_xt = self.lookup_required("EXIT", line, col, source_line)?;
        self.vm.dict_write(Cell::Xt(exit_xt)).map_err(&make_err)?;

        // Check for unresolved forward label references BEFORE taking compile_state,
        // so that rollback_def() can still work if an error is detected.
        if let Some(state) = &self.compile_state {
            if let Some(&(label, _)) = state.patch_list.first() {
                self.rollback_def();
                return Err(make_err(TbxError::UndefinedLabel { label }));
            }
        }

        // Take the compile state to get local_count and patch list.
        let state = self
            .compile_state
            .take()
            .expect("compile_state must be Some in handle_end");

        // Patch all self-recursive CALL instructions with the confirmed local_count.
        for &pos in &state.call_patch_list {
            self.vm
                .dict_write_at(pos, Cell::Int(state.local_count as i64))
                .map_err(&make_err)?;
        }

        // Update the word header with the confirmed local_count and unsmudge (make visible).
        // The word was registered as the last entry at hdr_len_at_def.
        let word_hdr_idx = state.hdr_len_at_def;
        if word_hdr_idx < self.vm.headers.len() {
            self.vm.headers[word_hdr_idx].local_count = state.local_count;
            // Unsmudge: clear FLAG_HIDDEN so the word is now visible to lookup.
            self.vm.headers[word_hdr_idx].flags &= !crate::dict::FLAG_HIDDEN;
        }

        // Seal user-defined space.
        self.vm.seal_user();

        self.vm.is_compiling = false;

        Ok(())
    }

    fn rollback_def(&mut self) {
        if let Some(state) = &self.compile_state.take() {
            self.vm.dp = state.dp_at_def;
            self.vm.dictionary.truncate(state.dp_at_def);
            self.vm.headers.truncate(state.hdr_len_at_def);
            self.vm.latest = state.saved_latest;
            self.vm.is_compiling = false;
        }
    }

    /// Register a local variable declared with `VAR name` during compile mode.
    ///
    /// Adds `name` to the local variable table and increments `local_count`.
    /// No code is emitted to the dictionary.
    fn handle_var(
        &mut self,
        arg_tokens: &[SpannedToken],
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        let name = match arg_tokens.first() {
            Some(st) => match &st.token {
                Token::Ident(n) => n.clone(),
                _ => {
                    return Err(make_err(TbxError::InvalidExpression {
                        reason: "expected variable name after VAR",
                    }))
                }
            },
            None => {
                return Err(make_err(TbxError::InvalidExpression {
                    reason: "expected variable name after VAR",
                }))
            }
        };

        let state = self
            .compile_state
            .as_mut()
            .expect("handle_var called outside compile mode");
        let idx = state.arity + state.local_count;
        state.local_table.insert(name, idx);
        state.local_count += 1;

        Ok(())
    }

    /// Declare a global variable at the top level.
    ///
    /// Allocates one cell in the dictionary as storage and registers a
    /// `Variable` header entry. The initial value is `Cell::None`.
    fn handle_global_var(
        &mut self,
        arg_tokens: &[SpannedToken],
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        let name = match arg_tokens.first() {
            Some(st) => match &st.token {
                Token::Ident(n) => n.clone(),
                _ => {
                    return Err(make_err(TbxError::InvalidExpression {
                        reason: "expected variable name after VAR",
                    }))
                }
            },
            None => {
                return Err(make_err(TbxError::InvalidExpression {
                    reason: "expected variable name after VAR",
                }))
            }
        };

        let storage_idx = self.vm.dp;
        self.vm.dict_write(Cell::None).map_err(&make_err)?;
        let entry = crate::dict::WordEntry::new_variable(&name, storage_idx);
        self.vm.register(entry);
        // Seal so that FORGET does not roll back the storage cell.
        self.vm.seal_user();

        Ok(())
    }

    /// Write a single statement and its arguments to the dictionary.
    ///
    /// Emits: `LIT_MARKER [arg_cells] (CALL stmt arity local_count | stmt) DROP_TO_MARKER`
    ///
    /// This is used both during interpretation (followed by `EXIT` + run) and during
    /// compilation (within a DEF body; `EXIT` is written by `handle_end`).
    fn write_stmt_to_dict(
        &mut self,
        stmt_name: &str,
        arg_tokens: &[SpannedToken],
        err_line: usize,
        err_col: usize,
        source_line: &str,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(err_line, err_col, source_line, e);

        // Look up the statement word.
        // Allow resolving the currently-compiled word (FLAG_HIDDEN) so that self-recursive
        // statement calls work. Other hidden words remain invisible.
        let self_word_opt = self.compile_state.as_ref().map(|s| s.word_name.as_str());
        let stmt_xt = self
            .vm
            .lookup_including_self(stmt_name, self_word_opt)
            .ok_or_else(|| {
                make_err(TbxError::UndefinedSymbol {
                    name: stmt_name.to_string(),
                })
            })?;

        // Reject system-internal words from user code.
        let stmt_flags = self.vm.headers[stmt_xt.index()].flags;
        if stmt_flags & FLAG_SYSTEM != 0 {
            return Err(make_err(TbxError::UndefinedSymbol {
                name: stmt_name.to_string(),
            }));
        }

        // Compile the argument expression to a cell sequence.
        // Local variables in the current compile scope shadow globals (local_table checked first).
        let (arg_cells, expr_patch_offsets) = {
            let local_table_opt: Option<&HashMap<String, usize>> =
                self.compile_state.as_ref().map(|s| &s.local_table);
            let self_word = self.compile_state.as_ref().map(|s| s.word_name.clone());
            let self_hdr_idx = self.compile_state.as_ref().map(|s| s.hdr_len_at_def);
            let mut compiler =
                ExprCompiler::with_context(&mut self.vm, local_table_opt, self_word, self_hdr_idx);
            let cells = compiler.compile_expr(arg_tokens).map_err(&make_err)?;
            let offsets = std::mem::take(&mut compiler.patch_offsets);
            (cells, offsets)
        };

        // Determine arity from top-level comma count.
        let arity = count_top_level_arity(arg_tokens).map_err(&make_err)?;

        // Check whether the statement is a compiled word (needs CALL with arity/locals)
        // or a primitive/other (called directly by placing Xt in the code stream).
        let stmt_is_word = matches!(
            self.vm.headers[stmt_xt.index()].kind,
            crate::dict::EntryKind::Word(_)
        );

        // Look up required system words for building the code buffer.
        // These must always be present after init_vm(); return a proper error if missing.
        let lit_marker_xt = self.lookup_required("LIT_MARKER", err_line, err_col, source_line)?;
        let call_xt = self.lookup_required("CALL", err_line, err_col, source_line)?;
        let drop_to_marker_xt =
            self.lookup_required("DROP_TO_MARKER", err_line, err_col, source_line)?;

        // Build code sequence:
        //   Xt(LIT_MARKER)
        //   [arg_cells]
        //   For compiled words: Xt(CALL), Xt(stmt), Int(arity), Int(local_count)
        //   For primitives:     Xt(stmt)  (dispatched directly by the inner interpreter)
        //   Xt(DROP_TO_MARKER)
        self.vm
            .dict_write(Cell::Xt(lit_marker_xt))
            .map_err(&make_err)?;
        // Record base_dp after LIT_MARKER so that expr_patch_offsets can be
        // translated to absolute dictionary positions.
        let base_dp = self.vm.dp;
        for cell in arg_cells {
            self.vm.dict_write(cell).map_err(&make_err)?;
        }
        // Register self-recursive local_count placeholder positions found inside
        // the argument expression.
        if let Some(state) = &mut self.compile_state {
            for offset in expr_patch_offsets {
                state.call_patch_list.push(base_dp + offset);
            }
        }
        if stmt_is_word {
            // Determine local_count for the CALL instruction.
            // For self-recursive calls (word currently being compiled), local_count is not yet
            // known — write 0 as placeholder and add the position to the patch list.
            // For all other calls, use the callee's confirmed local_count from the header.
            // Compare by header index (not name) to handle shadowed/redefined words correctly.
            let is_self_recursive = self
                .compile_state
                .as_ref()
                .map(|s| stmt_xt.index() == s.hdr_len_at_def)
                .unwrap_or(false);

            self.vm.dict_write(Cell::Xt(call_xt)).map_err(&make_err)?;
            self.vm.dict_write(Cell::Xt(stmt_xt)).map_err(&make_err)?;
            self.vm
                .dict_write(Cell::Int(arity as i64))
                .map_err(&make_err)?;

            if is_self_recursive {
                let patch_pos = self.vm.dp;
                self.vm.dict_write(Cell::Int(0)).map_err(&make_err)?;
                if let Some(state) = &mut self.compile_state {
                    state.call_patch_list.push(patch_pos);
                }
            } else {
                let callee_local_count = self.vm.headers[stmt_xt.index()].local_count;
                self.vm
                    .dict_write(Cell::Int(callee_local_count as i64))
                    .map_err(&make_err)?;
            }
        } else {
            self.vm.dict_write(Cell::Xt(stmt_xt)).map_err(&make_err)?;
        }
        self.vm
            .dict_write(Cell::Xt(drop_to_marker_xt))
            .map_err(&make_err)?;

        Ok(())
    }

    /// Execute a multi-line source string.
    ///
    /// Splits `src` by newlines and calls `exec_line` for each.
    /// Stops on the first error (including `TbxError::Halted`, which the inner
    /// interpreter returns for the `HALT` statement).
    pub fn exec_source(&mut self, src: &str) -> Result<(), InterpreterError> {
        for line in src.lines() {
            match self.exec_line(line) {
                Ok(()) => {}
                Err(e) if matches!(e.kind, TbxError::Halted) => return Ok(()),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Take the current output buffer contents, leaving it empty.
    pub fn take_output(&mut self) -> String {
        self.vm.take_output()
    }

    /// Register a line-number label at the current dictionary pointer.
    ///
    /// Inserts the label into the label table and back-patches any forward
    /// references that were left as `Int(0)` placeholders.
    fn register_label(
        &mut self,
        n: i64,
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);
        let dp = self.vm.dp;
        let state = self
            .compile_state
            .as_mut()
            .expect("register_label called outside compile mode");

        // Reject duplicate label definitions within the same word.
        if state.label_table.contains_key(&n) {
            return Err(make_err(TbxError::DuplicateLabel { label: n }));
        }
        state.label_table.insert(n, dp);

        // Collect all dictionary positions that are waiting for this label.
        let patches: Vec<usize> = state
            .patch_list
            .iter()
            .filter(|(lbl, _)| *lbl == n)
            .map(|(_, pos)| *pos)
            .collect();
        state.patch_list.retain(|(lbl, _)| *lbl != n);

        // Apply back-patches (must end the borrow of compile_state first).
        let _ = state;
        for patch_pos in patches {
            self.vm.dictionary[patch_pos] = Cell::Int(dp as i64);
        }

        Ok(())
    }

    /// Compile `GOTO N` into the dictionary.
    ///
    /// Emits: `Xt(GOTO) Int(target)`
    /// If `N` is unknown (forward reference), emits `Int(0)` and registers a back-patch entry.
    fn compile_goto(
        &mut self,
        arg_tokens: &[SpannedToken],
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        let label_n = parse_label_number(arg_tokens).ok_or_else(|| {
            make_err(TbxError::InvalidExpression {
                reason: "GOTO requires an integer label",
            })
        })?;

        let goto_xt = self.lookup_required("GOTO", line, col, source_line)?;
        self.vm.dict_write(Cell::Xt(goto_xt)).map_err(&make_err)?;
        self.emit_jump_target(label_n, source_line, line, col)?;
        Ok(())
    }

    /// Compile `BIF cond, N` or `BIT cond, N` into the dictionary.
    ///
    /// `is_truthy`: `true` → BIT (branch if true), `false` → BIF (branch if false).
    /// Emits: `[condition cells] Xt(BIF|BIT) Int(target)`
    fn compile_branch(
        &mut self,
        is_truthy: bool,
        arg_tokens: &[SpannedToken],
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        // Split at the last top-level comma: left = condition expression, right = label.
        let split_pos = last_top_level_comma(arg_tokens)
            .map_err(&make_err)?
            .ok_or_else(|| {
                make_err(TbxError::InvalidExpression {
                    reason: "BIF/BIT requires syntax: BIF cond, label",
                })
            })?;
        let cond_tokens = &arg_tokens[..split_pos];
        let label_tokens = &arg_tokens[split_pos + 1..];

        // Parse label number.
        let label_n = parse_label_number(label_tokens).ok_or_else(|| {
            make_err(TbxError::InvalidExpression {
                reason: "BIF/BIT label must be an integer",
            })
        })?;

        // Compile the condition expression directly into the dictionary.
        let (cond_cells, expr_patch_offsets) = {
            let local_table_opt = self.compile_state.as_ref().map(|s| &s.local_table);
            let self_word = self.compile_state.as_ref().map(|s| s.word_name.clone());
            let self_hdr_idx = self.compile_state.as_ref().map(|s| s.hdr_len_at_def);
            let mut compiler =
                ExprCompiler::with_context(&mut self.vm, local_table_opt, self_word, self_hdr_idx);
            let cells = compiler.compile_expr(cond_tokens).map_err(&make_err)?;
            let offsets = std::mem::take(&mut compiler.patch_offsets);
            (cells, offsets)
        };
        let base_dp = self.vm.dp;
        for cell in cond_cells {
            self.vm.dict_write(cell).map_err(&make_err)?;
        }
        // Register self-recursive local_count placeholder positions found inside
        // the condition expression.
        if let Some(state) = &mut self.compile_state {
            for offset in expr_patch_offsets {
                state.call_patch_list.push(base_dp + offset);
            }
        }

        // Emit BIF or BIT.
        let branch_name = if is_truthy { "BIT" } else { "BIF" };
        let branch_xt = self.lookup_required(branch_name, line, col, source_line)?;
        self.vm.dict_write(Cell::Xt(branch_xt)).map_err(&make_err)?;

        // Emit jump target (with back-patch if this is a forward reference).
        self.emit_jump_target(label_n, source_line, line, col)?;
        Ok(())
    }

    /// Compile a `RETURN` statement inside a DEF body.
    ///
    /// - `RETURN` (no args)   → `Xt(EXIT)`
    /// - `RETURN expr`        → `[expr cells] Xt(RETURN_VAL)`
    ///
    /// Unlike regular statements, no `LIT_MARKER`/`DROP_TO_MARKER` wrapper is emitted,
    /// because RETURN must not discard the return value from the stack.
    fn compile_return(
        &mut self,
        arg_tokens: &[SpannedToken],
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        if !arg_tokens.is_empty() {
            // Compile the return expression directly into the dictionary.
            let (expr_cells, expr_patch_offsets) = {
                let local_table_opt = self.compile_state.as_ref().map(|s| &s.local_table);
                let self_word = self.compile_state.as_ref().map(|s| s.word_name.clone());
                let self_hdr_idx = self.compile_state.as_ref().map(|s| s.hdr_len_at_def);
                let mut compiler = ExprCompiler::with_context(
                    &mut self.vm,
                    local_table_opt,
                    self_word,
                    self_hdr_idx,
                );
                let cells = compiler.compile_expr(arg_tokens).map_err(&make_err)?;
                let offsets = std::mem::take(&mut compiler.patch_offsets);
                (cells, offsets)
            };
            let base_dp = self.vm.dp;
            for cell in expr_cells {
                self.vm.dict_write(cell).map_err(&make_err)?;
            }
            // Register self-recursive local_count placeholder positions found inside
            // the return expression.
            if let Some(state) = &mut self.compile_state {
                for offset in expr_patch_offsets {
                    state.call_patch_list.push(base_dp + offset);
                }
            }
            // Emit RETURN_VAL to return the top-of-stack value from the word.
            let return_val_xt = self.lookup_required("RETURN_VAL", line, col, source_line)?;
            self.vm
                .dict_write(Cell::Xt(return_val_xt))
                .map_err(&make_err)?;
        } else {
            // Void return: emit EXIT to leave the word immediately.
            let exit_xt = self.lookup_required("EXIT", line, col, source_line)?;
            self.vm.dict_write(Cell::Xt(exit_xt)).map_err(&make_err)?;
        }

        Ok(())
    }

    /// Emit a jump target address cell into the dictionary.
    ///
    /// If the label is already known, emits `Int(addr)`.
    /// If the label is unknown (forward reference), emits `Int(0)` and records the position
    /// in `patch_list` for back-patching when the label is defined.
    fn emit_jump_target(
        &mut self,
        label_n: i64,
        source_line: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_line, e);

        // Check whether the label is already known (backward reference).
        let target_opt = self
            .compile_state
            .as_ref()
            .expect("emit_jump_target called outside compile mode")
            .label_table
            .get(&label_n)
            .copied();

        if let Some(target) = target_opt {
            self.vm
                .dict_write(Cell::Int(target as i64))
                .map_err(&make_err)?;
        } else {
            // Forward reference: emit placeholder and record position for back-patching.
            let patch_pos = self.vm.dp;
            self.vm.dict_write(Cell::Int(0)).map_err(&make_err)?;
            self.compile_state
                .as_mut()
                .expect("emit_jump_target called outside compile mode")
                .patch_list
                .push((label_n, patch_pos));
        }
        Ok(())
    }
}

/// Count the number of top-level comma-separated arguments in a token slice.
///
/// "Top-level" means not nested inside parentheses.
/// Returns `Ok(0)` for an empty slice, otherwise `Ok(top_level_commas + 1)`.
///
/// Returns `Err(TbxError::InvalidExpression)` if an unmatched `)` is found.
fn count_top_level_arity(tokens: &[SpannedToken]) -> Result<usize, TbxError> {
    if tokens.is_empty() {
        return Ok(0);
    }
    let mut depth: usize = 0;
    let mut commas: usize = 0;
    for st in tokens {
        match &st.token {
            Token::LParen => depth += 1,
            Token::RParen => {
                depth = depth.checked_sub(1).ok_or(TbxError::InvalidExpression {
                    reason: "unmatched ')' in argument list",
                })?;
            }
            Token::Comma if depth == 0 => commas += 1,
            _ => {}
        }
    }
    Ok(commas + 1)
}

/// Find the position of the last top-level comma in a token slice.
///
/// "Top-level" means not nested inside parentheses.
/// Returns `None` if no top-level comma is found.
fn last_top_level_comma(tokens: &[SpannedToken]) -> Result<Option<usize>, TbxError> {
    let mut depth: usize = 0;
    let mut last_comma = None;
    for (i, st) in tokens.iter().enumerate() {
        match &st.token {
            Token::LParen => depth += 1,
            Token::RParen => {
                depth = depth.checked_sub(1).ok_or(TbxError::InvalidExpression {
                    reason: "unmatched ')' in argument list",
                })?;
            }
            Token::Comma if depth == 0 => last_comma = Some(i),
            _ => {}
        }
    }
    Ok(last_comma)
}

/// Parse a label number from a token slice.
///
/// Skips leading `Newline`/`Eof` tokens and returns the integer value of the
/// first meaningful token if it is an `IntLit` or `LineNum`.
fn parse_label_number(tokens: &[SpannedToken]) -> Option<i64> {
    let tok = tokens
        .iter()
        .find(|st| !matches!(st.token, Token::Newline | Token::Eof))?;
    match &tok.token {
        Token::IntLit(n) => Some(*n),
        Token::LineNum(n) => Some(*n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_putdec_42() {
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 42").unwrap();
        let out = interp.take_output();
        assert!(
            out.contains("42"),
            "expected '42' in output, got: {:?}",
            out
        );
    }

    #[test]
    fn test_exec_halt() {
        let mut interp = Interpreter::new();
        let result = interp.exec_line("HALT");
        // HALT causes TbxError::Halted wrapped in InterpreterError
        assert!(result.is_err(), "expected error from HALT");
        let err = result.unwrap_err();
        assert!(matches!(err.kind, TbxError::Halted));
    }

    #[test]
    fn test_exec_source_putdec_then_halt() {
        let mut interp = Interpreter::new();
        let src = "PUTDEC 42\nHALT";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert!(
            out.contains("42"),
            "expected '42' in output, got: {:?}",
            out
        );
    }

    #[test]
    fn test_exec_undefined_symbol() {
        let mut interp = Interpreter::new();
        let result = interp.exec_line("NOSUCHWORD 1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err.kind, TbxError::UndefinedSymbol { .. }));
    }

    #[test]
    fn test_exec_system_word_direct_call_rejected() {
        let mut interp = Interpreter::new();
        // Attempting to call a FLAG_SYSTEM word (LIT_MARKER) as a statement should fail.
        let result = interp.exec_line("LIT_MARKER");
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_rem_is_skipped() {
        let mut interp = Interpreter::new();
        // REM line should not produce any error or output.
        interp.exec_line("REM this is a comment").unwrap();
        let out = interp.take_output();
        assert!(out.is_empty());
    }

    #[test]
    fn test_exec_empty_line() {
        let mut interp = Interpreter::new();
        interp.exec_line("").unwrap();
        interp.exec_line("   ").unwrap();
    }

    #[test]
    fn test_count_top_level_arity_empty() {
        assert_eq!(count_top_level_arity(&[]), Ok(0));
    }

    #[test]
    fn test_vm_state_restored_after_error() {
        // After a runtime error the data stack, return stack, and bp must be clean.
        let mut interp = Interpreter::new();
        // Force a runtime error by calling an undefined symbol at runtime.
        let _ = interp.exec_line("NOSUCHWORD");
        // A subsequent valid call must still work.
        interp.exec_line("PUTDEC 1").unwrap();
        let out = interp.take_output();
        assert!(out.contains('1'));
    }

    #[test]
    fn test_def_end_basic() {
        // Define a word GREET that prints 42, then call it.
        let mut interp = Interpreter::new();
        let src = "\
DEF GREET
PUTDEC 42
END
GREET";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert!(
            out.contains("42"),
            "expected '42' in output, got: {:?}",
            out
        );
    }

    #[test]
    fn test_def_missing_name_is_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_line("DEF");
        assert!(result.is_err());
    }

    #[test]
    fn test_end_outside_def_is_not_intercepted() {
        // END outside DEF should fall through to normal execution (likely error or no-op).
        let mut interp = Interpreter::new();
        // END is not in compile mode so it goes to the normal lookup path.
        // It may be undefined — just check it doesn't panic.
        let _ = interp.exec_line("END");
    }

    #[test]
    fn test_nested_def_is_error() {
        let mut interp = Interpreter::new();
        let src = "DEF OUTER\nDEF INNER\nEND\nEND";
        let result = interp.exec_source(src);
        assert!(result.is_err());
    }

    #[test]
    fn test_def_error_rolls_back() {
        // A DEF body with an undefined symbol should fail and roll back.
        let mut interp = Interpreter::new();
        let src = "DEF BAD\nNOSUCH 1\nEND";
        assert!(interp.exec_source(src).is_err());
        // After rollback, defining and calling a valid word must succeed.
        interp
            .exec_source("DEF GOOD\nPUTDEC 99\nEND\nGOOD")
            .unwrap();
        let out = interp.take_output();
        assert!(
            out.contains("99"),
            "expected '99' in output after rollback, got: {:?}",
            out
        );
    }

    // Helper: tokenize a source fragment into SpannedTokens, stripping Newline/Eof.
    fn tokenize_args(s: &str) -> Vec<SpannedToken> {
        let mut lex = Lexer::new(s);
        let mut tokens = Vec::new();
        loop {
            let st = lex.next_token();
            match &st.token {
                Token::Newline | Token::Eof => break,
                _ => tokens.push(st),
            }
        }
        tokens
    }

    #[test]
    fn test_count_top_level_arity_single() {
        let tokens = tokenize_args("42");
        assert_eq!(count_top_level_arity(&tokens), Ok(1));
    }

    #[test]
    fn test_count_top_level_arity_multiple() {
        let tokens = tokenize_args("1 , 2 , 3");
        assert_eq!(count_top_level_arity(&tokens), Ok(3));
    }

    #[test]
    fn test_count_top_level_arity_nested_parens() {
        // Commas inside parentheses must not be counted.
        let tokens = tokenize_args("f(1,2)");
        assert_eq!(count_top_level_arity(&tokens), Ok(1));
    }

    #[test]
    fn test_count_top_level_arity_unmatched_rparen() {
        let tokens = tokenize_args(")");
        assert!(matches!(
            count_top_level_arity(&tokens),
            Err(TbxError::InvalidExpression { .. })
        ));
    }

    #[test]
    fn test_exec_primitive_call_in_expression() {
        // Regression test for issue #208: calling a Primitive from within an expression
        // must succeed via the direct-Xt path (no CALL instruction generated by expr.rs).
        // ADD(1, 2) compiles to: Xt(LIT), Int(1), Xt(LIT), Int(2), Xt(ADD) — ADD is dispatched
        // directly as a Primitive.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC ADD(1, 2)").unwrap();
        let out = interp.take_output();
        assert_eq!(out, "3", "expected '3', got: {:?}", out);
    }

    // --- issue #205: DEF formal parameters and VAR locals ---

    #[test]
    fn test_def_with_param_double() {
        // DEF DOUBLE(X) multiplies its argument by 2.
        let mut interp = Interpreter::new();
        let src = "\
DEF DOUBLE(X)
  PUTDEC X * 2
  PUTSTR \"\\n\"
END
DOUBLE 21";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "42\n", "expected '42\\n', got: {:?}", out);
    }

    #[test]
    fn test_def_with_var_counter() {
        // DEF COUNTER declares a local VAR and assigns it.
        let mut interp = Interpreter::new();
        let src = "\
DEF COUNTER
  VAR I
  LET &I, 1
  PUTDEC I
  PUTSTR \"\\n\"
END
COUNTER";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "1\n", "expected '1\\n', got: {:?}", out);
    }

    #[test]
    fn test_def_param_multiple_calls() {
        // Calling a parameterized word multiple times must work correctly.
        let mut interp = Interpreter::new();
        let src = "\
DEF DOUBLE(X)
  PUTDEC X * 2
  PUTSTR \"\\n\"
END
DOUBLE 3
DOUBLE 10";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "6\n20\n", "expected '6\\n20\\n', got: {:?}", out);
    }

    #[test]
    fn test_def_param_local_shadows_global() {
        // A formal parameter should shadow a global variable of the same name.
        let mut interp = Interpreter::new();
        let src = "\
VAR X
LET &X, 99
DEF SHADOW(X)
  PUTDEC X
  PUTSTR \"\\n\"
END
SHADOW 42";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(
            out, "42\n",
            "expected '42\\n' (local shadows global), got: {:?}",
            out
        );
    }

    #[test]
    fn test_def_param_and_var_combined() {
        // A word with both a formal parameter and a local VAR should work correctly.
        let mut interp = Interpreter::new();
        let src = "\
DEF ADDONE(X)
  VAR R
  LET &R, X + 1
  PUTDEC R
  PUTSTR \"\\n\"
END
ADDONE 10";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "11\n", "expected '11\\n', got: {:?}", out);
    }

    #[test]
    fn test_def_var_isolated_across_calls() {
        // Each call to a word with VAR locals must get its own independent slot.
        // Calling ADDONE twice should produce independent results.
        let mut interp = Interpreter::new();
        let src = "\
DEF ADDONE(X)
  VAR R
  LET &R, X + 1
  PUTDEC R
  PUTSTR \"\\n\"
END
ADDONE 5
ADDONE 20";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "6\n21\n", "expected '6\\n21\\n', got: {:?}", out);
    }

    // --- issue #224: line-number labels, GOTO, BIF, BIT in compile mode ---

    #[test]
    fn test_goto_backward_compiles() {
        // Compile a word containing a backward GOTO; just verify compilation succeeds.
        let mut interp = Interpreter::new();
        interp
            .exec_source("DEF MYWORD\n  10\n  GOTO 10\nEND")
            .unwrap();
        assert!(
            interp.vm.lookup("MYWORD").is_some(),
            "MYWORD should be defined"
        );
    }

    #[test]
    fn test_loop_1_to_10() {
        // A counted loop using GOTO and BIT that prints 1..10.
        let src = r#"
DEF MYWORD
  VAR I
  LET &I, 1
  10
    PUTDEC I
    PUTSTR "\n"
    LET &I, I + 1
    BIT I > 10, 99
    GOTO 10
  99
END
MYWORD
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        let output = interp.take_output();
        let expected: String = (1..=10).map(|i| format!("{}\n", i)).collect();
        assert_eq!(output, expected, "loop output mismatch");
    }

    #[test]
    fn test_bif_skips_on_false() {
        // BIF condition,label — branch if condition is false (zero).
        // When I = 0 (false), BIF should jump to label 99 and skip PUTDEC.
        let src = r#"
DEF TESTBIF
  VAR I
  LET &I, 0
  BIF I, 99
  PUTDEC 42
  PUTSTR "\n"
  99
END
TESTBIF
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        // Condition I=0 is false, so BIF should jump over PUTDEC.
        let output = interp.take_output();
        assert_eq!(output, "", "BIF with false condition should skip PUTDEC");
    }

    #[test]
    fn test_bif_falls_through_on_true() {
        // BIF condition,label — when condition is true (non-zero), should NOT branch.
        let src = r#"
DEF TESTBIF2
  VAR I
  LET &I, 1
  BIF I, 99
  PUTDEC 42
  PUTSTR "\n"
  99
END
TESTBIF2
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        let output = interp.take_output();
        assert_eq!(
            output, "42\n",
            "BIF with true condition should fall through to PUTDEC"
        );
    }

    #[test]
    fn test_forward_reference_backpatch() {
        // GOTO to a label that appears AFTER the GOTO (forward reference).
        // The word should execute without getting stuck.
        let src = r#"
DEF FWDTEST
  GOTO 99
  PUTDEC 999
  PUTSTR "\n"
  99
  PUTDEC 1
  PUTSTR "\n"
END
FWDTEST
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        let output = interp.take_output();
        // GOTO 99 skips PUTDEC 999; only PUTDEC 1 should execute.
        assert_eq!(output, "1\n", "forward GOTO should skip first PUTDEC");
    }

    #[test]
    fn test_undefined_label_is_error() {
        // Referencing a label that is never defined should produce UndefinedLabel at END.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF BADWORD\n  GOTO 999\nEND");
        assert!(result.is_err(), "expected error for undefined label");
        let err = result.unwrap_err();
        assert!(
            matches!(err.kind, TbxError::UndefinedLabel { label: 999 }),
            "expected UndefinedLabel(999), got: {:?}",
            err.kind
        );
    }

    #[test]
    fn test_label_table_and_patch_list_helpers() {
        // Unit tests for the module-level helper functions.
        let toks = tokenize_args("42");
        assert_eq!(parse_label_number(&toks), Some(42));

        let toks = tokenize_args("I > 10, 99");
        let comma_pos = last_top_level_comma(&toks);
        assert!(
            comma_pos.unwrap().is_some(),
            "should find a top-level comma"
        );

        let toks_no_comma = tokenize_args("42");
        assert_eq!(last_top_level_comma(&toks_no_comma).unwrap(), None);
    }

    #[test]
    fn test_duplicate_label_is_error() {
        // Defining the same label twice in one word must produce DuplicateLabel.
        let src = "DEF DUPWORD\n  10\n  PUTDEC 1\n  10\nEND";
        let mut interp = Interpreter::new();
        let result = interp.exec_source(src);
        assert!(result.is_err(), "expected error for duplicate label");
        let err = result.unwrap_err();
        assert!(
            matches!(err.kind, TbxError::DuplicateLabel { label: 10 }),
            "expected DuplicateLabel(10), got: {:?}",
            err.kind
        );

        // After rollback, the same word name can be redefined successfully.
        let result2 = interp.exec_source("DEF DUPWORD\n  PUTDEC 5\nEND\nDUPWORD");
        assert!(
            result2.is_ok(),
            "redefine after DuplicateLabel rollback failed: {:?}",
            result2.unwrap_err()
        );
        assert_eq!(interp.take_output(), "5");
    }

    #[test]
    fn test_last_top_level_comma_unmatched_paren_errors() {
        // An unmatched ')' must produce an InvalidExpression error.
        let toks = tokenize_args("42 ), 99");
        let result = last_top_level_comma(&toks);
        assert!(
            matches!(result, Err(TbxError::InvalidExpression { .. })),
            "expected InvalidExpression for unmatched ')', got: {:?}",
            result
        );
    }

    #[test]
    fn test_undefined_label_error_rollback_allows_redefine() {
        // After UndefinedLabel error, the VM should roll back so that the word
        // can be redefined successfully.
        let mut interp = Interpreter::new();

        // First attempt: GOTO to undefined label 999 — should error.
        let result = interp.exec_source("DEF REDEFWORD\n  GOTO 999\nEND");
        assert!(result.is_err());

        // Second attempt: valid definition of the same name — should succeed.
        let result2 = interp.exec_source("DEF REDEFWORD\n  PUTDEC 7\nEND\nREDEFWORD");
        assert!(
            result2.is_ok(),
            "redefine after rollback failed: {:?}",
            result2.unwrap_err()
        );
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_return_with_value() {
        // RETURN A + B compiles to [expr cells] Xt(RETURN_VAL).
        // SUM(3, 4) should leave 7 on the caller's stack.
        // Note: the word is named SUM (not ADD) to avoid shadowing the built-in ADD primitive
        // that the `+` operator relies on internally.
        let src = r#"
DEF SUM(A, B)
  RETURN A + B
END
PUTDEC SUM(3, 4)
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_return_void() {
        // RETURN (no args) inside a conditional block compiles to Xt(EXIT).
        // When FLAG=0, BIF jumps over PUTDEC, and RETURN exits the word early;
        // output should be empty.
        let src = r#"
DEF PRINTIF(FLAG, VAL)
  BIF FLAG, 99
    PUTDEC VAL
  99
  RETURN
END
PRINTIF 0, 42
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(
            interp.take_output(),
            "",
            "RETURN void should exit without output"
        );

        // FLAG=1: BIF does not jump, PUTDEC executes, then RETURN exits.
        let src2 = r#"
DEF PRINTIF(FLAG, VAL)
  BIF FLAG, 99
    PUTDEC VAL
  99
  RETURN
END
PRINTIF 1, 42
"#;
        let mut interp2 = Interpreter::new();
        interp2.exec_source(src2).unwrap();
        assert_eq!(
            interp2.take_output(),
            "42",
            "RETURN void after PUTDEC should produce output"
        );
    }

    #[test]
    fn test_return_val_in_conditional() {
        // BIF + RETURN expr: when FLAG=1 return A, else return B.
        let src = r#"
DEF CHOOSE(FLAG, A, B)
  BIF FLAG, 10
    RETURN A
  10
  RETURN B
END
PUTDEC CHOOSE(1, 100, 200)
PUTSTR " "
PUTDEC CHOOSE(0, 100, 200)
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "100 200");
    }

    #[test]
    fn test_def_word_named_after_operator_primitive() {
        // DEF ADD(A, B) RETURN A + B END must not cause infinite recursion.
        // During body compilation, FLAG_HIDDEN prevents the compiler from resolving
        // the `+` operator to the partially-compiled ADD word instead of the primitive.
        let src = r#"
DEF ADD(A, B)
  RETURN A + B
END
PUTDEC ADD(3, 4)
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_def_word_named_after_mul_primitive() {
        // MUL is the primitive for `*`. A user word named MUL must not shadow it during body.
        let src = r#"
DEF MUL(A, B)
  RETURN A * B
END
PUTDEC MUL(3, 4)
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "12");
    }

    #[test]
    fn test_def_word_named_after_lt_primitive() {
        // LT is the primitive for `<`. A user word named LT must not shadow it during body.
        // Use LT result inside another DEF to verify correctness.
        // Note: multi-arg statement calls use comma syntax: WORD arg1, arg2
        let src = r#"
DEF LT(A, B)
  RETURN A < B
END
DEF CHECK(A, B)
  BIF LT(A, B), 99
    PUTSTR "yes"
  99
  RETURN
END
CHECK 1, 2
CHECK 5, 3
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "yes");
    }

    #[test]
    fn test_self_recursive_word() {
        // Self-recursive calls must work even though the word is FLAG_HIDDEN during compilation.
        // FACT(N): returns N! (factorial). BIF N, 10 jumps to label 10 when N=0 (base case).
        let src = r#"
DEF FACT(N)
  BIF N, 10
    RETURN N * FACT(N - 1)
  10
  RETURN 1
END
PUTDEC FACT(5)
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "120");
    }

    #[test]
    fn test_recursive_with_var_expr() {
        // Regression test for issue #222: self-recursive call inside a LET expression
        // (processed by ExprCompiler) must have its local_count back-patched.
        // Previously, local_count=0 was permanently embedded and caused IndexOutOfBounds
        // at runtime when the VAR slot was accessed.
        // Uses BIT (branch if true) so the base case label is reached when N <= 1 is true.
        let src = r#"
DEF FACT(N)
  VAR R
  BIT N <= 1, 10
    LET &R, N * FACT(N - 1)
    RETURN R
  10 RETURN 1
END
PUTDEC FACT(5)
PUTSTR "\n"
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "120\n");
    }
}
