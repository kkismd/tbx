//! Outer interpreter: tokenizes source text and executes statements via the inner interpreter.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use crate::cell::{Cell, ReturnFrame, Xt};
use crate::dict::FLAG_SYSTEM;
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::init_vm;
use crate::lexer::{Lexer, SpannedToken, Token};
use crate::vm::VM;

/// Maximum allowed nesting depth for USE statements.
///
/// Acts as a safety net for non-circular but excessively deep USE chains.
/// Circular references are detected precisely by `loading_files` before this
/// limit is reached, so this constant guards only against pathological
/// (non-circular) deep nesting.
///
/// 64 levels is sufficient for any realistic library hierarchy; each
/// `exec_source` frame allocates several KB of stack space (lexer, token
/// buffer, VM execution context), and the typical thread stack (1–8 MB) is
/// exhausted well before 256 levels regardless of platform.
const MAX_USE_DEPTH: usize = 64;

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

impl std::error::Error for InterpreterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Display already formats `self.kind` inline (see the Display impl above),
        // so we do not chain the source here.  Returning Some(&self.kind) would cause
        // error reporters like `anyhow` / `eyre` to print the same message twice.
        None
    }
}

/// Token list and segment boundaries produced by `parse_line_into_segments`.
///
/// The `Vec<(usize, usize)>` contains `(start, end)` index pairs into the token list.
type ParsedSegments = (Vec<SpannedToken>, Vec<(usize, usize)>);

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
    /// Current USE nesting depth. Incremented each time `exec_source` is called
    /// via a USE statement, decremented on return. Acts as a safety net against
    /// excessively deep (but non-circular) USE chains.
    use_depth: usize,
    /// Effective upper bound for `use_depth`. Defaults to `MAX_USE_DEPTH`.
    /// Exposed as a field so that tests can set a smaller value without
    /// creating hundreds of temporary files.
    max_use_depth: usize,
    /// Set of canonicalized paths currently being loaded via USE.
    ///
    /// A path is inserted before `exec_source` is called and removed after it
    /// returns (whether with success or error). If a path is already present
    /// when a USE is about to start, a circular reference is detected and
    /// `TbxError::CircularUse` is returned.
    ///
    /// Note: if `exec_source` panics, `loading_files` will not be cleaned up.
    /// A full RAII guard is not feasible here because holding a mutable
    /// borrow of `loading_files` (via the guard) conflicts with the
    /// `&mut self` borrow required by the recursive `exec_source` call.
    /// In practice this is acceptable: panics in `exec_source` signal
    /// unrecoverable programmer errors and typically abort the process.
    loading_files: HashSet<PathBuf>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    /// Create a new `Interpreter` backed by a fully initialized VM.
    ///
    /// Loads the standard library (`lib/basic.tbx`) embedded at compile time.
    /// Panics if the standard library fails to load, which indicates a bug in
    /// the library source rather than a runtime failure.
    ///
    /// For a fallible variant that returns an error instead of panicking,
    /// use [`Interpreter::try_new`].
    pub fn new() -> Self {
        // lib/basic.tbx is embedded at compile time and is always syntactically valid TBX.
        // A panic here indicates a bug in the standard library source, not a runtime failure.
        Self::try_new().unwrap_or_else(|e| {
            panic!("internal error: failed to load lib/basic.tbx: {e}");
        })
    }

    /// Create a new `Interpreter`, returning an error if the standard library fails to load.
    ///
    /// This is the fallible counterpart of [`Interpreter::new`]. Prefer this in contexts
    /// where proper error propagation is possible.
    pub fn try_new() -> Result<Self, InterpreterError> {
        let mut interp = Self {
            vm: init_vm(),
            use_depth: 0,
            max_use_depth: MAX_USE_DEPTH,
            loading_files: HashSet::new(),
        };
        const STDLIB: &str = include_str!("../lib/basic.tbx");
        interp.exec_source(STDLIB)?;
        interp.vm.seal_lib();
        Ok(interp)
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

    /// Tokenizes `source_line` and splits it into non-empty statement segments.
    ///
    /// Segments are delimited by semicolons.  A leading `LineNum` token on the
    /// first segment is stripped; if the interpreter is currently inside a DEF
    /// body, it is also registered as a branch-target label.  Empty segments
    /// (e.g. a trailing semicolon) are omitted from the result.
    ///
    /// Returns a tuple of `(tokens, boundaries)` where each boundary `(start, end)`
    /// is a half-open index range into `tokens`.
    fn parse_line_into_segments(
        &mut self,
        source_line: &str,
    ) -> Result<ParsedSegments, InterpreterError> {
        let mut lex = Lexer::new(source_line);
        let mut tokens: Vec<SpannedToken> = Vec::new();
        loop {
            let st = lex.next_token();
            match &st.token {
                Token::Newline | Token::Eof => break,
                _ => tokens.push(st),
            }
        }

        if tokens.is_empty() {
            return Ok((tokens, Vec::new()));
        }

        // Split token list into segments at each Semicolon.
        // Semicolons cannot appear inside expressions, so a flat split is correct.
        let semi_positions: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, st)| matches!(st.token, Token::Semicolon).then_some(i))
            .collect();

        let mut raw_boundaries: Vec<(usize, usize)> = Vec::with_capacity(semi_positions.len() + 1);
        let mut start = 0;
        for &pos in &semi_positions {
            raw_boundaries.push((start, pos));
            start = pos + 1;
        }
        raw_boundaries.push((start, tokens.len()));

        let mut boundaries: Vec<(usize, usize)> = Vec::with_capacity(raw_boundaries.len());
        let mut first_segment = true;
        for (seg_start_orig, seg_end) in raw_boundaries {
            let mut seg_start = seg_start_orig;

            // Only the first segment may begin with a line number.
            if first_segment {
                first_segment = false;
                if seg_start < seg_end {
                    if let Token::LineNum(n) = tokens[seg_start].token {
                        if self.vm.compile_state.is_some() {
                            // Inside a DEF body: register as a branch-target label.
                            let ln_line = tokens[seg_start].pos.line;
                            let ln_col = tokens[seg_start].pos.col;
                            self.register_label(n, source_line, ln_line, ln_col)
                                .inspect_err(|_e| {
                                    self.vm.rollback_def();
                                })?;
                        }
                        // Line numbers outside DEF are silently discarded.
                        seg_start += 1;
                    }
                }
            }

            // Omit empty segments (e.g. trailing semicolon or bare line number).
            if seg_start < seg_end {
                boundaries.push((seg_start, seg_end));
            }
        }

        Ok((tokens, boundaries))
    }

    /// Execute a single source line.
    ///
    /// Tokenizes `line`, resolves the statement word, builds a temporary code buffer,
    /// and runs it through the inner interpreter.
    ///
    /// Returns `Ok(())` on success, or an `InterpreterError` containing position
    /// and error details on failure.
    pub fn exec_line(&mut self, line: &str) -> Result<(), InterpreterError> {
        let (tokens, boundaries) = self.parse_line_into_segments(line)?;
        for (seg_start, seg_end) in boundaries {
            self.exec_segment(&tokens[seg_start..seg_end], line)?;
        }
        Ok(())
    }

    /// Executes a single statement segment (a slice of tokens with no Semicolons).
    ///
    /// `tokens` must be non-empty and must not contain `Token::LineNum` at index 0
    /// (the caller is responsible for stripping it on the first segment).
    fn exec_segment(
        &mut self,
        tokens: &[SpannedToken],
        source_line: &str,
    ) -> Result<(), InterpreterError> {
        let mut idx = 0;

        // Extract statement name.
        let stmt_tok = &tokens[idx];
        let stmt_name = match &stmt_tok.token {
            Token::Ident(name) => name.clone(),
            _ => return Ok(()), // not an identifier — skip
        };
        let stmt_pos_line = stmt_tok.pos.line;
        let stmt_pos_col = stmt_tok.pos.col;
        idx += 1;

        // Normalize the statement name to uppercase for case-insensitive keyword matching.
        // This preserves backward compatibility with lowercase variants of built-in words
        // (e.g. `def`, `end`, `rem`) while keeping user-defined word lookups consistent.
        let stmt_name = stmt_name.to_ascii_uppercase();

        // Handle REM: skip the rest of the segment (lexer already consumed trailing input).
        if stmt_name == "REM" {
            return Ok(());
        }

        // IMMEDIATE word dispatch: execute immediately regardless of compile/interpret mode.
        // If the looked-up word has FLAG_IMMEDIATE set, feed the remaining tokens into
        // vm.token_stream and execute it directly.
        if let Some(xt) = self.vm.lookup(&stmt_name) {
            let flags = self.vm.headers[xt.index()].flags;
            if flags & crate::dict::FLAG_IMMEDIATE != 0 {
                return self.exec_immediate_word(
                    xt,
                    &tokens[idx..],
                    stmt_pos_line,
                    stmt_pos_col,
                    source_line,
                );
            }
        }

        // In compile mode: write this statement to the dictionary instead of executing it.
        if self.vm.compile_state.is_some() {
            let result = self.write_stmt_to_dict(
                &stmt_name,
                &tokens[idx..],
                stmt_pos_line,
                stmt_pos_col,
                source_line,
            );
            if result.is_err() {
                self.vm.rollback_def();
            }
            return result;
        }

        // Helper closure for wrapping TbxError into InterpreterError.
        let make_err =
            |e: TbxError| InterpreterError::new(stmt_pos_line, stmt_pos_col, source_line, e);

        // Save the current dictionary pointer to use as the buffer start.
        let buf_start = self.vm.dp;

        // Write statement and arguments to the dictionary (LIT_MARKER … DROP_TO_MARKER).
        // On failure, reset dp so subsequent exec_line calls start from a clean state.
        if let Err(e) = self.write_stmt_to_dict(
            &stmt_name,
            &tokens[idx..],
            stmt_pos_line,
            stmt_pos_col,
            source_line,
        ) {
            self.vm.dp = buf_start;
            self.vm.dictionary.truncate(buf_start);
            return Err(e);
        }

        // Append EXIT to terminate the temporary code buffer.
        // On failure, reset dp before returning.
        let exit_xt = match self.lookup_required("EXIT", stmt_pos_line, stmt_pos_col, source_line) {
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

    /// Write a single statement and its arguments to the dictionary.
    ///
    /// Emits: `LIT_MARKER [arg_cells] (CALL stmt arity local_count | stmt) DROP_TO_MARKER`
    ///
    /// This is used both during interpretation (followed by `EXIT` + run) and during
    /// compilation (within a DEF body; `EXIT` is written by the END primitive).
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
        let self_word_opt: Option<String> =
            self.vm.compile_state.as_ref().map(|s| s.word_name.clone());
        let stmt_xt = self
            .vm
            .lookup_including_self(stmt_name, self_word_opt.as_deref())
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
        // Uses the same take-compile-restore pattern as `compile_expr_taking_local_table` in
        // primitives.rs: take local_table out first so we can pass `&mut VM` to ExprCompiler,
        // then restore it unconditionally.  The error type here is InterpreterError (not TbxError)
        // due to the `make_err` wrapper, so the helper cannot be shared directly.
        let self_word = self.vm.compile_state.as_ref().map(|s| s.word_name.clone());
        let self_hdr_idx = self.vm.compile_state.as_ref().map(|s| s.word_hdr_idx());
        let local_table = self
            .vm
            .compile_state
            .as_mut()
            .map(|s| std::mem::take(&mut s.local_table));
        let compile_result: Result<(Vec<Cell>, Vec<usize>), InterpreterError> = {
            let local_table_ref = local_table.as_ref();
            let mut compiler =
                ExprCompiler::with_context(&mut self.vm, local_table_ref, self_word, self_hdr_idx);
            compiler
                .compile_expr(arg_tokens)
                .map_err(&make_err)
                .map(|cells| {
                    let offsets = std::mem::take(&mut compiler.patch_offsets);
                    (cells, offsets)
                })
        };
        // Restore local_table regardless of success or failure.
        if let (Some(state), Some(lt)) = (self.vm.compile_state.as_mut(), local_table) {
            state.local_table = lt;
        }
        let (arg_cells, expr_patch_offsets) = compile_result?;

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
        if let Some(state) = &mut self.vm.compile_state {
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
                .vm
                .compile_state
                .as_ref()
                .map(|s| stmt_xt.index() == s.word_hdr_idx())
                .unwrap_or(false);

            self.vm.dict_write(Cell::Xt(call_xt)).map_err(&make_err)?;
            self.vm.dict_write(Cell::Xt(stmt_xt)).map_err(&make_err)?;
            self.vm
                .dict_write(Cell::Int(arity as i64))
                .map_err(&make_err)?;

            if is_self_recursive {
                let patch_pos = self.vm.dp;
                self.vm.dict_write(Cell::Int(0)).map_err(&make_err)?;
                if let Some(state) = &mut self.vm.compile_state {
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

    /// Override the maximum USE nesting depth (test-only).
    ///
    /// Allows unit tests to trigger `TbxError::UseNestingDepthExceeded`
    /// without creating hundreds of temporary files.
    #[cfg(test)]
    fn set_max_use_depth(&mut self, max: usize) {
        self.max_use_depth = max;
    }

    /// Execute an IMMEDIATE word, regardless of compile/interpret mode.
    ///
    /// Sets up `vm.token_stream` with the remaining tokens, dispatches the word
    /// (Primitive or zero-arity Word), then clears the stream.
    ///
    /// On error, rolls back compile state and stack state before returning.
    fn exec_immediate_word(
        &mut self,
        xt: Xt,
        tokens_after_stmt: &[SpannedToken],
        stmt_pos_line: usize,
        stmt_pos_col: usize,
        source_line: &str,
    ) -> Result<(), InterpreterError> {
        let make_err =
            |e: TbxError| InterpreterError::new(stmt_pos_line, stmt_pos_col, source_line, e);

        // Clone fields needed for dispatch before the mutable borrow below.
        let kind = self.vm.headers[xt.index()].kind.clone();
        let arity = self.vm.headers[xt.index()].arity;
        let local_count = self.vm.headers[xt.index()].local_count;

        // Feed remaining tokens into vm.token_stream so the IMMEDIATE word can
        // consume them via vm.next_token().
        let remaining: VecDeque<SpannedToken> = tokens_after_stmt.iter().cloned().collect();
        self.vm.token_stream = Some(remaining);

        // Save VM state for rollback on error.
        let saved_data_stack_len = self.vm.data_stack.len();
        let saved_return_stack_len = self.vm.return_stack.len();
        let saved_bp = self.vm.bp;

        let run_result = match kind {
            // Native primitive: call the function pointer directly (avoids
            // temporary-buffer issues when the primitive writes to the dictionary).
            crate::dict::EntryKind::Primitive(f) => f(&mut self.vm),
            // User-defined word: run via vm.run(), passing the body start address.
            // Guard: words with formal parameters (arity > 0) or VAR locals
            // (local_count > 0) require a CALL frame (bp/stack setup) that
            // vm.run() alone does not provide.
            crate::dict::EntryKind::Word(body_addr) => {
                if arity > 0 || local_count > 0 {
                    Err(TbxError::InvalidExpression {
                        reason: "IMMEDIATE user word with parameters or VAR locals cannot be called without a CALL frame",
                    })
                } else {
                    self.vm.run(body_addr)
                }
            }
            _ => Err(TbxError::InvalidExpression {
                reason: "IMMEDIATE word kind is not executable",
            }),
        };

        // Clear token stream.
        self.vm.token_stream = None;

        // On error, rollback compile state and stacks.
        if run_result.is_err() {
            self.vm.rollback_def();
            self.vm.data_stack.truncate(saved_data_stack_len);
            self.vm.return_stack.truncate(saved_return_stack_len);
            self.vm.bp = saved_bp;
            // Discard any pending USE path set before the error.
            self.vm.pending_use_path = None;
        }

        run_result.map_err(make_err)?;

        // If use_prim stored a path, read the file and execute it now.
        if let Some(path) = self.vm.pending_use_path.take() {
            if self.use_depth >= self.max_use_depth {
                return Err(make_err(TbxError::UseNestingDepthExceeded {
                    limit: self.max_use_depth,
                }));
            }
            // Canonicalize the path before reading so that different textual
            // representations of the same file (e.g. relative vs absolute)
            // are treated as identical for circular-reference detection.
            // canonicalize() fails if the file does not exist, so we report
            // FileNotFound in that case rather than the generic IO error.
            let canonical = std::fs::canonicalize(&path).map_err(|e| {
                make_err(TbxError::FileNotFound {
                    path: path.clone(),
                    reason: e.to_string(),
                })
            })?;
            // Detect circular USE: if this path is already being loaded we
            // are in a cycle (e.g. A → B → A).
            if self.loading_files.contains(&canonical) {
                return Err(make_err(TbxError::CircularUse {
                    path: canonical.display().to_string(),
                }));
            }
            // canonicalize() succeeded, so the file exists.  If read_to_string
            // fails here (e.g. permission denied), we still report FileNotFound
            // because there is no separate "file unreadable" error variant.
            // The reason string (e.g. "Permission denied (os error 13)") tells
            // the user the actual cause.
            let source = std::fs::read_to_string(&canonical).map_err(|e| {
                make_err(TbxError::FileNotFound {
                    path: canonical.display().to_string(),
                    reason: format!("read failed: {e}"),
                })
            })?;
            self.loading_files.insert(canonical.clone());
            self.use_depth += 1;
            let result = self.exec_source(&source);
            self.use_depth -= 1;
            self.loading_files.remove(&canonical);
            result?;
        }

        Ok(())
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
        let state = self.vm.compile_state.as_mut().ok_or_else(|| {
            make_err(TbxError::InvalidExpression {
                reason: "register_label called outside compile mode",
            })
        })?;

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

    /// Compile and run a full TBX source program in program mode.
    ///
    /// Processes `source` in a single pass:
    /// - DEF blocks are compiled into the dictionary immediately (existing `is_compiling` flow).
    /// - Ground-level (non-DEF) statements are collected into a main-routine buffer and
    ///   executed as a single unit after all line processing completes.
    ///
    /// This is the "full program mode" entry point; use `exec_source` for interactive mode.
    pub fn compile_program(&mut self, source: &str) -> Result<(), InterpreterError> {
        let mut main_cells: Vec<Cell> = Vec::new();
        let mut stmt_positions: Vec<StmtPosition> = Vec::new();

        for (line_idx, line) in source.lines().enumerate() {
            let line_num = line_idx + 1; // 1-based line number
            let (tokens, boundaries) = self.parse_line_into_segments(line)?;
            for (seg_start, seg_end) in boundaries {
                let was_compiling = self.vm.compile_state.is_some();
                self.compile_program_segment(
                    &tokens[seg_start..seg_end],
                    line,
                    &mut main_cells,
                    &mut stmt_positions,
                    line_num,
                )?;
                // If DEF just started on this segment, record the source line number.
                if !was_compiling {
                    if let Some(state) = &mut self.vm.compile_state {
                        state.start_line = line_num;
                    }
                }
            }
        }

        // --- Finalise: build and run the main routine ---

        // Guard: if a DEF block was left unclosed (no matching END), roll back the
        // partial definition and return an error.  Leaving compile_state set would
        // corrupt subsequent calls because every statement would be treated as part
        // of the unfinished word body.
        if self.vm.compile_state.is_some() {
            // Capture the word name and DEF start line before rollback for a more informative error message.
            let word_name = self
                .vm
                .compile_state
                .as_ref()
                .map(|s| s.word_name.clone())
                .unwrap_or_default();
            let def_start_line = self
                .vm
                .compile_state
                .as_ref()
                .map(|s| s.start_line)
                .unwrap_or(0);
            self.vm.rollback_def();
            return Err(InterpreterError::new(
                def_start_line,
                0,
                &format!("DEF {word_name}"),
                TbxError::InvalidExpression {
                    reason: "DEF without matching END at end of source",
                },
            ));
        }

        // If there are no ground-level statements, nothing to execute.
        if main_cells.is_empty() {
            return Ok(());
        }

        let main_start = self.vm.dp;

        // Write collected ground-level cells to the dictionary.
        for cell in main_cells {
            if let Err(e) = self.vm.dict_write(cell) {
                self.vm.dp = main_start;
                self.vm.dictionary.truncate(main_start);
                return Err(InterpreterError::new(0, 0, "", e));
            }
        }

        // Append EXIT to terminate the main routine.
        let exit_xt = match self.lookup_required("EXIT", 0, 0, "") {
            Ok(xt) => xt,
            Err(e) => {
                self.vm.dp = main_start;
                self.vm.dictionary.truncate(main_start);
                return Err(e);
            }
        };
        if let Err(e) = self.vm.dict_write(Cell::Xt(exit_xt)) {
            self.vm.dp = main_start;
            self.vm.dictionary.truncate(main_start);
            return Err(InterpreterError::new(0, 0, "", e));
        }

        // Save stack/bp state for rollback on runtime error or HALT.
        let saved_data_stack_len = self.vm.data_stack.len();
        let saved_return_stack_len = self.vm.return_stack.len();
        let saved_bp = self.vm.bp;

        // Capture main-routine size before execution so that ALLOT or other dp-advancing
        // operations inside vm.run() do not skew the length used for position lookup.
        let main_len = self.vm.dp - main_start;

        // Execute the main routine.
        let run_result = self.vm.run(main_start);
        let error_pc = self.vm.pc;

        // Release main-routine memory regardless of outcome.
        self.vm.dp = main_start;
        self.vm.dictionary.truncate(main_start);

        match run_result {
            Ok(()) => Ok(()),
            // HALT is normal termination in program mode.
            // Restore stacks because DROP_TO_MARKER may not have run after HALT.
            Err(TbxError::Halted) => {
                self.vm.data_stack.truncate(saved_data_stack_len);
                self.vm.return_stack.truncate(saved_return_stack_len);
                self.vm.bp = saved_bp;
                Ok(())
            }
            Err(e) => {
                let (line, col, source) = resolve_source_pos(
                    error_pc,
                    &self.vm.return_stack,
                    main_start,
                    main_len,
                    &stmt_positions,
                );
                self.vm.data_stack.truncate(saved_data_stack_len);
                self.vm.return_stack.truncate(saved_return_stack_len);
                self.vm.bp = saved_bp;
                Err(InterpreterError::new(line, col, &source, e))
            }
        }
    }

    /// Process a single statement segment in program-compile mode.
    ///
    /// Behaves like `exec_segment`, except ground-level (non-IMMEDIATE, non-DEF-body)
    /// statements are not executed immediately; instead their compiled cells are drained
    /// from the dictionary into `main_cells` for deferred execution.
    ///
    /// `stmt_positions` receives one entry per ground-level statement compiled:
    /// `(start_offset_in_main_cells, line, col, source_line_text)`.
    ///
    /// `absolute_line` is the 1-based line number of this segment in the full source file
    /// (the token positions produced by `parse_line_into_segments` are relative to a single
    /// line and cannot be used for source-level position recording).
    fn compile_program_segment(
        &mut self,
        tokens: &[SpannedToken],
        source_line: &str,
        main_cells: &mut Vec<Cell>,
        stmt_positions: &mut Vec<StmtPosition>,
        absolute_line: usize,
    ) -> Result<(), InterpreterError> {
        let mut idx = 0;

        // Extract statement name.
        let stmt_tok = &tokens[idx];
        let stmt_name = match &stmt_tok.token {
            Token::Ident(name) => name.clone(),
            _ => return Ok(()), // not an identifier — skip
        };
        let stmt_pos_line = stmt_tok.pos.line;
        let stmt_pos_col = stmt_tok.pos.col;
        idx += 1;

        // Normalize to uppercase for case-insensitive keyword matching.
        let stmt_name = stmt_name.to_ascii_uppercase();

        // Handle REM: skip the rest of the segment.
        if stmt_name == "REM" {
            return Ok(());
        }

        // IMMEDIATE word dispatch: execute immediately regardless of compile/interpret mode.
        // Delegates to exec_immediate_word helper to avoid code duplication with exec_segment.
        if let Some(xt) = self.vm.lookup(&stmt_name) {
            let flags = self.vm.headers[xt.index()].flags;
            if flags & crate::dict::FLAG_IMMEDIATE != 0 {
                return self.exec_immediate_word(
                    xt,
                    &tokens[idx..],
                    stmt_pos_line,
                    stmt_pos_col,
                    source_line,
                );
            }
        }

        // Inside a DEF body: write statement to dictionary directly (same as exec_segment).
        if self.vm.compile_state.is_some() {
            let result = self.write_stmt_to_dict(
                &stmt_name,
                &tokens[idx..],
                stmt_pos_line,
                stmt_pos_col,
                source_line,
            );
            if result.is_err() {
                self.vm.rollback_def();
            }
            return result;
        }

        // Ground-level statement: compile to a temporary dict area, then drain into main_cells.
        let buf_start = self.vm.dp;
        if let Err(e) = self.write_stmt_to_dict(
            &stmt_name,
            &tokens[idx..],
            stmt_pos_line,
            stmt_pos_col,
            source_line,
        ) {
            self.vm.dp = buf_start;
            self.vm.dictionary.truncate(buf_start);
            return Err(e);
        }

        // Record the offset of this statement in main_cells for source-position lookup.
        let stmt_offset = main_cells.len();

        // Drain the newly written cells from the dictionary into the main-cells buffer.
        // This keeps the dictionary region clean so that subsequent DEF compilations
        // do not interleave with ground-level code.
        main_cells.extend(self.vm.dictionary.drain(buf_start..));
        self.vm.dp = buf_start;

        // Only record an entry when at least one cell was produced.
        if main_cells.len() > stmt_offset {
            stmt_positions.push(StmtPosition {
                offset: stmt_offset,
                line: absolute_line,
                col: stmt_pos_col,
                source: source_line.to_string(),
            });
        }

        Ok(())
    }
}

/// Source-position metadata for one ground-level statement compiled by `compile_program_segment`.
#[derive(Debug)]
struct StmtPosition {
    /// Offset of the statement's first cell in the `main_cells` buffer.
    offset: usize,
    /// 1-based line number in the full source file.
    line: usize,
    /// 1-based column number of the statement keyword.
    col: usize,
    /// Full text of the source line containing the statement.
    source: String,
}

/// Resolve the source position for a runtime error that occurred during `compile_program`.
///
/// Looks up `stmt_positions` — a table of `StmtPosition` entries built by
/// `compile_program_segment` — to find the statement that was executing when the error occurred.
///
/// Two strategies are attempted in order:
///
/// 1. **Direct PC match**: if `error_pc` falls inside `[main_start, main_start + main_len)`,
///    use `offset = error_pc - main_start` and search the table.
/// 2. **Return-stack scan**: walk `return_stack` from the end (most-recently-pushed) toward
///    the front, looking for a `ReturnFrame::Call { return_pc }` whose `return_pc` falls
///    inside the main-routine range `(main_start, main_start + main_len)`.
///    Use `offset = return_pc - main_start - 1` (points at the call cell) and search the table.
///
/// The table search finds the entry with the largest `offset` that is ≤ the computed offset.
///
/// Returns `(0, 0, String::new())` when neither strategy finds a match (fallback).
fn resolve_source_pos(
    error_pc: usize,
    return_stack: &[ReturnFrame],
    main_start: usize,
    main_len: usize,
    stmt_positions: &[StmtPosition],
) -> (usize, usize, String) {
    let lookup = |offset: usize| -> Option<(usize, usize, String)> {
        stmt_positions
            .iter()
            .rev()
            .find(|sp| sp.offset <= offset)
            .map(|sp| (sp.line, sp.col, sp.source.clone()))
    };

    // Strategy 1: error_pc is inside the main routine.
    if error_pc >= main_start && error_pc < main_start + main_len {
        let offset = error_pc - main_start;
        if let Some(pos) = lookup(offset) {
            return pos;
        }
    }

    // Strategy 2: scan return stack for a call frame pointing just after the main routine.
    // The main routine spans [main_start, main_start + main_len); EXIT occupies the last cell.
    // A valid return_pc from a call inside the main routine must satisfy:
    //   main_start < return_pc < main_start + main_len
    // (return_pc = pc + 1 for EntryKind::Word, pc + 4 for EntryKind::Call; both are
    // strictly less than main_start + main_len because EXIT follows the last statement.)
    for frame in return_stack.iter().rev() {
        if let ReturnFrame::Call { return_pc, .. } = frame {
            if *return_pc > main_start && *return_pc < main_start + main_len {
                let offset = return_pc - main_start - 1;
                if let Some(pos) = lookup(offset) {
                    return pos;
                }
            }
        }
    }

    (0, 0, String::new())
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
    fn test_end_outside_def_returns_error() {
        // END outside DEF is handled by end_prim (FLAG_IMMEDIATE), which checks
        // is_compiling and returns InvalidExpression when called in interpret mode.
        let mut interp = Interpreter::new();
        assert!(interp.exec_line("END").is_err());
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
  SET &I, 1
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
SET &X, 99
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
  SET &R, X + 1
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
  SET &R, X + 1
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
  SET &I, 1
  10
    PUTDEC I
    PUTSTR "\n"
    SET &I, I + 1
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
  SET &I, 0
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
  SET &I, 1
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
        // Regression test for issue #222: self-recursive call inside a SET expression
        // (processed by ExprCompiler) must have its local_count back-patched.
        // Previously, local_count=0 was permanently embedded and caused IndexOutOfBounds
        // at runtime when the VAR slot was accessed.
        // Uses BIT (branch if true) so the base case label is reached when N <= 1 is true.
        let src = r#"
DEF FACT(N)
  VAR R
  BIT N <= 1, 10
    SET &R, N * FACT(N - 1)
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

    #[test]
    fn test_recursive_self_call_in_return_expr() {
        // Regression test for issue #222: self-recursive call inside RETURN expression
        // (compile_return path) must have its local_count back-patched.
        let src = r#"
DEF FACT(N)
  VAR R
  BIT N <= 1, 10
    SET &R, N - 1
    RETURN N * FACT(R)
  10 RETURN 1
END
PUTDEC FACT(5)
PUTSTR "\n"
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "120\n");
    }

    // --- issue #234: semicolon-separated multiple statements ---

    #[test]
    fn test_semicolon_two_statements_interpret_mode() {
        // Two statements on one line separated by semicolon must both execute.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTSTR \"a\"; PUTDEC 42").unwrap();
        assert_eq!(interp.take_output(), "a42");
    }

    #[test]
    fn test_semicolon_three_statements() {
        // Three semicolon-separated statements must all execute in order.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1; PUTDEC 2; PUTDEC 3").unwrap();
        assert_eq!(interp.take_output(), "123");
    }

    #[test]
    fn test_semicolon_trailing() {
        // A trailing semicolon (empty last segment) must be silently ignored.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1;").unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_rem_stops_execution() {
        // REM causes the lexer to consume the rest of the input, so statements
        // after a REM segment are never seen.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1; REM x; PUTDEC 2").unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_with_paren_args() {
        // Parenthesised arguments must not be confused with segment boundaries.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC ADD(1,2); PUTDEC 3").unwrap();
        assert_eq!(interp.take_output(), "33");
    }

    #[test]
    fn test_semicolon_in_def_block() {
        // Semicolons inside a DEF block must compile each segment independently.
        let mut interp = Interpreter::new();
        let src = "\
DEF GREET
  PUTSTR \"hi\"; PUTSTR \"\\n\"
END
GREET";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "hi\n");
    }

    #[test]
    fn test_semicolon_partial_exec_on_error() {
        // When a later segment errors, prior segments have already executed.
        // This documents the expected partial-execution semantics.
        let mut interp = Interpreter::new();
        let result = interp.exec_line("PUTDEC 1; NOSUCHWORD");
        assert!(result.is_err(), "second segment should return an error");
        // First segment's output is already flushed.
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_leading() {
        // A leading semicolon produces an empty first segment, which is skipped.
        let mut interp = Interpreter::new();
        interp.exec_line("; PUTDEC 1").unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_consecutive() {
        // Consecutive semicolons produce empty segments that are silently skipped.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1;; PUTDEC 2").unwrap();
        assert_eq!(interp.take_output(), "12");
    }

    #[test]
    fn test_recursive_self_call_in_bif_condition() {
        // Regression test for issue #222: self-recursive call inside BIF/BIT condition
        // expression (compile_branch path) must have its local_count back-patched.
        //
        // MYGT(R) appears directly inside the BIT condition `MYGT(R) > 0`, which is
        // compiled by compile_branch via ExprCompiler. Without the fix, local_count=0
        // would be permanently embedded, causing IndexOutOfBounds when the VAR R slot
        // is accessed inside the recursive call.
        //
        // Trace: MYGT(3)->2, MYGT(2)->2, MYGT(1)->1, MYGT(0)->0
        let src = r#"
DEF MYGT(N)
  VAR R
  BIT N <= 0, 10
    SET &R, N - 1
    BIT MYGT(R) > 0, 20
      RETURN 1
    20 RETURN 2
  10 RETURN 0
END
PUTDEC MYGT(3)
PUTSTR "\n"
"#;
        let mut interp = Interpreter::new();
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "2\n");
    }

    // --- user-defined IMMEDIATE word dispatch (issue #245) ---

    #[test]
    fn test_user_defined_immediate_word_executes_in_interpret_mode() {
        // A user word flagged as IMMEDIATE should execute immediately in interpret mode.
        let mut interp = Interpreter::new();
        let src = "\
DEF IWORD
PUTDEC 99
END
IMMEDIATE IWORD
IWORD";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "99", "expected '99' in output, got: {:?}", out);
    }

    #[test]
    fn test_user_defined_immediate_word_executes_during_compile() {
        // A user word flagged as IMMEDIATE should execute at compile time, not be compiled into
        // the calling word's body.

        let mut interp = Interpreter::new();

        // Phase 1: define IWORD, mark it IMMEDIATE, then compile OUTER which references IWORD.
        // Because IWORD is IMMEDIATE it should be executed immediately during compilation of OUTER
        // and must NOT be stored into OUTER's body.
        interp
            .exec_source(
                "\
DEF IWORD
PUTDEC 77
END
IMMEDIATE IWORD
DEF OUTER
IWORD
END",
            )
            .unwrap();

        // IWORD ran once at compile time, so output should be "77".
        let compile_out = interp.take_output();
        assert_eq!(
            compile_out, "77",
            "IWORD must execute during compilation of OUTER (got: {compile_out:?})"
        );

        // Phase 2: call OUTER at runtime. Since IWORD was not compiled into OUTER's body,
        // executing OUTER should produce no output.
        interp.exec_source("OUTER").unwrap();
        let runtime_out = interp.take_output();
        assert_eq!(
            runtime_out, "",
            "OUTER must not re-execute IWORD at runtime (got: {runtime_out:?})"
        );
    }

    #[test]
    fn test_user_defined_immediate_word_with_locals_returns_error() {
        // A user word with VAR locals cannot be IMMEDIATE-dispatched directly
        // because vm.run() does not set up the CALL frame (bp / local slots).
        let mut interp = Interpreter::new();
        let src = "\
DEF ILOCAL
VAR X
PUTDEC 1
END
IMMEDIATE ILOCAL
ILOCAL";
        let result = interp.exec_source(src);
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word has VAR locals"
        );
    }

    #[test]
    fn test_user_defined_immediate_word_with_arity_returns_error() {
        // A user word with formal parameters (arity > 0) cannot be IMMEDIATE-dispatched
        // directly because vm.run() does not set up the CALL frame.
        let mut interp = Interpreter::new();
        let src = "\
DEF IPARAM(X)
PUTDEC X
END
IMMEDIATE IPARAM
IPARAM 42";
        let result = interp.exec_source(src);
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word has formal parameters"
        );
    }

    #[test]
    fn test_immediate_on_constant_returns_error_and_rolls_back() {
        // Applying FLAG_IMMEDIATE to a Constant dictionary entry and invoking it should
        // return an error (the `_ =>` branch) and roll back compile_state so
        // that the interpreter can be reused normally.
        let mut interp = Interpreter::new();

        // Register a Constant entry with FLAG_IMMEDIATE directly via the VM,
        // since TBX has no built-in CONSTANT keyword.
        {
            use crate::cell::Cell;
            use crate::dict::{WordEntry, FLAG_IMMEDIATE};
            let mut entry = WordEntry::new_constant("MAGIC", Cell::Int(42));
            entry.flags |= FLAG_IMMEDIATE;
            interp.vm.register(entry);
        }

        // Calling the IMMEDIATE-flagged constant inside a DEF should fail.
        let bad = "DEF BAD\nMAGIC\nEND";
        assert!(
            interp.exec_source(bad).is_err(),
            "expected error when an IMMEDIATE Constant is dispatched"
        );

        // After the error the interpreter must be fully recovered: define and
        // call a valid word to confirm compile_state was properly rolled back.
        interp
            .exec_source("DEF OK\nPUTDEC 7\nEND\nOK")
            .expect("interpreter should be reusable after IMMEDIATE-Constant error");
        assert_eq!(
            interp.take_output(),
            "7",
            "expected '7' from OK after rollback"
        );
    }

    #[test]
    fn test_immediate_on_variable_returns_error_and_rolls_back() {
        // VAR X outside a DEF creates a global Variable entry. Marking it IMMEDIATE
        // and invoking it inside a DEF should trigger the `_ =>` branch, return an
        // error, and leave the interpreter in a clean state.
        let mut interp = Interpreter::new();

        // VAR outside DEF creates a global Variable; IMMEDIATE marks it FLAG_IMMEDIATE.
        interp.exec_source("VAR V\nIMMEDIATE V").unwrap();

        let bad = "DEF BAD\nV\nEND";
        assert!(
            interp.exec_source(bad).is_err(),
            "expected error when an IMMEDIATE Variable is dispatched"
        );

        // The interpreter should be reusable after the rollback.
        interp
            .exec_source("DEF OK2\nPUTDEC 8\nEND\nOK2")
            .expect("interpreter should be reusable after IMMEDIATE-Variable error");
        assert_eq!(
            interp.take_output(),
            "8",
            "expected '8' from OK2 after rollback"
        );
    }

    // --- compile_program (issue #263: full program mode) ---

    #[test]
    fn test_compile_program_ground_only() {
        // Ground-level statements only (no DEF); should execute in order.
        let mut interp = Interpreter::new();
        let src = r#"
PUTDEC 1
PUTSTR "\n"
PUTDEC 2
PUTSTR "\n"
"#;
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "1\n2\n");
    }

    #[test]
    fn test_compile_program_def_then_ground() {
        // DEF defined first, called from ground-level afterward.
        let mut interp = Interpreter::new();
        let src = r#"
DEF GREET
  PUTDEC 42
  PUTSTR "\n"
END
GREET
"#;
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "42\n");
    }

    #[test]
    fn test_compile_program_interleaved() {
        // DEF → ground → DEF → ground.
        // The second DEF word must be callable from the second ground statement.
        let mut interp = Interpreter::new();
        let src = r#"
DEF FIRST
  PUTDEC 1
END
FIRST
DEF SECOND
  PUTDEC 2
END
SECOND
"#;
        interp.compile_program(src).unwrap();
        // Both ground statements must run; FIRST must be callable from ground.
        assert_eq!(interp.take_output(), "12");
    }

    #[test]
    fn test_compile_program_halt_terminates_normally() {
        // HALT in ground-level code should terminate without error.
        let mut interp = Interpreter::new();
        let src = r#"
PUTDEC 7
HALT
PUTDEC 99
"#;
        interp.compile_program(src).unwrap();
        // Only the output before HALT should appear.
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_compile_program_undefined_word_is_error() {
        // Calling a non-existent word in ground-level code must return an error.
        let mut interp = Interpreter::new();
        let result = interp.compile_program("NOSUCHWORD 1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err.kind, TbxError::UndefinedSymbol { .. }),
            "expected UndefinedSymbol, got: {:?}",
            err.kind
        );
    }

    #[test]
    fn test_compile_program_exec_source_still_passes() {
        // Verify that exec_source-based tests are unaffected (regression guard).
        let mut interp = Interpreter::new();
        let src = "DEF GREET\nPUTDEC 42\nEND\nGREET";
        interp.exec_source(src).unwrap();
        assert_eq!(
            interp.take_output(),
            "42",
            "expected '42' from exec_source regression check"
        );
    }

    #[test]
    fn test_compile_program_unclosed_def_is_error() {
        // A DEF without a matching END must return an error and leave the VM in a
        // clean state (compile_state = None) so that subsequent calls work correctly.
        let mut interp = Interpreter::new();
        let result = interp.compile_program("DEF NOEND\nPUTDEC 1");
        assert!(result.is_err(), "expected error for unclosed DEF");
        assert!(
            matches!(result.unwrap_err().kind, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for unclosed DEF"
        );
        // The VM must be reusable after rollback.
        interp
            .compile_program("PUTDEC 99")
            .expect("should succeed after unclosed-DEF rollback");
        assert_eq!(interp.take_output(), "99");
    }

    #[test]
    fn test_compile_program_unclosed_def_reports_def_line() {
        // The error for an unclosed DEF must carry the 1-based line number of
        // the DEF keyword, not 0.
        let mut interp = Interpreter::new();
        // DEF is on line 3 (1-based).
        let src = "PUTDEC 1\nPUTDEC 2\nDEF NOEND\nPUTDEC 3";
        let result = interp.compile_program(src);
        assert!(result.is_err(), "expected error for unclosed DEF");
        let err = result.unwrap_err();
        assert_eq!(
            err.line, 3,
            "error line should point to the DEF line (3), got {}",
            err.line
        );
    }

    #[test]
    fn test_compile_program_line_number_outside_def_ignored() {
        // Line-number labels appearing outside a DEF block are silently skipped;
        // the following statement must still execute normally.
        let mut interp = Interpreter::new();
        interp.compile_program("10 PUTDEC 1").unwrap();
        assert_eq!(
            interp.take_output(),
            "1",
            "statement after ignored line-number label should execute"
        );
    }

    #[test]
    fn test_compile_program_runtime_error_cleans_up() {
        // A runtime error (e.g. division by zero) must return Err and leave the
        // VM in a reusable state so that a subsequent compile_program call works.
        let mut interp = Interpreter::new();
        // Division by zero triggers a runtime error inside vm.run().
        let result = interp.compile_program("PUTDEC 1 / 0");
        assert!(
            result.is_err(),
            "expected error from runtime division by zero"
        );
        // The data stack must be restored so that the VM is reusable.
        interp
            .compile_program("PUTDEC 42")
            .expect("compile_program should succeed after runtime error");
        assert_eq!(interp.take_output(), "42");
    }

    #[test]
    fn test_compile_program_runtime_error_line_number() {
        // A runtime error must carry the correct 1-based source line number,
        // not the placeholder 0 that was used before issue #275 was fixed.
        let mut interp = Interpreter::new();
        // The division-by-zero is on line 2.
        let src = "PUTDEC 1\nPUTDEC 1 / 0\nPUTDEC 3";
        let result = interp.compile_program(src);
        let err = result.expect_err("expected runtime error from division by zero");
        assert_ne!(
            err.line, 0,
            "runtime error line must not be 0 (was: {})",
            err.line
        );
        assert_eq!(
            err.line, 2,
            "runtime error must point to line 2, got {}",
            err.line
        );
        // Column and source line are also expected to be accurate.
        assert_eq!(
            err.col, 1,
            "column should point to the start of the PUTDEC keyword (col 1), got {}",
            err.col
        );
        assert!(
            err.source_line.contains("1 / 0"),
            "source_line should contain the failing expression, got: {:?}",
            err.source_line
        );
    }

    #[test]
    fn test_compile_program_runtime_error_in_user_word_line_number() {
        // A runtime error inside a user-defined word called from line 6 must
        // report line 6 (the call site in the main routine).
        let mut interp = Interpreter::new();
        let src = "DEF BAD_WORD\n  PUTDEC 1 / 0\nEND\nPUTDEC 1\nPUTDEC 2\nBAD_WORD";
        let result = interp.compile_program(src);
        let err = result.expect_err("expected runtime error from division by zero in user word");
        assert_ne!(
            err.line, 0,
            "runtime error line must not be 0 (was: {})",
            err.line
        );
        assert_eq!(
            err.line, 6,
            "runtime error must point to line 6 (the BAD_WORD call site), got {}",
            err.line
        );
        // Column and source line are also expected to be accurate.
        assert_eq!(
            err.col, 1,
            "column should point to the start of BAD_WORD keyword (col 1), got {}",
            err.col
        );
        assert!(
            err.source_line.contains("BAD_WORD"),
            "source_line should contain the call site identifier, got: {:?}",
            err.source_line
        );
    }

    // --- compile_program integration tests (issue #266) ---

    #[test]
    fn test_compile_program_forward_reference_is_error() {
        // In single-pass compilation, a ground-level reference to a word that
        // has not yet been defined (i.e. the DEF appears after the reference)
        // must produce an UndefinedSymbol error.
        let mut interp = Interpreter::new();
        let src = "FORWARD_WORD\nDEF FORWARD_WORD\n  PUTDEC 1\nEND";
        let result = interp.compile_program(src);
        let err =
            result.expect_err("expected UndefinedSymbol error for forward reference, but got Ok");
        assert!(
            matches!(err.kind, TbxError::UndefinedSymbol { .. }),
            "expected UndefinedSymbol error kind"
        );
        // The VM must be reusable after the error (no compile state left over).
        interp
            .compile_program("PUTDEC 1")
            .expect("VM should be reusable after UndefinedSymbol error");
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_exec_line_then_compile_program_coexistence() {
        // A word defined via exec_line must be callable from a subsequent
        // compile_program on the same Interpreter instance.
        let mut interp = Interpreter::new();
        interp.exec_line("DEF HELLO").unwrap();
        interp.exec_line("PUTDEC 99").unwrap();
        interp.exec_line("END").unwrap();
        interp.compile_program("HELLO").unwrap();
        assert_eq!(interp.take_output(), "99");
    }

    #[test]
    fn test_compile_program_then_exec_line_coexistence() {
        // A word defined inside compile_program must be callable via exec_line
        // on the same Interpreter instance afterwards.
        let mut interp = Interpreter::new();
        interp
            .compile_program("DEF ADD1(X)\nRETURN X + 1\nEND")
            .unwrap();
        interp.exec_line("PUTDEC ADD1(41)").unwrap();
        assert_eq!(interp.take_output(), "42");
    }

    #[test]
    fn test_compile_program_ground_deferred_execution() {
        // Proves that ground-level statements are deferred: they are collected into
        // a main routine and executed AFTER all IMMEDIATE words have been processed.
        //
        // The key observable property: an IMMEDIATE word that appears between two
        // ground statements runs at compile time, so its output appears BEFORE the
        // output of both ground statements, even though it sits between them in source.
        //
        // Source order:
        //   PUTDEC 1              ← ground stmt (deferred)
        //   IMMEDIATE SHOW        ← marks SHOW as IMMEDIATE (flag-set only, no output yet)
        //   SHOW                  ← now IMMEDIATE: runs at compile time → outputs "99"
        //   PUTDEC 2              ← ground stmt (deferred)
        //
        // Deferred execution produces: "99" (compile-time) + "1" + "2" (runtime) = "9912".
        // If ground statements executed inline, PUTDEC 1 would run before SHOW:
        // output would be "1" + "99" + "2" = "1992" — a different result.
        let mut interp = Interpreter::new();
        let src = "\
DEF SHOW
  PUTDEC 99
END
PUTDEC 1
IMMEDIATE SHOW
SHOW
PUTDEC 2";
        interp.compile_program(src).unwrap();
        assert_eq!(
            interp.take_output(),
            "9912",
            "expected IMMEDIATE output ('99') before deferred ground output ('12')"
        );
    }

    // --- compile_program + IMMEDIATE (issue #264) ---

    // --- compile_program: ground-level IMMEDIATE execution order (issue #277) ---

    #[test]
    fn test_compile_program_immediate_executes_before_main_cells() {
        // An IMMEDIATE word at ground level executes during the compile phase, so its
        // output appears before the output produced by main_cells at run time.
        // Source:
        //   DEF IWORD / PUTSTR "IM" / END
        //   IMMEDIATE IWORD
        //   PUTSTR "BEFORE"   <- deferred (compiled into main_cells)
        //   IWORD             <- IMMEDIATE word (executes immediately)
        //   PUTSTR "AFTER"    <- deferred (compiled into main_cells)
        //
        // Expected: "IM" is emitted during compile, then "BEFORE" and "AFTER" at run
        // time, giving "IMBEFOREAFTER".
        let mut interp = Interpreter::new();
        let src = "\
DEF IWORD
PUTSTR \"IM\"
END
IMMEDIATE IWORD
PUTSTR \"BEFORE\"
IWORD
PUTSTR \"AFTER\"";
        interp.compile_program(src).unwrap();
        let out = interp.take_output();
        assert_eq!(
            out, "IMBEFOREAFTER",
            "IMMEDIATE word should produce output during compile, main_cells run after; got: {out:?}"
        );
    }

    #[test]
    fn test_compile_program_immediate_not_compiled_into_main_cells() {
        // An IMMEDIATE word at ground level must NOT be compiled into main_cells, so
        // it is executed exactly once (during compile) and never again at run time.
        // Source:
        //   DEF ONCE / PUTSTR "X" / END
        //   IMMEDIATE ONCE
        //   ONCE              <- this line is the IMMEDIATE invocation
        //
        // Expected: "X" appears once only.
        let mut interp = Interpreter::new();
        let src = "\
DEF ONCE
PUTSTR \"X\"
END
IMMEDIATE ONCE
ONCE";
        interp.compile_program(src).unwrap();
        let out = interp.take_output();
        assert_eq!(
            out, "X",
            "IMMEDIATE word should execute once during compile and not be added to main_cells; got: {out:?}"
        );
    }

    #[test]
    fn test_compile_program_multiple_immediate_words_execute_in_order() {
        // Multiple IMMEDIATE words at ground level must execute in source order.
        // Source structure:
        //   line 1: DEF A … END
        //   line 2: IMMEDIATE A          <- marks A as immediate
        //   line 3: DEF B … END
        //   line 4: IMMEDIATE B          <- marks B as immediate
        //   line 5: A                    <- ground-level call; A is IMMEDIATE → runs now
        //   line 6: B                    <- ground-level call; B is IMMEDIATE → runs now
        //   line 7: PUTSTR "C"           <- deferred into main_cells
        //
        // Compile phase: lines 5 and 6 execute immediately → "AB"
        // Run phase:     line 7 executes → "C"
        // Total expected output: "ABC"
        let mut interp = Interpreter::new();
        let src = "\
DEF A
PUTSTR \"A\"
END
IMMEDIATE A
DEF B
PUTSTR \"B\"
END
IMMEDIATE B
A
B
PUTSTR \"C\"";
        interp.compile_program(src).unwrap();
        let out = interp.take_output();
        assert_eq!(
            out, "ABC",
            "multiple IMMEDIATE words should execute in source order during compile; got: {out:?}"
        );
    }

    #[test]
    fn test_compile_program_immediate_at_ground_level() {
        // An IMMEDIATE word used at ground level during compile_program should execute
        // immediately (not be compiled into the main routine), and subsequent ground-level
        // statements must still be compiled and executed normally.
        let mut interp = Interpreter::new();
        let src = "\
DEF IWORD
PUTDEC 55
END
IMMEDIATE IWORD
IWORD
PUTDEC 42";
        interp.compile_program(src).unwrap();
        // IWORD executes immediately at ground level (output "55").
        // The subsequent PUTDEC 42 is compiled into the main routine and executes after
        // the IMMEDIATE word, producing "42".
        let out = interp.take_output();
        assert_eq!(
            out, "5542",
            "expected '5542': IMMEDIATE word output followed by continued compilation, got: {out:?}"
        );
    }

    #[test]
    fn test_compile_program_immediate_in_expression_is_error() {
        // An IMMEDIATE word used inside an expression (argument to a statement) must
        // produce an InvalidExpression error.
        let mut interp = Interpreter::new();
        // Create a global variable V and mark it IMMEDIATE.
        interp.exec_source("VAR V\nIMMEDIATE V").unwrap();
        // Using V inside an expression should fail.
        let result = interp.compile_program("PUTDEC V");
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word appears inside an expression"
        );
        assert!(
            matches!(
                result.unwrap_err().kind,
                crate::error::TbxError::InvalidExpression { .. }
            ),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_exec_source_immediate_in_expression_is_error() {
        // Regression: interpreter mode (exec_source) should also reject IMMEDIATE words
        // inside expressions, returning InvalidExpression.
        let mut interp = Interpreter::new();
        interp.exec_source("VAR V\nIMMEDIATE V").unwrap();
        let result = interp.exec_source("PUTDEC V");
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word appears inside an expression in exec_source"
        );
        assert!(
            matches!(
                result.unwrap_err().kind,
                crate::error::TbxError::InvalidExpression { .. }
            ),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_compile_program_immediate_fname_call_in_expression_is_error() {
        // Verify that an IMMEDIATE word used in FNAME(args) form inside an expression
        // produces an InvalidExpression error (compile_program path).
        //
        // The FLAG_IMMEDIATE check in the expression evaluator runs before the
        // function-call syntax check, so FNAME(args) form also triggers InvalidExpression.
        let mut interp = Interpreter::new();
        // Define a plain Word entry and mark it IMMEDIATE.
        interp
            .exec_source("DEF IWORD(X)\nRETURN\nEND\nIMMEDIATE IWORD")
            .unwrap();
        // Using IWORD in FNAME(args) form inside an expression should fail.
        let result = interp.compile_program("PUTDEC IWORD(1)");
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word appears in FNAME(args) form inside an expression"
        );
        assert!(
            matches!(
                result.unwrap_err().kind,
                crate::error::TbxError::InvalidExpression { .. }
            ),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_compile_program_immediate_fname_zero_args_in_expression_is_error() {
        // Verify that an IMMEDIATE word used in FNAME() zero-argument form inside an
        // expression also produces an InvalidExpression error (compile_program path).
        let mut interp = Interpreter::new();
        interp
            .exec_source("DEF IWORD\nRETURN\nEND\nIMMEDIATE IWORD")
            .unwrap();
        let result = interp.compile_program("PUTDEC IWORD()");
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word appears in FNAME() form inside an expression"
        );
        assert!(
            matches!(
                result.unwrap_err().kind,
                crate::error::TbxError::InvalidExpression { .. }
            ),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_exec_source_immediate_fname_call_in_expression_is_error() {
        // Verify that an IMMEDIATE word used in FNAME(args) form inside an expression
        // produces an InvalidExpression error (exec_source path).
        let mut interp = Interpreter::new();
        interp
            .exec_source("DEF IWORD(X)\nRETURN\nEND\nIMMEDIATE IWORD")
            .unwrap();
        let result = interp.exec_source("PUTDEC IWORD(1)");
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word appears in FNAME(args) form inside an expression (exec_source)"
        );
        assert!(
            matches!(
                result.unwrap_err().kind,
                crate::error::TbxError::InvalidExpression { .. }
            ),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_exec_source_immediate_fname_zero_args_in_expression_is_error() {
        // Verify that an IMMEDIATE word used in FNAME() zero-argument form inside an
        // expression also produces an InvalidExpression error (exec_source path).
        let mut interp = Interpreter::new();
        interp
            .exec_source("DEF IWORD\nRETURN\nEND\nIMMEDIATE IWORD")
            .unwrap();
        let result = interp.exec_source("PUTDEC IWORD()");
        assert!(
            result.is_err(),
            "expected error when IMMEDIATE word appears in FNAME() form inside an expression (exec_source)"
        );
        assert!(
            matches!(
                result.unwrap_err().kind,
                crate::error::TbxError::InvalidExpression { .. }
            ),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_immediate_in_def_body_expression_is_error() {
        // An IMMEDIATE word used inside an expression within a DEF body must also
        // produce an InvalidExpression error (both in exec_source and compile_program).
        //
        // exec_source path
        {
            let mut interp = Interpreter::new();
            interp.exec_source("VAR V\nIMMEDIATE V").unwrap();
            let result = interp.exec_source("DEF FOO\nPUTDEC V\nEND");
            assert!(
                result.is_err(),
                "expected error when IMMEDIATE word appears in DEF body expression (exec_source)"
            );
            assert!(
                matches!(
                    result.unwrap_err().kind,
                    crate::error::TbxError::InvalidExpression { .. }
                ),
                "expected TbxError::InvalidExpression (exec_source)"
            );
        }
        // compile_program path
        {
            let mut interp = Interpreter::new();
            // Set up V as IMMEDIATE via exec_source, then compile a DEF body that uses it.
            interp.exec_source("VAR V\nIMMEDIATE V").unwrap();
            let result = interp.compile_program("DEF FOO\nPUTDEC V\nEND");
            assert!(
                result.is_err(),
                "expected error when IMMEDIATE word appears in DEF body expression (compile_program)"
            );
            assert!(
                matches!(
                    result.unwrap_err().kind,
                    crate::error::TbxError::InvalidExpression { .. }
                ),
                "expected TbxError::InvalidExpression (compile_program)"
            );
        }
    }

    // --- USE ---

    #[test]
    fn test_use_loads_and_executes_file() {
        // Create a temporary TBX file that defines a word.
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_path = dir.path().join("lib.tbx");
        std::fs::write(&lib_path, "DEF HELLO\nPUTSTR \"hello\"\nEND\n").unwrap();

        let mut interp = Interpreter::new();
        let src = format!("USE \"{}\"\nHELLO", lib_path.display());
        interp.exec_source(&src).unwrap();
        assert!(interp.take_output().contains("hello"));
    }

    #[test]
    fn test_use_compile_program_mode() {
        // USE must also work when called from compile_program (the full-program entry point).
        // This covers the compile_program_segment -> exec_immediate_word code path.
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_path = dir.path().join("lib.tbx");
        std::fs::write(&lib_path, "DEF GREET\nPUTSTR \"greet\"\nEND\n").unwrap();

        let mut interp = Interpreter::new();
        let src = format!("USE \"{}\"\nGREET", lib_path.display());
        interp.compile_program(&src).unwrap();
        assert!(interp.take_output().contains("greet"));
    }

    #[test]
    fn test_use_file_not_found_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_source("USE \"/nonexistent/path/does_not_exist.tbx\"");
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, TbxError::FileNotFound { .. }),
            "expected TbxError::FileNotFound"
        );
    }

    #[test]
    fn test_use_non_string_argument_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_source("USE 42");
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, TbxError::InvalidExpression { .. }),
            "expected TbxError::InvalidExpression"
        );
    }

    #[test]
    fn test_use_trailing_token_error() {
        // USE "path" EXTRA_TOKEN must return InvalidExpression.
        // The error is raised before file access, so a real file is not needed.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("USE \"/dummy_does_not_exist.tbx\" EXTRA");
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, TbxError::InvalidExpression { .. }),
            "expected TbxError::InvalidExpression for trailing token"
        );
    }

    #[test]
    fn test_use_inside_def_error() {
        // USE inside a DEF body must return InvalidExpression.
        // The error is raised before file access, so a real file is not needed.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF BADWORD\nUSE \"/dummy_does_not_exist.tbx\"\nEND");
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, TbxError::InvalidExpression { .. }),
            "expected TbxError::InvalidExpression when USE is inside DEF"
        );
    }

    #[test]
    fn test_use_self_reference_returns_circular_error() {
        // A file that USEs itself must be detected as a circular USE (not
        // UseNestingDepthExceeded) because loading_files catches the cycle.
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_path = dir.path().join("self_use.tbx");
        // Write a file that USEs itself.
        let content = format!("USE \"{}\"\n", lib_path.display());
        std::fs::write(&lib_path, &content).unwrap();

        let mut interp = Interpreter::new();
        let src = format!("USE \"{}\"", lib_path.display());
        let result = interp.exec_source(&src);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, TbxError::CircularUse { .. }),
            "expected TbxError::CircularUse for self-referencing USE"
        );
    }

    #[test]
    fn test_use_mutual_circular_returns_circular_error() {
        // A → B → A must be detected as circular USE.
        let dir = tempfile::tempdir().expect("tempdir");
        let path_a = dir.path().join("a.tbx");
        let path_b = dir.path().join("b.tbx");
        // a.tbx USEs b.tbx; b.tbx USEs a.tbx back.
        std::fs::write(&path_a, format!("USE \"{}\"\n", path_b.display())).unwrap();
        std::fs::write(&path_b, format!("USE \"{}\"\n", path_a.display())).unwrap();

        let mut interp = Interpreter::new();
        let src = format!("USE \"{}\"", path_a.display());
        let result = interp.exec_source(&src);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err().kind, TbxError::CircularUse { .. }),
            "expected TbxError::CircularUse for mutual circular USE (A→B→A)"
        );
    }

    #[test]
    fn test_use_linear_chain_succeeds() {
        // A non-circular chain A → B → C must succeed.
        let dir = tempfile::tempdir().expect("tempdir");
        let path_c = dir.path().join("c.tbx");
        let path_b = dir.path().join("b.tbx");
        let path_a = dir.path().join("a.tbx");
        std::fs::write(&path_c, "DEF HELLO\nPUTSTR \"hello\"\nEND\n").unwrap();
        std::fs::write(&path_b, format!("USE \"{}\"\n", path_c.display())).unwrap();
        std::fs::write(&path_a, format!("USE \"{}\"\n", path_b.display())).unwrap();

        let mut interp = Interpreter::new();
        let src = format!("USE \"{}\"", path_a.display());
        interp.exec_source(&src).unwrap();
        // HELLO (defined in c.tbx) must be callable after the chain completes.
        interp.exec_source("HELLO").unwrap();
        assert!(
            interp.take_output().contains("hello"),
            "linear chain A→B→C must succeed and define HELLO"
        );
    }

    #[test]
    fn test_use_nesting_depth_exceeded() {
        // Verify that a non-circular but excessively deep USE chain triggers
        // UseNestingDepthExceeded.  We reduce max_use_depth to 2 so only 3
        // temporary files are needed (A→B→C where C tries to USE D, which
        // exceeds the limit).
        //
        // The depth check fires before canonicalize(), so d.tbx does not need
        // to exist on disk — we never get that far.
        let dir = tempfile::tempdir().expect("tempdir");
        let path_c = dir.path().join("c.tbx");
        let path_b = dir.path().join("b.tbx");
        let path_a = dir.path().join("a.tbx");
        // Use a non-existent path for d.tbx; the depth check fires before
        // canonicalize() is called, so the file need not exist.
        let path_d = dir.path().join("d.tbx");
        std::fs::write(&path_c, format!("USE \"{}\"\n", path_d.display())).unwrap();
        std::fs::write(&path_b, format!("USE \"{}\"\n", path_c.display())).unwrap();
        std::fs::write(&path_a, format!("USE \"{}\"\n", path_b.display())).unwrap();

        let mut interp = Interpreter::new();
        // With max_use_depth=2, the chain A(depth=0)→B(depth=1)→C(depth=2)
        // reaches the limit when C tries to USE D.
        interp.set_max_use_depth(2);
        let src = format!("USE \"{}\"", path_a.display());
        let result = interp.exec_source(&src);
        assert!(result.is_err());
        assert!(
            matches!(
                result.unwrap_err().kind,
                TbxError::UseNestingDepthExceeded { .. }
            ),
            "expected TbxError::UseNestingDepthExceeded for non-circular deep USE"
        );
    }

    #[test]
    fn test_use_halt_in_loaded_file_does_not_stop_caller() {
        // HALT inside a USEd file terminates that file's execution but the
        // calling program continues (exec_source treats Halted as Ok(())).
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_path = dir.path().join("lib_halt.tbx");
        // GREET is defined before HALT; NEVER is defined after HALT.
        std::fs::write(
            &lib_path,
            "DEF GREET\nPUTSTR \"hi\"\nEND\nHALT\nDEF NEVER\nPUTSTR \"never\"\nEND\n",
        )
        .unwrap();

        let mut interp = Interpreter::new();
        let src = format!("USE \"{}\"", lib_path.display());
        // USE must succeed (HALT in the loaded file is not propagated to caller).
        interp.exec_source(&src).unwrap();

        // GREET (defined before HALT) must be available.
        interp.exec_source("GREET").unwrap();
        assert!(
            interp.take_output().contains("hi"),
            "GREET defined before HALT should be callable after USE"
        );

        // NEVER (defined after HALT) must NOT be available — confirms HALT
        // actually stopped file execution at the HALT line.
        let result = interp.exec_source("NEVER");
        assert!(
            result.is_err(),
            "NEVER defined after HALT should not be callable"
        );
        assert!(
            matches!(result.unwrap_err().kind, TbxError::UndefinedSymbol { .. }),
            "expected UndefinedSymbol for word defined after HALT"
        );
    }

    // --- IF / ENDIF (lib/basic.tbx) ---

    #[test]
    fn test_if_endif_condition_true_executes_body() {
        // IF with a true condition must execute the body.
        let mut interp = Interpreter::new();
        let src = "\
DEF CHECK(X)
  IF X > 0
    PUTSTR \"yes\"
  ENDIF
END
CHECK 5";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "yes", "expected 'yes', got: {:?}", out);
    }

    #[test]
    fn test_if_endif_condition_false_skips_body() {
        // IF with a false condition must skip the body.
        let mut interp = Interpreter::new();
        let src = "\
DEF CHECK(X)
  IF X > 0
    PUTSTR \"yes\"
  ENDIF
  PUTSTR \"done\"
END
CHECK 0";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "done", "expected 'done', got: {:?}", out);
    }

    #[test]
    fn test_if_endif_in_def_multiple_calls() {
        // A DEF containing IF/ENDIF must be callable multiple times with different results.
        let mut interp = Interpreter::new();
        let src = "\
DEF SIGN(X)
  IF X > 0
    PUTSTR \"+\"
  ENDIF
  IF X < 0
    PUTSTR \"-\"
  ENDIF
END
SIGN 1
SIGN -1
SIGN 0";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "+-", "expected '+-', got: {:?}", out);
    }

    #[test]
    fn test_if_endif_outside_def_is_error() {
        // IF/ENDIF outside a DEF body requires compile mode; using them at top level
        // (interpret mode) must return an error because COMPILE_EXPR needs compile mode.
        let mut interp = Interpreter::new();
        let result = interp.exec_line("IF 1 > 0");
        assert!(
            result.is_err(),
            "IF outside DEF should return an error (no compile mode)"
        );
    }

    #[test]
    fn test_endif_without_if_is_error() {
        // ENDIF without a preceding IF leaves the compile stack empty when CS_POP is
        // called inside the ENDIF body, which must produce a StackUnderflow error.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF FOO\n  ENDIF\nEND");
        assert!(
            result.is_err(),
            "ENDIF without IF should return an error (empty compile stack)"
        );
    }

    #[test]
    fn test_if_without_endif_is_error() {
        // IF without a matching ENDIF leaves the compile stack non-empty when END is
        // reached, which must produce a CompileStackNotEmpty error.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF FOO(X)\n  IF X > 0\nEND");
        assert!(
            result.is_err(),
            "IF without ENDIF should return an error (non-empty compile stack at END)"
        );
    }

    #[test]
    fn test_endif_outside_def_is_error() {
        // ENDIF at top level (interpret mode) must return an error because CS_POP
        // checks is_compiling before PATCH_ADDR is reached.
        let mut interp = Interpreter::new();
        let result = interp.exec_line("ENDIF");
        assert!(
            result.is_err(),
            "ENDIF at top level should return an error (no compile mode)"
        );
    }

    #[test]
    fn test_if_endif_nested() {
        // Nested IF/ENDIF must work correctly because compile_stack is LIFO.
        // Inner ENDIF patches only the inner IF placeholder; outer ENDIF patches only
        // the outer IF placeholder.
        let mut interp = Interpreter::new();
        let src = "\
DEF NESTED(X)
  IF X > 0
    IF X > 10
      PUTSTR \"big\"
    ENDIF
    PUTSTR \"pos\"
  ENDIF
END
NESTED 15
NESTED 5
NESTED -1";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "bigpospos", "expected 'bigpospos', got: {:?}", out);
    }

    #[test]
    fn test_resolve_source_pos_fallback() {
        // error_pc is outside main routine, return stack is empty → fallback (0, 0, "")
        let (line, col, src) = resolve_source_pos(
            999, // error_pc outside main
            &[], // empty return stack
            0,   // main_start
            10,  // main_len
            &[], // empty stmt_positions
        );
        assert_eq!(line, 0);
        assert_eq!(col, 0);
        assert!(src.is_empty());
    }

    #[test]
    fn test_compile_program_runtime_error_nested_word_line_number() {
        // DEF INNER → DEF OUTER(calls INNER) → OUTER call at line 7
        // Error in INNER, but err.line should point to OUTER call site (line 7)
        let mut interp = Interpreter::new();
        let src = "DEF INNER\n  PUTDEC 1 / 0\nEND\nDEF OUTER\n  INNER\nEND\nOUTER";
        let result = interp.compile_program(src);
        let err = result.expect_err("expected runtime error");
        assert_ne!(err.line, 0, "line must not be 0");
        assert_eq!(
            err.line, 7,
            "error must point to OUTER call site at line 7, got {}",
            err.line
        );
    }
}
