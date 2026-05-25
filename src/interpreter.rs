//! Outer interpreter: tokenizes source text and executes statements via the inner interpreter.

use std::collections::{HashSet, VecDeque};
use std::io::BufRead;
use std::path::PathBuf;

use crate::cell::{Cell, ReturnFrame, Xt};
use crate::dict::FLAG_SYSTEM;
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::init_vm;
use crate::lexer::{Position, SpannedToken, Token};
use crate::statement_reader::{LogicalStatement, StatementReader};
use crate::vm::{StackTraceFrame, VM};

#[cfg(test)]
use crate::lexer::Lexer;

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
    pub source_excerpt: String,
    pub kind: TbxError,
    pub call_stack: Vec<StackTraceFrame>,
}

impl InterpreterError {
    /// Construct a new `InterpreterError` with the given location and error kind.
    fn new(line: usize, col: usize, source_excerpt: &str, kind: TbxError) -> Self {
        InterpreterError {
            line,
            col,
            source_excerpt: source_excerpt.to_string(),
            kind,
            call_stack: Vec::new(),
        }
    }

    /// Construct a new `InterpreterError` with a captured runtime call stack.
    fn with_call_stack(
        line: usize,
        col: usize,
        source_excerpt: &str,
        kind: TbxError,
        call_stack: Vec<StackTraceFrame>,
    ) -> Self {
        InterpreterError {
            line,
            col,
            source_excerpt: source_excerpt.to_string(),
            kind,
            call_stack,
        }
    }
}

impl std::fmt::Debug for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let excerpt = self.source_excerpt.replace('\n', "\n  ");
        write!(
            f,
            "line {}:{}: {:?}\n  {}",
            self.line, self.col, self.kind, excerpt
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
    /// Optional base directory used to resolve relative USE paths.
    ///
    /// When set, relative paths in USE statements are resolved against this
    /// directory rather than the current working directory. This makes the
    /// interpreter independent of the process CWD, which is important for
    /// tests and embedded use cases where the CWD may differ from the
    /// directory containing the TBX source files.
    ///
    /// Absolute paths are not affected. When `None` (the default), relative
    /// paths are resolved against the CWD as before.
    ///
    /// This acts as the fallback when no file is currently being loaded (i.e.
    /// when a USE appears at the top-level entry point rather than inside a
    /// file loaded by a previous USE).
    base_dir: Option<PathBuf>,
    /// Directory of the file that is currently being executed via USE.
    ///
    /// When a USE statement loads a file, this is set to that file's parent
    /// directory before `exec_source` is called recursively, so that nested
    /// USE statements within the file resolve paths relative to the file's own
    /// location rather than relative to `base_dir`.
    ///
    /// Specifically, for the resolution of a relative path `p` inside a file:
    ///   1. Use `current_dir` (the directory of the including file) if set.
    ///   2. Fall back to `base_dir` if set.
    ///   3. Fall back to the process CWD.
    current_dir: Option<PathBuf>,
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
            base_dir: None,
            current_dir: None,
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
        source_excerpt: &str,
    ) -> Result<Xt, InterpreterError> {
        self.vm.lookup(name).ok_or_else(|| {
            InterpreterError::new(
                line,
                col,
                source_excerpt,
                TbxError::UndefinedSymbol {
                    name: name.to_string(),
                },
            )
        })
    }

    /// Executes a single source line.
    ///
    /// `absolute_line` is the 1-based line number of this line in the full source being
    /// processed.  Pass `1` when executing a standalone line (e.g. from a REPL where
    /// each `exec_line` call represents a fresh, unnumbered statement).
    pub fn exec_line(&mut self, line: &str, absolute_line: usize) -> Result<(), InterpreterError> {
        let mut reader = StatementReader::new(line);
        let mut first_statement = true;
        loop {
            let stmt = match reader.next_statement() {
                Ok(Some(stmt)) => stmt,
                Ok(None) => break,
                Err(e) => {
                    if self.vm.compile_state.is_some() {
                        self.vm.rollback_def();
                    }
                    return Err(InterpreterError::new(
                        absolute_line,
                        e.col,
                        &e.source_excerpt,
                        e.kind,
                    ));
                }
            };

            let stmt = LogicalStatement {
                start_line: absolute_line,
                end_line: absolute_line,
                ..stmt
            };
            let stmt = if first_statement {
                first_statement = false;
                stmt
            } else {
                restore_nonleading_exec_line_label(stmt, absolute_line)
            };
            self.exec_logical_statement(stmt)?;
        }
        Ok(())
    }

    /// Executes a single statement segment (a slice of tokens with no Semicolons).
    ///
    /// `tokens` must be non-empty. Any leading line-number label has already
    /// been stripped by the caller.
    ///
    /// `absolute_line` is the 1-based line number of this segment in the full source being
    /// processed.
    fn exec_segment(
        &mut self,
        tokens: &[SpannedToken],
        source_excerpt: &str,
        absolute_line: usize,
    ) -> Result<(), InterpreterError> {
        let mut idx = 0;

        // Extract statement name.
        let stmt_tok = &tokens[idx];
        let stmt_name = match &stmt_tok.token {
            Token::Ident(name) => name.clone(),
            _ => return Ok(()), // not an identifier — skip
        };
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
                    absolute_line,
                    stmt_pos_col,
                    source_excerpt,
                );
            }
        }

        // In compile mode: write this statement to the dictionary instead of executing it.
        if self.vm.compile_state.is_some() {
            let result = self.write_stmt_to_dict(
                &stmt_name,
                &tokens[idx..],
                absolute_line,
                stmt_pos_col,
                source_excerpt,
            );
            if result.is_err() {
                self.vm.rollback_def();
            }
            return result;
        }

        // Helper closure for wrapping TbxError into InterpreterError.
        let make_err =
            |e: TbxError| InterpreterError::new(absolute_line, stmt_pos_col, source_excerpt, e);

        // Save the current dictionary pointer to use as the buffer start.
        let buf_start = self.vm.dp;

        // Write statement and arguments to the dictionary (LIT_MARKER … DROP_TO_MARKER).
        // On failure, reset dp so subsequent exec_line calls start from a clean state.
        if let Err(e) = self.write_stmt_to_dict(
            &stmt_name,
            &tokens[idx..],
            absolute_line,
            stmt_pos_col,
            source_excerpt,
        ) {
            self.vm.dp = buf_start;
            self.vm.dictionary.truncate(buf_start);
            return Err(e);
        }

        // Append EXIT to terminate the temporary code buffer.
        // On failure, reset dp before returning.
        let exit_xt =
            match self.lookup_required("EXIT", absolute_line, stmt_pos_col, source_excerpt) {
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
        let call_stack = if run_result.is_err() {
            Some(self.vm.stack_trace_frames())
        } else {
            None
        };

        if run_result.is_err() {
            self.vm.data_stack.truncate(saved_data_stack_len);
            self.vm.return_stack.truncate(saved_return_stack_len);
            self.vm.bp = saved_bp;
        }

        match run_result {
            Ok(()) => Ok(()),
            Err(e) => Err(InterpreterError::with_call_stack(
                absolute_line,
                stmt_pos_col,
                source_excerpt,
                e,
                call_stack.expect("call_stack must be Some when run_result is Err"),
            )),
        }
    }

    /// Register the statement's line-number label (if any) inside a DEF body
    /// and return the statement when there are still tokens to execute.
    ///
    /// `StatementReader` already strips a leading line-number label from the
    /// token list and stores it in `stmt.label`. This method only needs to
    /// register that label as a branch target when compiling a word.
    ///
    /// Returns `Ok(Some(stmt))` when the statement has tokens to execute,
    /// `Ok(None)` when the statement was a bare line-number label (nothing
    /// left to execute), or `Err` when label registration fails (compile
    /// state is rolled back before returning).
    fn prepare_logical_statement(
        &mut self,
        stmt: LogicalStatement,
    ) -> Result<Option<LogicalStatement>, InterpreterError> {
        if let Some(n) = stmt.label {
            if self.vm.compile_state.is_some() {
                let ln_line = stmt.start_line;
                let ln_col = stmt.start_col;
                self.register_label(n, &stmt.source_excerpt, ln_line, ln_col)
                    .inspect_err(|_e| {
                        self.vm.rollback_def();
                    })?;
            }
        }

        if stmt.tokens.is_empty() {
            return Ok(None);
        }

        Ok(Some(stmt))
    }

    /// Executes one logical statement produced by `StatementReader`.
    fn exec_logical_statement(&mut self, stmt: LogicalStatement) -> Result<(), InterpreterError> {
        let stmt = match self.prepare_logical_statement(stmt)? {
            Some(s) => s,
            None => return Ok(()),
        };
        self.exec_segment(&stmt.tokens, &stmt.source_excerpt, stmt.start_line)
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
        source_excerpt: &str,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(err_line, err_col, source_excerpt, e);

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

        // Reject empty-parens function-call syntax at statement level: NAME().
        // arg_tokens == [LParen, RParen] means the user wrote `NAME()` as a statement,
        // which is not the formal call form. The bare `NAME` form is correct.
        // Non-empty parens like `NAME(arg)` are indistinguishable at the token level from
        // the grouped-expression form `NAME (arg)` and are therefore left to pass through.
        if matches!(
            arg_tokens,
            [a, b] if a.token == Token::LParen && b.token == Token::RParen
        ) {
            return Err(make_err(TbxError::InvalidStatementCallSyntax {
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

        self.vm.headers[stmt_xt.index()]
            .check_variadic_arity(arity)
            .map_err(&make_err)?;

        // Check whether the statement is a compiled word (needs CALL with arity/locals)
        // or a primitive/other (called directly by placing Xt in the code stream).
        let stmt_is_word = matches!(
            self.vm.headers[stmt_xt.index()].kind,
            crate::dict::EntryKind::Word(_)
        );

        // Look up required system words for building the code buffer.
        // These must always be present after init_vm(); return a proper error if missing.
        let lit_marker_xt =
            self.lookup_required("LIT_MARKER", err_line, err_col, source_excerpt)?;
        let call_xt = self.lookup_required("CALL", err_line, err_col, source_excerpt)?;
        let drop_to_marker_xt =
            self.lookup_required("DROP_TO_MARKER", err_line, err_col, source_excerpt)?;

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
            // For a variadic primitive used as a statement, emit LIT + Int(arity)
            // before the Xt so the primitive can pop the arity from the stack.
            let is_variadic_prim = matches!(
                self.vm.headers[stmt_xt.index()].kind,
                crate::dict::EntryKind::Primitive(_)
            ) && self.vm.headers[stmt_xt.index()].is_variadic;
            if is_variadic_prim {
                let lit_xt = self.lookup_required("LIT", err_line, err_col, source_excerpt)?;
                self.vm.dict_write(Cell::Xt(lit_xt)).map_err(&make_err)?;
                self.vm
                    .dict_write(Cell::Int(arity as i64))
                    .map_err(&make_err)?;
            }
            self.vm.dict_write(Cell::Xt(stmt_xt)).map_err(&make_err)?;
        }
        self.vm
            .dict_write(Cell::Xt(drop_to_marker_xt))
            .map_err(&make_err)?;

        Ok(())
    }

    /// Execute a multi-line source string.
    ///
    /// Reads logical statements from the source and executes each one.
    /// Stops on the first error (including `TbxError::Halted`, which the inner
    /// interpreter returns for the `HALT` statement).
    pub fn exec_source(&mut self, src: &str) -> Result<(), InterpreterError> {
        let mut reader = StatementReader::new(src);
        loop {
            let stmt = match reader.next_statement() {
                Ok(Some(stmt)) => stmt,
                Ok(None) => break,
                Err(e) => {
                    if self.vm.compile_state.is_some() {
                        self.vm.rollback_def();
                    }
                    return Err(InterpreterError::new(
                        e.line,
                        e.col,
                        &e.source_excerpt,
                        e.kind,
                    ));
                }
            };
            match self.exec_logical_statement(stmt) {
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

    /// Read one line from the VM's input reader.
    ///
    /// Returns `Ok(Some(line))` with the line stripped of trailing newline
    /// characters, `Ok(None)` on EOF, or an `Err` on I/O failure.
    ///
    /// This method is used by `main::run_stdin()` so that the outer read loop
    /// draws from the same `BufReader` as `ACCEPT`, avoiding the deadlock that
    /// would occur if two separate `StdinLock` acquisitions competed on the
    /// same thread.
    pub fn read_input_line(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        match self.vm.input_reader.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(
                line.trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_string(),
            )),
            Err(e) => Err(e),
        }
    }

    /// Override the maximum USE nesting depth (test-only).
    ///
    /// Allows unit tests to trigger `TbxError::UseNestingDepthExceeded`
    /// without creating hundreds of temporary files.
    #[cfg(test)]
    fn set_max_use_depth(&mut self, max: usize) {
        self.max_use_depth = max;
    }

    /// Expose the inner VM for direct inspection in unit tests.
    #[cfg(test)]
    pub fn vm(&self) -> &VM {
        &self.vm
    }

    /// Set the base directory used to resolve relative USE paths.
    ///
    /// When set, relative paths in USE statements are resolved against `dir`
    /// instead of the process current working directory. This allows the
    /// interpreter to operate correctly regardless of the CWD.
    ///
    /// Absolute paths in USE statements are never affected by this setting.
    ///
    /// # Nested USE
    ///
    /// For nested `USE` statements inside a file loaded via `USE`, relative
    /// paths are resolved against the directory of the including file rather
    /// than this `base_dir`. This allows files to reference their own siblings
    /// without knowing where they were loaded from.
    ///
    /// `base_dir` serves as the fallback when a USE appears at the top level
    /// (i.e. not inside a file that was itself loaded via USE).
    ///
    /// # Errors
    ///
    /// Returns `Err(TbxError::InvalidArgument)` if `dir` is a relative path.
    /// Use `std::fs::canonicalize(&dir)` or `std::env::current_dir()` to obtain
    /// an absolute path before calling this method.
    pub fn set_base_dir(&mut self, dir: PathBuf) -> Result<(), TbxError> {
        if dir.is_absolute() {
            self.base_dir = Some(dir);
            Ok(())
        } else {
            Err(TbxError::InvalidArgument {
                message: format!(
                    "set_base_dir requires an absolute path; got: {}",
                    dir.display()
                ),
            })
        }
    }

    /// Execute an IMMEDIATE word, regardless of compile/interpret mode.
    ///
    /// Sets up `vm.token_stream` with the remaining tokens, dispatches the word
    /// (Primitive or zero-arity Word, including those with VAR locals), then
    /// clears the stream.
    ///
    /// **Limitation**: `RETURN expr` (value-returning return) is not supported
    /// inside IMMEDIATE words because `vm.run()` uses a `TopLevel` sentinel
    /// instead of a full CALL frame.  Using `RETURN expr` will produce a
    /// `TbxError::InvalidReturn` (which is rolled back cleanly).
    /// Void `RETURN` (EXIT) works correctly.
    ///
    /// On error, rolls back compile state and stack state before returning.
    fn exec_immediate_word(
        &mut self,
        xt: Xt,
        tokens_after_stmt: &[SpannedToken],
        stmt_pos_line: usize,
        stmt_pos_col: usize,
        source_excerpt: &str,
    ) -> Result<(), InterpreterError> {
        let make_err =
            |e: TbxError| InterpreterError::new(stmt_pos_line, stmt_pos_col, source_excerpt, e);

        // Clone fields needed for dispatch before the mutable borrow below.
        let kind = self.vm.headers[xt.index()].kind.clone();
        let arity = self.vm.headers[xt.index()].arity;
        let local_count = self.vm.headers[xt.index()].local_count;
        let immediate_word_name = self.vm.headers[xt.index()].name.clone();

        // Feed remaining tokens into vm.token_stream so the IMMEDIATE word can
        // consume them via vm.next_token().
        let remaining: VecDeque<SpannedToken> = tokens_after_stmt.iter().cloned().collect();
        self.vm.token_stream = Some(remaining);

        // Save VM state for rollback on error.
        let saved_data_stack_len = self.vm.data_stack.len();
        let saved_return_stack_len = self.vm.return_stack.len();
        let saved_bp = self.vm.bp;

        // Frame to inject into the call stack if a runtime error occurs during
        // dispatch.  Only set after we have committed to executing the word —
        // pre-execution rejections (e.g. arity > 0) leave this None so the
        // resulting error reports an empty call stack.
        let mut pending_synthetic_frame: Option<StackTraceFrame> = None;
        let run_result = match kind {
            // Native primitive: call the function pointer directly (avoids
            // temporary-buffer issues when the primitive writes to the dictionary).
            crate::dict::EntryKind::Primitive(f) => {
                pending_synthetic_frame = Some(StackTraceFrame {
                    word_name: immediate_word_name.clone(),
                    actual_arity: 0,
                });
                f(&mut self.vm)
            }
            // User-defined word: run via vm.run(), passing the body start address.
            // Guard: words with formal parameters (arity > 0) still require a
            // CALL frame and are rejected.  Words with only VAR locals
            // (local_count > 0, arity == 0) are supported: we set up bp and
            // push zero-initialised local slots manually, then tear them down
            // after the word returns.
            crate::dict::EntryKind::Word(body_addr) => {
                if arity > 0 {
                    Err(TbxError::InvalidExpression {
                        reason: "IMMEDIATE user word with parameters cannot be called without a CALL frame",
                    })
                } else {
                    // Set up local variable slots when the word declares VARs.
                    // push() errors are propagated as run_result so that the
                    // existing rollback block below (token_stream clear,
                    // rollback_def, stack/bp restore) handles all cleanup
                    // uniformly without an early return.
                    let push_err = if local_count > 0 {
                        self.vm.bp = self.vm.data_stack.len();
                        let mut err = None;
                        for _ in 0..local_count {
                            if let Err(e) = self.vm.push(crate::cell::Cell::Int(0)) {
                                err = Some(e);
                                break;
                            }
                        }
                        err
                    } else {
                        None
                    };

                    if let Some(e) = push_err {
                        Err(e)
                    } else {
                        pending_synthetic_frame = Some(StackTraceFrame {
                            word_name: immediate_word_name,
                            actual_arity: 0,
                        });
                        let result = self.vm.run(body_addr);
                        // On success, tear down all local slots.  truncate()
                        // removes both the zero-initialised local slots and any
                        // surplus values the word may have left on the stack —
                        // IMMEDIATE words do not return values via the data stack.
                        if result.is_ok() && local_count > 0 {
                            self.vm.data_stack.truncate(saved_data_stack_len);
                            self.vm.bp = saved_bp;
                        }
                        result
                    }
                }
            }
            _ => Err(TbxError::InvalidExpression {
                reason: "IMMEDIATE word kind is not executable",
            }),
        };

        // Clear token stream.
        self.vm.token_stream = None;

        // On error, rollback compile state and stacks.
        let call_stack = if run_result.is_err() {
            let mut frames = self.vm.stack_trace_frames();
            if let Some(frame) = pending_synthetic_frame {
                let insert_pos = frames
                    .iter()
                    .position(|existing| existing.word_name == "<top-level>")
                    .unwrap_or(frames.len());
                frames.insert(insert_pos, frame);
            }
            Some(frames)
        } else {
            None
        };

        if run_result.is_err() {
            self.vm.rollback_def();
            self.vm.data_stack.truncate(saved_data_stack_len);
            self.vm.return_stack.truncate(saved_return_stack_len);
            self.vm.bp = saved_bp;
            // Discard any pending USE path set before the error.
            self.vm.pending_use_path = None;
        }

        match run_result {
            Ok(()) => {}
            Err(e) => {
                return Err(InterpreterError::with_call_stack(
                    stmt_pos_line,
                    stmt_pos_col,
                    source_excerpt,
                    e,
                    call_stack.expect("call_stack must be Some when run_result is Err"),
                ));
            }
        }

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
            //
            // Resolve the path for the USE statement.
            //
            // Priority for relative paths:
            //   1. `current_dir` — directory of the file currently being loaded
            //      via USE (set when we are inside a nested USE).  This allows
            //      files to reference siblings without knowing their absolute
            //      location.
            //   2. `base_dir`    — the application-level root set by
            //      `set_base_dir`.  Used for top-level USE statements.
            //   3. Process CWD   — fallback when neither is set.
            //
            // Absolute paths bypass all of the above.
            let resolved_path = if std::path::Path::new(&path).is_relative() {
                if let Some(cur) = &self.current_dir {
                    cur.join(&path)
                } else if let Some(base) = &self.base_dir {
                    base.join(&path)
                } else {
                    PathBuf::from(&path)
                }
            } else {
                PathBuf::from(&path)
            };
            let canonical = std::fs::canonicalize(&resolved_path).map_err(|e| {
                make_err(TbxError::FileNotFound {
                    path: resolved_path.display().to_string(),
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
            // Set current_dir to the directory of the file being loaded so
            // that nested USE statements inside it resolve paths relative to
            // that file's own directory rather than the top-level base_dir.
            let file_dir = canonical.parent().map(|p| p.to_path_buf());
            let prev_current_dir = self.current_dir.take();
            self.current_dir = file_dir;
            self.loading_files.insert(canonical.clone());
            self.use_depth += 1;
            let result = self.exec_source(&source);
            self.use_depth -= 1;
            self.loading_files.remove(&canonical);
            self.current_dir = prev_current_dir;
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
        source_excerpt: &str,
        line: usize,
        col: usize,
    ) -> Result<(), InterpreterError> {
        let make_err = |e: TbxError| InterpreterError::new(line, col, source_excerpt, e);
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
            self.vm.dictionary[patch_pos] = Cell::DictAddr(dp);
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

        let mut reader = StatementReader::new(source);
        loop {
            let stmt = match reader.next_statement() {
                Ok(Some(s)) => s,
                Ok(None) => break,
                Err(e) => {
                    if self.vm.compile_state.is_some() {
                        self.vm.rollback_def();
                    }
                    return Err(InterpreterError::new(
                        e.line,
                        e.col,
                        &e.source_excerpt,
                        e.kind,
                    ));
                }
            };

            let stmt_start_line = stmt.start_line;
            let stmt = match self.prepare_logical_statement(stmt)? {
                Some(s) => s,
                None => continue,
            };

            let was_compiling = self.vm.compile_state.is_some();
            self.compile_program_segment(
                &stmt.tokens,
                &stmt.source_excerpt,
                &mut main_cells,
                &mut stmt_positions,
                stmt_start_line,
            )?;
            // If DEF just started on this segment, record the logical statement start line.
            if !was_compiling {
                if let Some(state) = &mut self.vm.compile_state {
                    state.start_line = stmt_start_line;
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
                let call_stack = self.vm.stack_trace_frames();
                self.vm.data_stack.truncate(saved_data_stack_len);
                self.vm.return_stack.truncate(saved_return_stack_len);
                self.vm.bp = saved_bp;
                Err(InterpreterError::with_call_stack(
                    line, col, &source, e, call_stack,
                ))
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
    /// `(start_offset_in_main_cells, line, col, source_excerpt)`.
    ///
    /// `absolute_line` is the 1-based line number of the logical statement's first line in the
    /// full source file, as supplied by `StatementReader`.  Token positions within a segment are
    /// relative to that segment and must not be used alone for source-level position recording.
    fn compile_program_segment(
        &mut self,
        tokens: &[SpannedToken],
        source_excerpt: &str,
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
                    absolute_line,
                    stmt_pos_col,
                    source_excerpt,
                );
            }
        }

        // Inside a DEF body: write statement to dictionary directly (same as exec_segment).
        if self.vm.compile_state.is_some() {
            let result = self.write_stmt_to_dict(
                &stmt_name,
                &tokens[idx..],
                absolute_line,
                stmt_pos_col,
                source_excerpt,
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
            absolute_line,
            stmt_pos_col,
            source_excerpt,
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
                source_excerpt: source_excerpt.to_string(),
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
    /// Source excerpt for the logical statement containing the statement.
    source_excerpt: String,
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
            .map(|sp| (sp.line, sp.col, sp.source_excerpt.clone()))
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
fn restore_nonleading_exec_line_label(
    mut stmt: LogicalStatement,
    absolute_line: usize,
) -> LogicalStatement {
    if let Some(label) = stmt.label.take() {
        stmt.tokens.insert(
            0,
            SpannedToken {
                token: Token::IntLit(label),
                pos: Position {
                    line: absolute_line,
                    col: stmt.start_col,
                },
                source_offset: 0,
                source_len: label.to_string().len(),
            },
        );
    }
    stmt
}

fn count_top_level_arity(tokens: &[SpannedToken]) -> Result<usize, TbxError> {
    if tokens.is_empty() {
        return Ok(0);
    }
    let mut depth: usize = 0;
    let mut commas: usize = 0;
    for st in tokens {
        match &st.token {
            Token::LParen | Token::LBracket => depth += 1,
            Token::RParen | Token::RBracket => {
                depth = depth.checked_sub(1).ok_or(TbxError::InvalidExpression {
                    reason: "unmatched ')' or ']' in argument list",
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
        interp.exec_line("PUTDEC 42", 1).unwrap();
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
        let result = interp.exec_line("HALT", 1);
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
    fn test_exec_source_multiline_str_concat() {
        let mut interp = Interpreter::new();
        let src = "\
VAR S
SET &S, STR_CONCAT(
  \"foo\",
  \"bar\"
)
PUTSTR S";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "foobar");
    }

    #[test]
    fn test_exec_source_multiline_nested_call() {
        let mut interp = Interpreter::new();
        let src = "\
PUTDEC ADD(
  1,
  MUL(
    2,
    3
  )
)";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_exec_source_multiline_expr_inside_def() {
        let mut interp = Interpreter::new();
        let src = "\
DEF SHOW
  PUTDEC ADD(
    1,
    2
  )
END
SHOW";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "3");
    }

    #[test]
    fn test_exec_source_line_number_label_inside_def() {
        let mut interp = Interpreter::new();
        let src = "\
DEF SHOW(X)
  BIT X = 0, 10
  PUTDEC 1
  10 PUTDEC 2
END
SHOW 0";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "2");
    }

    #[test]
    fn test_exec_source_semicolon_and_rem_compatibility() {
        let mut interp = Interpreter::new();
        interp
            .exec_source("PUTDEC 1; REM x; PUTDEC 2\nPUTDEC 3")
            .unwrap();
        assert_eq!(interp.take_output(), "13");
    }

    #[test]
    fn test_exec_line_unclosed_paren_still_errors() {
        let mut interp = Interpreter::new();
        let result = interp.exec_line("PUTDEC ADD(", 1);
        assert!(result.is_err(), "exec_line must remain single-line");
    }

    #[test]
    fn test_exec_source_unclosed_paren_reports_open_paren_position() {
        let mut interp = Interpreter::new();
        let err = interp
            .exec_source("PUTDEC ADD(\n  1\n")
            .expect_err("unclosed paren should be an error");
        assert_eq!(err.line, 1);
        assert_eq!(err.col, 11);
        assert!(matches!(
            err.kind,
            TbxError::InvalidExpression {
                reason: "unmatched '(' in statement"
            }
        ));
    }

    #[test]
    fn test_exec_source_reader_error_inside_def_rolls_back() {
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF BAD\n  PUTDEC ADD(\n");
        assert!(result.is_err(), "unclosed paren inside DEF should fail");

        interp
            .exec_source("PUTDEC 7")
            .expect("interpreter should be reusable after reader error rollback");
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_exec_undefined_symbol() {
        let mut interp = Interpreter::new();
        let result = interp.exec_line("NOSUCHWORD 1", 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err.kind, TbxError::UndefinedSymbol { .. }));
    }

    #[test]
    fn test_exec_system_word_direct_call_rejected() {
        let mut interp = Interpreter::new();
        // Attempting to call a FLAG_SYSTEM word (LIT_MARKER) as a statement should fail.
        let result = interp.exec_line("LIT_MARKER", 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_rem_is_skipped() {
        let mut interp = Interpreter::new();
        // REM line should not produce any error or output.
        interp.exec_line("REM this is a comment", 1).unwrap();
        let out = interp.take_output();
        assert!(out.is_empty());
    }

    #[test]
    fn test_exec_empty_line() {
        let mut interp = Interpreter::new();
        interp.exec_line("", 1).unwrap();
        interp.exec_line("   ", 1).unwrap();
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
        let _ = interp.exec_line("NOSUCHWORD", 1);
        // A subsequent valid call must still work.
        interp.exec_line("PUTDEC 1", 1).unwrap();
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
        let result = interp.exec_line("DEF", 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_end_outside_def_returns_error() {
        // END outside DEF is handled by end_prim (FLAG_IMMEDIATE), which checks
        // is_compiling and returns InvalidExpression when called in interpret mode.
        let mut interp = Interpreter::new();
        assert!(interp.exec_line("END", 1).is_err());
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
    fn test_count_top_level_arity_nested_brackets() {
        // Commas inside brackets must not be counted as top-level separators.
        // Regression test for issue #776: @A[1, 2] inside a call argument must
        // not be misinterpreted as two separate arguments.
        let tokens = tokenize_args("@A[1 , 2]");
        assert_eq!(count_top_level_arity(&tokens), Ok(1));
    }

    #[test]
    fn test_count_top_level_arity_mixed_brackets_and_parens() {
        // f(@A[1, 2], @B[3, 4]) must count as 2 top-level arguments.
        let tokens = tokenize_args("f(@A[1 , 2]) , g(@B[3 , 4])");
        assert_eq!(count_top_level_arity(&tokens), Ok(2));
    }

    #[test]
    fn test_exec_primitive_call_in_expression() {
        // Regression test for issue #208: calling a Primitive from within an expression
        // must succeed via the direct-Xt path (no CALL instruction generated by expr.rs).
        // ADD(1, 2) compiles to: Xt(LIT), Int(1), Xt(LIT), Int(2), Xt(ADD) — ADD is dispatched
        // directly as a Primitive.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC ADD(1, 2)", 1).unwrap();
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
        interp.exec_line("PUTSTR \"a\"; PUTDEC 42", 1).unwrap();
        assert_eq!(interp.take_output(), "a42");
    }

    #[test]
    fn test_semicolon_three_statements() {
        // Three semicolon-separated statements must all execute in order.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1; PUTDEC 2; PUTDEC 3", 1).unwrap();
        assert_eq!(interp.take_output(), "123");
    }

    #[test]
    fn test_semicolon_trailing() {
        // A trailing semicolon (empty last segment) must be silently ignored.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1;", 1).unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_rem_stops_execution() {
        // REM causes the lexer to consume the rest of the input, so statements
        // after a REM segment are never seen.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1; REM x; PUTDEC 2", 1).unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_with_paren_args() {
        // Parenthesised arguments must not be confused with segment boundaries.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC ADD(1,2); PUTDEC 3", 1).unwrap();
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
        let result = interp.exec_line("PUTDEC 1; NOSUCHWORD", 1);
        assert!(result.is_err(), "second segment should return an error");
        // First segment's output is already flushed.
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_leading() {
        // A leading semicolon produces an empty first segment, which is skipped.
        let mut interp = Interpreter::new();
        interp.exec_line("; PUTDEC 1", 1).unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_semicolon_consecutive() {
        // Consecutive semicolons produce empty segments that are silently skipped.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1;; PUTDEC 2", 1).unwrap();
        assert_eq!(interp.take_output(), "12");
    }

    #[test]
    fn test_exec_line_only_first_segment_can_use_line_number_label() {
        // exec_line historically treated a leading integer as a label only on
        // the first segment of the physical line. Later semicolon-separated
        // segments starting with an integer are not statements and must be
        // skipped rather than reinterpreted as label-prefixed statements.
        let mut interp = Interpreter::new();
        interp.exec_line("PUTDEC 1; 10 PUTDEC 2", 1).unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_exec_line_nonleading_int_segment_inside_def_is_not_label() {
        // The same rule must hold while compiling a DEF body: only the first
        // physical-line segment may act as a line-number label.
        let mut interp = Interpreter::new();
        interp.exec_line("DEF SHOW", 1).unwrap();
        interp.exec_line("PUTDEC 1; 20 PUTDEC 2", 2).unwrap();
        interp.exec_line("END", 3).unwrap();
        interp.exec_line("SHOW", 4).unwrap();
        assert_eq!(interp.take_output(), "1");
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
    fn test_user_defined_immediate_word_with_locals_succeeds() {
        // A user word with VAR locals should now work as an IMMEDIATE word.
        // The interpreter must set up bp and local slots before calling run().
        let mut interp = Interpreter::new();
        let src = "\
DEF ILOCAL
VAR X
SET &X, 99
PUTDEC X
END
IMMEDIATE ILOCAL
ILOCAL";
        interp
            .exec_source(src)
            .expect("IMMEDIATE word with VAR should succeed");
        assert_eq!(interp.take_output(), "99");
    }

    #[test]
    fn test_immediate_word_var_read_write() {
        // VAR local declared in an IMMEDIATE word can be written and read back.
        let mut interp = Interpreter::new();
        let src = "\
DEF IWORD
VAR A
SET &A, 42
PUTDEC A
END
IMMEDIATE IWORD
IWORD";
        interp.exec_source(src).expect("should succeed");
        assert_eq!(interp.take_output(), "42");
    }

    #[test]
    fn test_immediate_word_var_isolated_from_outer_stack() {
        // After an IMMEDIATE word with VAR locals runs, the data stack must be
        // back to its original length (local slots cleaned up).
        let mut interp = Interpreter::new();
        let src = "\
DEF ICLEAN
VAR A
VAR B
SET &A, 1
SET &B, 2
END
IMMEDIATE ICLEAN
ICLEAN";
        interp.exec_source(src).expect("should succeed");
        assert_eq!(
            interp.vm.data_stack.len(),
            0,
            "data stack must be clean after IMMEDIATE word with locals"
        );
    }

    #[test]
    fn test_immediate_word_var_during_compile() {
        // An IMMEDIATE word with VAR locals invoked inside a DEF…END block must
        // execute at compile time and produce output, while the outer word's
        // body must execute silently at call time.
        let mut interp = Interpreter::new();
        let src = "\
DEF ICOMP
VAR X
SET &X, 55
PUTDEC X
END
IMMEDIATE ICOMP
DEF OUTER
ICOMP
END";
        interp
            .exec_source(src)
            .expect("compile phase should succeed");
        let compile_out = interp.take_output();
        assert_eq!(
            compile_out, "55",
            "ICOMP must execute during compilation of OUTER (got: {compile_out:?})"
        );
        interp
            .exec_source("OUTER")
            .expect("runtime phase should succeed");
        let runtime_out = interp.take_output();
        assert_eq!(
            runtime_out, "",
            "OUTER must not re-execute ICOMP at runtime (got: {runtime_out:?})"
        );
    }

    #[test]
    fn test_immediate_word_multiple_vars() {
        // Multiple VAR locals declared in an IMMEDIATE word must be independent.
        let mut interp = Interpreter::new();
        let src = "\
DEF IMULTI
VAR P
VAR Q
SET &P, 10
SET &Q, 20
PUTDEC P
PUTDEC Q
END
IMMEDIATE IMULTI
IMULTI";
        interp.exec_source(src).expect("should succeed");
        assert_eq!(interp.take_output(), "1020");
    }

    #[test]
    fn test_immediate_word_var_push_overflow_rolls_back() {
        // When vm.push() fails during local-slot allocation (DataStackOverflow),
        // the rollback block must run: token_stream cleared, stack and bp restored.
        use crate::cell::Cell;
        use crate::constants::MAX_DATA_STACK_DEPTH;

        let mut interp = Interpreter::new();

        // Define an IMMEDIATE word with one VAR local.
        interp
            .exec_source(
                "\
DEF IFULL
VAR X
END
IMMEDIATE IFULL",
            )
            .expect("definition phase must succeed");

        // Fill the data stack to its limit so the next push overflows.
        interp
            .vm
            .data_stack
            .resize(MAX_DATA_STACK_DEPTH, Cell::Int(0));
        let before_len = interp.vm.data_stack.len();
        let before_bp = interp.vm.bp;

        // Calling the IMMEDIATE word must return an error (DataStackOverflow).
        let result = interp.exec_source("IFULL");
        assert!(
            result.is_err(),
            "expected DataStackOverflow when stack is full, got: {:?}",
            result
        );

        // VM state must be fully restored after the error.
        assert_eq!(
            interp.vm.data_stack.len(),
            before_len,
            "data_stack must be restored to its pre-call length"
        );
        assert_eq!(
            interp.vm.bp, before_bp,
            "bp must be restored after push overflow"
        );
        assert!(
            interp.vm.token_stream.is_none(),
            "token_stream must be cleared after push overflow"
        );
    }

    #[test]
    fn test_immediate_word_var_partial_push_overflow_rolls_back() {
        // Verify rollback when the Nth push (not the first) overflows.
        // Use a word with 2 VAR locals and fill the stack so that the
        // first push succeeds but the second overflows.
        use crate::cell::Cell;
        use crate::constants::MAX_DATA_STACK_DEPTH;

        let mut interp = Interpreter::new();

        // Define an IMMEDIATE word with two VAR locals.
        interp
            .exec_source(
                "\
DEF IPARTIAL
VAR A
VAR B
END
IMMEDIATE IPARTIAL",
            )
            .expect("definition phase must succeed");

        // Fill the stack to MAX - 1 so the first push succeeds, second overflows.
        interp
            .vm
            .data_stack
            .resize(MAX_DATA_STACK_DEPTH - 1, Cell::Int(0));
        let before_len = interp.vm.data_stack.len();
        let before_bp = interp.vm.bp;

        let result = interp.exec_source("IPARTIAL");
        assert!(
            result.is_err(),
            "expected overflow on second local slot push, got: {:?}",
            result
        );

        // After the error the stack must be restored to its original length.
        assert_eq!(
            interp.vm.data_stack.len(),
            before_len,
            "data_stack must be restored after partial push overflow"
        );
        assert_eq!(
            interp.vm.bp, before_bp,
            "bp must be restored after partial push overflow"
        );
        assert!(
            interp.vm.token_stream.is_none(),
            "token_stream must be cleared after partial push overflow"
        );
    }

    #[test]
    fn test_immediate_word_var_early_void_return() {
        // A void RETURN (EXIT) inside an IMMEDIATE word with VAR locals must
        // exit early and leave the stack clean.
        let mut interp = Interpreter::new();
        let src = "\
DEF IEARLY
VAR X
SET &X, 99
RETURN
PUTDEC X
END
IMMEDIATE IEARLY
IEARLY";
        interp
            .exec_source(src)
            .expect("early void RETURN in IMMEDIATE word must succeed");
        // PUTDEC after RETURN must not execute.
        assert_eq!(interp.take_output(), "", "PUTDEC after RETURN must not run");
        assert_eq!(
            interp.vm.data_stack.len(),
            0,
            "data stack must be clean after early RETURN"
        );
    }

    #[test]
    fn test_immediate_word_var_return_expr_errors() {
        // RETURN expr (value-returning) inside an IMMEDIATE word must return
        // TbxError::InvalidReturn because vm.run() uses a TopLevel sentinel
        // instead of a proper CALL frame.  VM state must be rolled back cleanly.
        let mut interp = Interpreter::new();
        let setup = "\
DEF IRETVAL
VAR X
SET &X, 5
RETURN X
END
IMMEDIATE IRETVAL";
        interp
            .exec_source(setup)
            .expect("definition phase must succeed");

        let before_len = interp.vm.data_stack.len();
        let before_bp = interp.vm.bp;

        let result = interp.exec_source("IRETVAL");
        assert!(result.is_err(), "RETURN expr in IMMEDIATE word must error");
        assert_eq!(
            interp.vm.data_stack.len(),
            before_len,
            "data_stack must be restored after InvalidReturn"
        );
        assert_eq!(
            interp.vm.bp, before_bp,
            "bp must be restored after InvalidReturn"
        );
        assert!(
            interp.vm.token_stream.is_none(),
            "token_stream must be cleared after InvalidReturn"
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
        let err = result.unwrap_err();
        assert!(
            err.call_stack.is_empty(),
            "pre-execution IMMEDIATE dispatch error should not synthesize a runtime frame"
        );
    }

    #[test]
    fn test_immediate_word_runtime_error_captures_word_frame() {
        let mut interp = Interpreter::new();
        let src = "DEF BAD\n  PUTDEC 1 / 0\nEND\nIMMEDIATE BAD\nBAD";
        let result = interp.exec_source(src);
        let err = result.expect_err("expected runtime error from IMMEDIATE word");
        assert_eq!(err.call_stack.len(), 2);
        assert_eq!(err.call_stack[0].word_name, "BAD");
        assert_eq!(err.call_stack[0].actual_arity, 0);
        assert_eq!(err.call_stack[1].word_name, "<top-level>");
    }

    #[test]
    fn test_immediate_primitive_runtime_error_captures_primitive_frame() {
        // END is an IMMEDIATE primitive; calling it outside of DEF returns a
        // runtime error from end_prim.  The primitive's name must appear in the
        // call stack so the error message points at the offending word.
        // (IMMEDIATE primitives are dispatched without entering vm.run(), so
        // the captured stack contains only the synthetic primitive frame.)
        let mut interp = Interpreter::new();
        let result = interp.exec_source("END");
        let err = result.expect_err("expected runtime error from IMMEDIATE primitive");
        assert_eq!(err.call_stack.len(), 1);
        assert_eq!(err.call_stack[0].word_name, "END");
        assert_eq!(err.call_stack[0].actual_arity, 0);
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
    fn test_compile_program_top_level_array_store_to_global_var() {
        let mut interp = Interpreter::new();
        let src = "DIM @A[1]\nPUTDEC ARRAY_LEN(@A)";
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_compile_program_top_level_str_store_to_global_var() {
        let mut interp = Interpreter::new();
        let src = r#"VAR S
SET &S, STR_CONCAT("foo", "bar")
PUTSTR S"#;
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "foobar");
    }

    #[test]
    fn test_compile_program_word_can_copy_top_level_str_global() {
        let mut interp = Interpreter::new();
        let src = r#"VAR S
VAR T
DEF COPY_STR()
  SET &T, S
END
SET &S, STR_CONCAT("foo", "bar")
COPY_STR
PUTSTR T"#;
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "foobar");
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
    fn test_compile_program_goto_outside_def_is_error() {
        // GOTO appearing outside a DEF body in ground-level code must produce
        // a compile-time error ("GOTO outside DEF"), not a runtime failure.
        // The interpreter must also remain reusable after this compile error.
        let mut interp = Interpreter::new();
        let result = interp.compile_program("GOTO 10");
        assert!(
            result.is_err(),
            "GOTO at ground level should be a compile error"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("GOTO outside DEF"),
            "error message should mention 'GOTO outside DEF', got: {err}"
        );
        // Verify that the interpreter is still usable after the compile error.
        interp
            .compile_program("PUTDEC 1")
            .expect("interpreter should be reusable after GOTO compile error");
        assert_eq!(interp.take_output(), "1");
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
            err.source_excerpt.contains("1 / 0"),
            "source_excerpt should contain the failing expression, got: {:?}",
            err.source_excerpt
        );
        assert_eq!(err.call_stack.len(), 1);
        assert_eq!(err.call_stack[0].word_name, "<top-level>");
        assert_eq!(err.call_stack[0].actual_arity, 0);
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
            err.source_excerpt.contains("BAD_WORD"),
            "source_excerpt should contain the call site identifier, got: {:?}",
            err.source_excerpt
        );
        assert_eq!(err.call_stack.len(), 2);
        assert_eq!(err.call_stack[0].word_name, "BAD_WORD");
        assert_eq!(err.call_stack[0].actual_arity, 0);
        assert_eq!(err.call_stack[1].word_name, "<top-level>");
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
        interp.exec_line("DEF HELLO", 1).unwrap();
        interp.exec_line("PUTDEC 99", 1).unwrap();
        interp.exec_line("END", 1).unwrap();
        interp.compile_program("HELLO").unwrap();
        assert_eq!(interp.take_output(), "99");
    }

    #[test]
    fn test_exec_line_goto_outside_def_is_error() {
        // GOTO appearing at ground level (outside a DEF block) must produce an error
        // in interpreter mode (exec_line) just as it does in full-program mode.
        // This verifies the spec documented in blueprint-language.md §"GOTO/BIF scope constraints".
        // The interpreter must remain usable (REPL can continue) after the error.
        let mut interp = Interpreter::new();
        let result = interp.exec_line("GOTO 10", 1);
        assert!(
            result.is_err(),
            "GOTO at ground level via exec_line should be an error"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("GOTO outside DEF"),
            "error message should mention 'GOTO outside DEF', got: {err}"
        );
        // Verify that the interpreter is still reusable after the error (REPL continuity).
        interp
            .exec_line("PUTDEC 1", 1)
            .expect("exec_line should be reusable after GOTO-outside-DEF error");
        assert_eq!(interp.take_output(), "1");
    }

    #[test]
    fn test_compile_program_then_exec_line_coexistence() {
        // A word defined inside compile_program must be callable via exec_line
        // on the same Interpreter instance afterwards.
        let mut interp = Interpreter::new();
        interp
            .compile_program("DEF ADD1(X)\nRETURN X + 1\nEND")
            .unwrap();
        interp.exec_line("PUTDEC ADD1(41)", 1).unwrap();
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

    // --- USE set_base_dir ---

    #[test]
    fn test_set_base_dir_resolves_relative_use_path() {
        // A relative USE path should be resolved from base_dir, not from CWD.
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_path = dir.path().join("lib.tbx");
        std::fs::write(&lib_path, "DEF HELLO\nPUTSTR \"hello\"\nEND\n").unwrap();

        let mut interp = Interpreter::new();
        // Set base_dir to the temp directory so that the relative path "lib.tbx"
        // resolves to the file created above.
        interp.set_base_dir(dir.path().to_path_buf()).unwrap();
        // Use a relative path: only the file name, relative to base_dir.
        interp.exec_source("USE \"lib.tbx\"\nHELLO").unwrap();
        assert!(
            interp.take_output().contains("hello"),
            "relative USE should succeed when base_dir is set"
        );
    }

    #[test]
    fn test_set_base_dir_file_not_found_error_contains_resolved_path() {
        // When a USE path does not exist, the FileNotFound error should report
        // the resolved (base_dir-joined) path, not the raw relative string.
        let dir = tempfile::tempdir().expect("tempdir");

        let mut interp = Interpreter::new();
        interp.set_base_dir(dir.path().to_path_buf()).unwrap();
        let result = interp.exec_source("USE \"no_such_file.tbx\"");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err.kind, TbxError::FileNotFound { .. }),
            "expected TbxError::FileNotFound"
        );
        // The error path must contain the base_dir component so that the user
        // can see which absolute path was attempted.
        if let TbxError::FileNotFound { path, .. } = &err.kind {
            assert!(
                path.contains(dir.path().to_str().unwrap()),
                "FileNotFound path should contain base_dir; got: {path}"
            );
        }
    }

    #[test]
    fn test_set_base_dir_inherited_by_nested_use() {
        // Nested USE (b.tbx USEd from a.tbx) should also resolve relative paths
        // against the same base_dir.
        let dir = tempfile::tempdir().expect("tempdir");
        let path_c = dir.path().join("c.tbx");
        let path_b = dir.path().join("b.tbx");
        let path_a = dir.path().join("a.tbx");
        std::fs::write(&path_c, "DEF NESTED\nPUTSTR \"nested\"\nEND\n").unwrap();
        // b.tbx references c.tbx with a relative path
        std::fs::write(&path_b, "USE \"c.tbx\"\n").unwrap();
        // a.tbx references b.tbx with a relative path
        std::fs::write(&path_a, "USE \"b.tbx\"\n").unwrap();

        let mut interp = Interpreter::new();
        interp.set_base_dir(dir.path().to_path_buf()).unwrap();
        // All USE paths are relative; base_dir must be applied throughout the chain.
        interp.exec_source("USE \"a.tbx\"\nNESTED").unwrap();
        assert!(
            interp.take_output().contains("nested"),
            "base_dir should be applied to nested USE chains"
        );
    }

    #[test]
    fn test_set_base_dir_nested_use_from_subdirectory_resolves_relative_to_including_file() {
        // Relative paths in nested USE files are resolved against the directory
        // of the including file, NOT against base_dir.
        //
        // Given: base_dir = /tmp/dir, modules/a.tbx USEs "utils.tbx"
        //   => looks for /tmp/dir/modules/utils.tbx (sibling of a.tbx)
        //
        // This allows subdirectory files to reference their own siblings without
        // knowing the base_dir root.
        let dir = tempfile::tempdir().expect("tempdir");
        let modules_dir = dir.path().join("modules");
        std::fs::create_dir(&modules_dir).unwrap();
        let path_a = modules_dir.join("a.tbx");
        // utils.tbx is placed next to a.tbx inside modules/
        let path_utils = modules_dir.join("utils.tbx");
        std::fs::write(&path_utils, "DEF UTIL_WORD\nPUTSTR \"util\"\nEND\n").unwrap();
        // a.tbx references utils.tbx with a path relative to its own directory
        std::fs::write(&path_a, "USE \"utils.tbx\"\n").unwrap();

        let mut interp = Interpreter::new();
        interp.set_base_dir(dir.path().to_path_buf()).unwrap();
        // a.tbx USEs "utils.tbx" which resolves to modules/utils.tbx (sibling) — success.
        interp
            .exec_source("USE \"modules/a.tbx\"\nUTIL_WORD")
            .unwrap();
        assert!(
            interp.take_output().contains("util"),
            "USE in subdirectory file should resolve paths relative to the including file's directory"
        );
    }

    #[test]
    fn test_nested_use_sibling_file_in_subdir_no_base_dir() {
        // Without base_dir: a file loaded via USE resolves its own USE paths
        // relative to its own directory (not the process CWD).
        //
        //   utils/
        //     math.tbx  -- USEs "helper.tbx"
        //     helper.tbx
        //
        // Loading math.tbx via an absolute path and then calling HELPER_WORD
        // should succeed.
        let dir = tempfile::tempdir().expect("tempdir");
        let utils_dir = dir.path().join("utils");
        std::fs::create_dir(&utils_dir).unwrap();
        let path_helper = utils_dir.join("helper.tbx");
        let path_math = utils_dir.join("math.tbx");
        std::fs::write(&path_helper, "DEF HELPER_WORD\nPUTSTR \"helper\"\nEND\n").unwrap();
        std::fs::write(&path_math, "USE \"helper.tbx\"\n").unwrap();

        let mut interp = Interpreter::new();
        // Load math.tbx via its absolute path; no base_dir set.
        let use_stmt = format!("USE \"{}\"", path_math.display());
        // Loading math.tbx triggers USE "helper.tbx" which must resolve relative
        // to math.tbx's own directory (utils/), not the process CWD.
        interp.exec_source(&use_stmt).unwrap();
        interp.exec_source("HELPER_WORD").unwrap();
        assert!(
            interp.take_output().contains("helper"),
            "HELPER_WORD defined via nested sibling USE should be callable"
        );
    }

    #[test]
    fn test_set_base_dir_rejects_relative_path() {
        // Passing a relative path must return Err(TbxError::InvalidArgument).
        let mut interp = Interpreter::new();
        let result = interp.set_base_dir(PathBuf::from("relative/path"));
        assert!(result.is_err());
        let TbxError::InvalidArgument { message } = result.unwrap_err() else {
            panic!("expected TbxError::InvalidArgument");
        };
        assert!(
            message.contains("relative/path"),
            "error message should include the invalid path; got: {message}"
        );
    }

    #[test]
    fn test_set_base_dir_rejects_empty_path() {
        // An empty string is also a relative path and must be rejected.
        let mut interp = Interpreter::new();
        let result = interp.set_base_dir(PathBuf::from(""));
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), TbxError::InvalidArgument { .. }),
            "empty path should return InvalidArgument"
        );
    }

    #[test]
    fn test_set_base_dir_does_not_mutate_on_error() {
        // When set_base_dir returns Err, base_dir must remain None (direct check).
        let mut interp = Interpreter::new();
        let _ = interp.set_base_dir(PathBuf::from("relative/path"));
        assert!(
            interp.base_dir.is_none(),
            "base_dir should remain None after a failed set_base_dir"
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
        let result = interp.exec_line("IF 1 > 0", 1);
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
        let result = interp.exec_line("ENDIF", 1);
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
        assert_eq!(err.call_stack.len(), 3);
        assert_eq!(err.call_stack[0].word_name, "INNER");
        assert_eq!(err.call_stack[1].word_name, "OUTER");
        assert_eq!(err.call_stack[2].word_name, "<top-level>");
    }

    // --- WHILE / ENDWH ---

    #[test]
    fn test_while_endwh_basic_countdown() {
        // Simple countdown using a VAR local: prints 3, 2, 1, then exits.
        let mut interp = Interpreter::new();
        let src = "\
DEF COUNTDOWN(N)
  VAR I
  SET &I, N
  WHILE I > 0
    PUTDEC I
    SET &I, I - 1
  ENDWH
END
COUNTDOWN 3";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "321", "expected '321', got: {:?}", out);
    }

    #[test]
    fn test_while_endwh_condition_false_from_start_skips_body() {
        // If the condition is false on the first evaluation, the body must not execute.
        let mut interp = Interpreter::new();
        let src = "\
DEF SKIP(N)
  VAR I
  SET &I, N
  WHILE I > 0
    PUTDEC I
    SET &I, I - 1
  ENDWH
END
SKIP -1";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "", "body must not run when initial condition is false");
    }

    #[test]
    fn test_while_endwh_accumulate() {
        // Accumulate sum 1+2+3+4+5 = 15.
        let mut interp = Interpreter::new();
        let src = "\
DEF SUMTO(N)
  VAR S
  VAR I
  SET &S, 0
  SET &I, 1
  WHILE I <= N
    SET &S, S + I
    SET &I, I + 1
  ENDWH
  PUTDEC S
END
SUMTO 5";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "15", "expected sum 15, got: {:?}", out);
    }

    #[test]
    fn test_while_endwh_nested() {
        // Nested WHILE loops: outer counts i=1..2, inner counts j=1..2.
        // Prints "11 12 21 22 " (with trailing space).
        let mut interp = Interpreter::new();
        let src = "\
DEF NESTED()
  VAR I
  VAR J
  SET &I, 1
  WHILE I <= 2
    SET &J, 1
    WHILE J <= 2
      PUTDEC I
      PUTDEC J
      PUTSTR \" \"
      SET &J, J + 1
    ENDWH
    SET &I, I + 1
  ENDWH
END
NESTED";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "11 12 21 22 ", "nested WHILE mismatch: {:?}", out);
    }

    #[test]
    fn test_while_outside_def_is_error() {
        // WHILE used outside a DEF body must yield an error.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("WHILE 1 > 0");
        assert!(result.is_err(), "WHILE outside DEF must be an error");
    }

    #[test]
    fn test_endwh_without_while_is_error() {
        // ENDWH without a matching WHILE must yield a StackUnderflow (CS_SWAP on empty stack).
        let mut interp = Interpreter::new();
        let src = "DEF BAD()\n  ENDWH\nEND";
        let result = interp.exec_source(src);
        assert!(result.is_err(), "ENDWH without WHILE must be an error");
    }

    #[test]
    fn test_while_without_endwh_is_error() {
        // WHILE without matching ENDWH: compile stack is non-empty at END,
        // which must return CompileStackNotEmpty.
        let mut interp = Interpreter::new();
        let src = "DEF UNCLOSED()\n  WHILE 1 > 0\n    PUTDEC 1\n  END";
        let result = interp.exec_source(src);
        assert!(
            result.is_err(),
            "WHILE without ENDWH must be an error at END"
        );
    }

    // --- CS_SWAP / CS_DROP / CS_DUP / CS_OVER / CS_ROT (integration via IMMEDIATE words) ---

    #[test]
    fn test_cs_swap_reorders_compile_stack() {
        // CS_SWAP is exercised indirectly through ENDWH; verify via a working WHILE loop.
        let mut interp = Interpreter::new();
        let src = "\
DEF COUNT(N)
  VAR I
  SET &I, N
  WHILE I > 0
    PUTDEC I
    SET &I, I - 1
  ENDWH
END
COUNT 2";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "21");
    }

    #[test]
    fn test_cs_drop_via_immediate_word() {
        // CS_DROP is exercised through a custom IMMEDIATE word that uses it.
        // SKIPONE pushes HERE on the compile stack and immediately drops it (no patch),
        // then emits a constant value — verifying that CS_DROP removes the entry without error.
        let mut interp = Interpreter::new();
        let src = "\
DEF SKIPONE
  CS_PUSH HERE
  CS_DROP
END
IMMEDIATE SKIPONE

DEF TRYDROP()
  SKIPONE
  PUTDEC 42
END
TRYDROP";
        // SKIPONE compiles HERE (saved address), drops it, and falls through.
        // PUTDEC 42 must run and produce output \"42\".
        interp.exec_source(src).unwrap();
        assert_eq!(
            interp.take_output(),
            "42",
            "CS_DROP must discard compile-stack entry; PUTDEC 42 must run"
        );
    }

    #[test]
    fn test_cs_dup_and_over_via_immediate_word() {
        // CS_DUP and CS_OVER are exercised through a custom IMMEDIATE word.
        // DUPTEST pushes a value on CS, dups it, then drops the copy.
        let mut interp = Interpreter::new();
        let src = "\
DEF DUPTEST
  CS_PUSH HERE
  CS_DUP
  CS_DROP
  CS_DROP
END
IMMEDIATE DUPTEST

DEF TRYDUPTHEN()
  DUPTEST
  PUTDEC 7
END
TRYDUPTHEN";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_cs_rot_via_immediate_word() {
        // CS_ROT is exercised through a custom IMMEDIATE word.
        // ROTATECS pushes three values, rotates them, then drops all three.
        let mut interp = Interpreter::new();
        let src = "\
DEF ROTATECS
  CS_PUSH HERE
  CS_PUSH HERE
  CS_PUSH HERE
  CS_ROT
  CS_DROP
  CS_DROP
  CS_DROP
END
IMMEDIATE ROTATECS

DEF TRYROT()
  ROTATECS
  PUTDEC 9
END
TRYROT";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "9");
    }

    #[test]
    fn test_elsif_without_if_is_error() {
        // ELSIF without a preceding IF leaves the compile stack empty when CS_POP is
        // called inside the ELSIF body, which must produce a StackUnderflow error.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF FOO(X)\n  ELSIF X > 0\nEND");
        assert!(
            result.is_err(),
            "ELSIF without IF should return an error (empty compile stack)"
        );
    }

    #[test]
    fn test_else_without_if_is_error() {
        // ELSE without a preceding IF leaves the compile stack empty when CS_POP is
        // called inside the ELSE body, which must produce a StackUnderflow error.
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF FOO(X)\n  ELSE\nEND");
        assert!(
            result.is_err(),
            "ELSE without IF should return an error (empty compile stack)"
        );
    }

    #[test]
    fn test_elsif_outside_def_is_error() {
        // ELSIF at top level (interpret mode) must return an error.
        // ELSIF starts with CS_CLOSE_TAG "IF", which checks is_compiling and returns
        // InvalidExpression immediately — no APPEND calls are made.
        let mut interp = Interpreter::new();
        let result = interp.exec_line("ELSIF 1 > 0", 1);
        assert!(
            result.is_err(),
            "ELSIF at top level should return an error (no compile mode)"
        );
    }

    #[test]
    fn test_else_outside_def_is_error() {
        // ELSE at top level (interpret mode) must return an error.
        // ELSE starts with CS_CLOSE_TAG "IF", which checks is_compiling and returns
        // InvalidExpression immediately — no APPEND calls are made.
        let mut interp = Interpreter::new();
        let result = interp.exec_line("ELSE", 1);
        assert!(
            result.is_err(),
            "ELSE at top level should return an error (no compile mode)"
        );
    }

    #[test]
    fn test_compile_error_line_number() {
        // compile_program must report the absolute line number of the error.
        let mut interp = Interpreter::new();
        let src = "VAR X\nUNKNOWN_WORD";
        let result = interp.compile_program(src);
        match result {
            Err(e) if e.line == 2 => {}
            other => panic!("expected error at line 2, got {other:?}"),
        }
    }

    #[test]
    fn test_compile_error_line_number_in_def() {
        // compile_program must report the absolute line number even inside DEF bodies.
        let mut interp = Interpreter::new();
        let src = "DEF TEST\n  UNKNOWN_WORD\nEND";
        let result = interp.compile_program(src);
        match result {
            Err(e) if e.line == 2 => {}
            other => panic!("expected error at line 2, got {other:?}"),
        }
    }

    #[test]
    fn test_exec_source_error_line_number() {
        // exec_source must report the absolute line number of a compile error, not always 1.
        let mut interp = Interpreter::new();
        // "UNKNOWN_WORD" is on line 2; the error should say line 2, not line 1.
        let src = "VAR X\nUNKNOWN_WORD";
        let result = interp.exec_source(src);
        match result {
            Err(e) if e.line == 2 => {}
            other => panic!("expected error at line 2, got {other:?}"),
        }
    }

    #[test]
    fn test_exec_source_error_line_number_in_def() {
        // exec_source must report the absolute line number even inside DEF bodies.
        let mut interp = Interpreter::new();
        // "UNKNOWN_WORD" is on line 2; the error should say line 2, not line 1.
        let src = "DEF TEST\n  UNKNOWN_WORD\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e) if e.line == 2 => {}
            other => panic!("expected error at line 2, got {other:?}"),
        }
    }

    // --- Control-structure mismatch detection (issue #358) ---

    #[test]
    fn test_if_while_endif_endwh_cross_nesting_error() {
        // IF ... WHILE ... ENDIF  must fail with MismatchedTag.
        let mut interp = Interpreter::new();
        let src =
            "DEF BAD(X)\n  IF X > 0\n    WHILE X > 0\n      SET &X, X - 1\n    ENDIF\n  ENDWH\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::MismatchedTag {
                        ref expected,
                        ref found,
                    } if expected == "IF" && found == "WHILE"
                ) => {}
            other => panic!("expected MismatchedTag(IF/WHILE), got {other:?}"),
        }
    }

    #[test]
    fn test_if_endwh_cross_nesting_error() {
        // IF ... ENDWH  must fail with MismatchedTag.
        let mut interp = Interpreter::new();
        let src = "DEF BAD(X)\n  IF X > 0\n    PUTDEC X\n  ENDWH\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::MismatchedTag {
                        ref expected,
                        ref found,
                    } if expected == "WHILE" && found == "IF"
                ) => {}
            other => panic!("expected MismatchedTag(WHILE/IF), got {other:?}"),
        }
    }

    #[test]
    fn test_endwh_without_while_unopened_error() {
        // ENDWH with no preceding WHILE must fail with NoOpenTag.
        let mut interp = Interpreter::new();
        let src = "DEF BAD()\n  ENDWH\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::NoOpenTag { ref expected } if expected == "WHILE"
                ) => {}
            other => panic!("expected NoOpenTag(WHILE), got {other:?}"),
        }
    }

    #[test]
    fn test_endif_without_if_unopened_error() {
        // ENDIF with no preceding IF must fail with NoOpenTag.
        let mut interp = Interpreter::new();
        let src = "DEF BAD()\n  ENDIF\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::NoOpenTag { ref expected } if expected == "IF"
                ) => {}
            other => panic!("expected NoOpenTag(IF), got {other:?}"),
        }
    }

    #[test]
    fn test_correct_if_while_endwh_endif_nesting() {
        // IF ... WHILE ... ENDWH ... ENDIF  must compile and run correctly.
        let mut interp = Interpreter::new();
        let src = "\
DEF COUNT_DOWN(X)
  IF X > 0
    WHILE X > 0
      SET &X, X - 1
    ENDWH
  ENDIF
END
COUNT_DOWN(3)";
        interp
            .exec_source(src)
            .expect("correct nesting must succeed");
    }

    #[test]
    fn test_correct_while_if_endif_endwh_nesting() {
        // WHILE ... IF ... ENDIF ... ENDWH  must compile and run correctly.
        let mut interp = Interpreter::new();
        let src = "\
DEF NOOP_LOOP(X)
  WHILE X > 0
    IF X > 1
      SET &X, X - 1
    ENDIF
    SET &X, X - 1
  ENDWH
END
NOOP_LOOP(4)";
        interp
            .exec_source(src)
            .expect("correct nesting must succeed");
    }

    #[test]
    fn test_if_else_endwh_cross_nesting_error() {
        // IF ... ELSE ... ENDWH must fail with MismatchedTag.
        // ELSE keeps Tag("IF") on compile_stack, so ENDWH sees "IF" instead of "WHILE".
        let mut interp = Interpreter::new();
        let src = "DEF BAD(X)\n  IF X > 0\n    PUTDEC X\n  ELSE\n    PUTDEC 0\n  ENDWH\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::MismatchedTag {
                        ref expected,
                        ref found,
                    } if expected == "WHILE" && found == "IF"
                ) => {}
            other => panic!("expected MismatchedTag(WHILE/IF), got {other:?}"),
        }
    }

    #[test]
    fn test_if_elsif_endwh_cross_nesting_error() {
        // IF ... ELSIF ... ENDWH must fail with MismatchedTag.
        // ELSIF keeps Tag("IF") on compile_stack, so ENDWH sees "IF" instead of "WHILE".
        let mut interp = Interpreter::new();
        let src = "DEF BAD(X)\n  IF X > 2\n    PUTDEC X\n  ELSIF X > 0\n    PUTDEC 1\n  ENDWH\nEND";
        let result = interp.exec_source(src);
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::MismatchedTag {
                        ref expected,
                        ref found,
                    } if expected == "WHILE" && found == "IF"
                ) => {}
            other => panic!("expected MismatchedTag(WHILE/IF), got {other:?}"),
        }
    }

    // --- IF ... ELSIF ... ELSE ... ENDIF (N=1 + ELSE path, issue #359) ---

    #[test]
    fn test_if_elsif_one_else_endif_sign3() {
        // IF ... ELSIF ... ELSE ... ENDIF with exactly one ELSIF exercises the
        // ENDIF loop once and then patches the final ELSE JUMP_ALWAYS placeholder.
        let mut interp = Interpreter::new();
        let src = "\
DEF SIGN3(X)
  IF X > 0
    PUTSTR \"+\"
  ELSIF X < 0
    PUTSTR \"-\"
  ELSE
    PUTSTR \"0\"
  ENDIF
END
SIGN3 5
SIGN3 -3
SIGN3 0";
        interp.exec_source(src).unwrap();
        let out = interp.take_output();
        assert_eq!(out, "+-0", "expected '+-0', got: {:?}", out);
    }

    // --- SELECT / CASE / CASE_ELSE / ENDSEL (lib/basic.tbx) ---

    #[test]
    fn test_select_case_matches_and_else_path() {
        let mut interp = Interpreter::new();
        let src = "\
DEF PICK(X)
  SELECT X
  CASE 1
    PUTSTR \"one\"
  CASE 2
    PUTSTR \"two\"
  CASE_ELSE
    PUTSTR \"other\"
  ENDSEL
END
PICK 1
PICK 2
PICK 9";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "onetwoother");
    }

    #[test]
    fn test_select_case_no_else_falls_through_cleanly() {
        let mut interp = Interpreter::new();
        let src = "\
DEF ONLY_ONE(X)
  SELECT X
  CASE 1
    RETURN 10
  ENDSEL
  RETURN 0
END
PUTDEC ONLY_ONE(1)
PUTDEC ONLY_ONE(3)";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "100");
    }

    #[test]
    fn test_select_case_no_else_match_can_continue_after_endsel() {
        let mut interp = Interpreter::new();
        let src = "\
DEF F(X)
  SELECT X
  CASE 1
    PUTSTR \"hit\"
  ENDSEL
  PUTDEC X
END
F(1)";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "hit1");
    }

    #[test]
    fn test_compile_program_select_case() {
        let mut interp = Interpreter::new();
        let src = r#"
DEF PICK(X)
  SELECT X
  CASE 1
    PUTSTR "one"
  CASE_ELSE
    PUTSTR "other"
  ENDSEL
END
PICK 1
PICK 9
"#;
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "oneother");
    }

    #[test]
    fn test_select_outside_def_is_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_line("SELECT 1", 1);
        assert!(result.is_err(), "SELECT outside DEF should be an error");
    }

    #[test]
    fn test_case_without_select_unopened_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF BAD(X)\n  CASE X\nEND");
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::NoOpenTag { ref expected } if expected == "SELECT"
                ) => {}
            other => panic!("expected NoOpenTag(SELECT), got {other:?}"),
        }
    }

    #[test]
    fn test_endsel_without_select_unopened_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF BAD()\n  ENDSEL\nEND");
        match result {
            Err(e)
                if matches!(
                    e.kind,
                    crate::error::TbxError::NoOpenTag { ref expected } if expected == "SELECT"
                ) => {}
            other => panic!("expected NoOpenTag(SELECT), got {other:?}"),
        }
    }

    #[test]
    fn test_select_without_endsel_is_error() {
        let mut interp = Interpreter::new();
        let result = interp.exec_source("DEF BAD(X)\n  SELECT X\n  CASE 1\n    PUTDEC 1\nEND");
        assert!(
            result.is_err(),
            "SELECT without ENDSEL should be an error at END"
        );
    }

    #[test]
    fn test_case_else_without_case_is_error() {
        let mut interp = Interpreter::new();
        let result =
            interp.exec_source("DEF BAD(X)\n  SELECT X\n  CASE_ELSE\n    PUTDEC 1\n  ENDSEL\nEND");
        assert!(result.is_err(), "CASE_ELSE without CASE should be an error");
    }

    #[test]
    fn test_case_after_case_else_is_error() {
        let mut interp = Interpreter::new();
        let src = "\
DEF BAD(X)
  SELECT X
  CASE 1
    PUTDEC 1
  CASE_ELSE
    PUTDEC 0
  CASE 2
    PUTDEC 2
  ENDSEL
END";
        let result = interp.exec_source(src);
        assert!(result.is_err(), "CASE after CASE_ELSE should be an error");
    }

    // --- LET compile word ---

    #[test]
    fn test_let_local_variable_basic() {
        // LET I = 10 inside a DEF body assigns a local variable.
        let mut interp = Interpreter::new();
        let src = "DEF TESTLET\n  VAR I\n  LET I = 10\n  PUTDEC I\nEND\nTESTLET";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "10");
    }

    #[test]
    fn test_let_local_arithmetic_expr() {
        // LET with an arithmetic RHS expression.
        let mut interp = Interpreter::new();
        let src = "DEF TESTLET(X)\n  VAR R\n  LET R = X * 2 + 1\n  PUTDEC R\nEND\nTESTLET 5";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "11");
    }

    #[test]
    fn test_let_global_variable() {
        // LET assigns a global variable declared outside a DEF.
        let mut interp = Interpreter::new();
        let src = "VAR G\nDEF SETG(V)\n  LET G = V\nEND\nSETG 42\nPUTDEC G";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "42");
    }

    #[test]
    fn test_let_missing_eq_is_error() {
        // LET without '=' should produce an InvalidExpression error.
        let mut interp = Interpreter::new();
        let src = "DEF BAD\n  VAR I\n  LET I 10\nEND";
        let result = interp.exec_source(src);
        assert!(result.is_err(), "expected error for LET without '='");
    }

    #[test]
    fn test_let_undefined_variable_is_error() {
        // LET with an undefined variable name should produce an UndefinedSymbol error.
        let mut interp = Interpreter::new();
        let src = "DEF BAD\n  LET NOSUCH = 10\nEND";
        let result = interp.exec_source(src);
        assert!(
            matches!(
                result,
                Err(ref e) if matches!(e.kind, crate::error::TbxError::UndefinedSymbol { .. })
            ),
            "expected UndefinedSymbol, got {result:?}"
        );
    }

    #[test]
    fn test_let_parameter_assignment() {
        // LET can assign to a function parameter (which is also a local StackAddr).
        let mut interp = Interpreter::new();
        let src = "DEF DOUBLE(X)\n  LET X = X * 2\n  PUTDEC X\nEND\nDOUBLE 7";
        interp.exec_source(src).unwrap();
        assert_eq!(interp.take_output(), "14");
    }

    #[test]
    fn test_let_outside_def_is_error() {
        // LET at top level (outside DEF) should fail because COMPILE_LVALUE
        // requires compile mode (is_compiling = true).
        let mut interp = Interpreter::new();
        let src = "VAR G\nLET G = 10";
        let result = interp.exec_source(src);
        assert!(result.is_err(), "expected error for LET outside DEF");
    }

    // --- compile_program: StatementReader-based multi-line expression support (issue #520) ---

    #[test]
    fn test_compile_program_multiline_str_concat() {
        // compile_program must handle multi-line STR_CONCAT(...) at ground level.
        let mut interp = Interpreter::new();
        let src = "\
VAR S
SET &S, STR_CONCAT(
  \"foo\",
  \"bar\"
)
PUTSTR S";
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "foobar");
    }

    #[test]
    fn test_compile_program_multiline_nested_call() {
        // compile_program must handle a nested multi-line call at ground level.
        let mut interp = Interpreter::new();
        let src = "\
PUTDEC ADD(
  1,
  MUL(
    2,
    3
  )
)";
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_compile_program_multiline_expr_inside_def() {
        // compile_program must handle a multi-line expression inside a DEF body.
        let mut interp = Interpreter::new();
        let src = "\
DEF SHOW
  PUTDEC ADD(
    1,
    2
  )
END
SHOW";
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "3");
    }

    #[test]
    fn test_compile_program_line_number_label_inside_def_multiline_body() {
        // Line-number labels inside a DEF body must still work when body uses
        // multi-line expressions (StatementReader path).
        let mut interp = Interpreter::new();
        let src = "\
DEF SHOW(X)
  BIT X = 0, 10
  PUTDEC 1
  10 PUTDEC 2
END
SHOW 0";
        interp.compile_program(src).unwrap();
        assert_eq!(interp.take_output(), "2");
    }

    #[test]
    fn test_compile_program_reader_error_inside_def_rolls_back() {
        // A StatementReader error (unclosed paren) inside a DEF body during
        // compile_program must roll back compile state so the interpreter is reusable.
        let mut interp = Interpreter::new();
        let result = interp.compile_program("DEF BAD\n  PUTDEC ADD(\n");
        assert!(result.is_err(), "unclosed paren inside DEF should fail");

        interp
            .compile_program("PUTDEC 7")
            .expect("interpreter should be reusable after reader error rollback");
        assert_eq!(interp.take_output(), "7");
    }

    #[test]
    fn test_compile_program_runtime_error_multiline_stmt_points_to_start_line() {
        // A runtime error in a multi-line ground-level statement must point to the
        // logical statement's start line, not to an interior continuation line.
        let mut interp = Interpreter::new();
        let src = "\
PUTDEC 1
PUTDEC ADD(
  1,
  1 / 0
)";
        let result = interp.compile_program(src);
        let err = result.expect_err("expected runtime error from division by zero");
        // The multi-line statement starts on line 2; the error must point there.
        assert_eq!(
            err.line, 2,
            "runtime error must point to line 2 (statement start), got {}",
            err.line
        );
        // Column must point to the start of the PUTDEC keyword on line 2.
        assert_eq!(
            err.col, 1,
            "column should point to the start of the PUTDEC keyword (col 1), got {}",
            err.col
        );
        // source_excerpt must preserve the full multi-line logical statement.
        assert!(
            err.source_excerpt.contains("PUTDEC ADD(\n  1,\n  1 / 0\n)"),
            "source_excerpt should contain the full statement, got: {:?}",
            err.source_excerpt
        );
    }

    #[test]
    fn test_exec_source_runtime_error_multiline_stmt_uses_full_excerpt() {
        // exec_source should preserve the full logical statement excerpt too.
        let mut interp = Interpreter::new();
        let src = "\
PUTDEC 1
PUTDEC ADD(
  1,
  1 / 0
)";
        let result = interp.exec_source(src);
        let err = result.expect_err("expected runtime error from division by zero");
        assert_eq!(
            err.line, 2,
            "runtime error must point to line 2, got {}",
            err.line
        );
        assert_eq!(
            err.col, 1,
            "column should point to the start of PUTDEC, got {}",
            err.col
        );
        assert!(
            err.source_excerpt.contains("PUTDEC ADD(\n  1,\n  1 / 0\n)"),
            "source_excerpt should contain the full statement, got: {:?}",
            err.source_excerpt
        );
    }

    #[test]
    fn test_exec_source_runtime_error_multiline_stmt_after_semicolon_uses_statement_excerpt() {
        let mut interp = Interpreter::new();
        let src = "\
PUTDEC 1; PUTDEC ADD(
  1,
  1 / 0
)";
        let result = interp.exec_source(src);
        let err = result.expect_err("expected runtime error from division by zero");
        assert_eq!(
            err.line, 1,
            "runtime error must point to line 1, got {}",
            err.line
        );
        assert_eq!(
            err.col, 11,
            "column should point to the second PUTDEC, got {}",
            err.col
        );
        assert_eq!(err.source_excerpt, "PUTDEC ADD(\n  1,\n  1 / 0\n)");
    }

    // ── issue #631: InvalidStatementCallSyntax ────────────────────────────

    const SETG_DEF: &str = "VAR G\nDEF SETG()\n  SET &G, 1\nEND\n";
    const GETG_DEF: &str = "VAR G\nSET &G, 42\nDEF GETG()\n  RETURN G\nEND\n";

    #[test]
    fn test_stmt_call_with_parens_exec_source_gives_proper_error() {
        // exec_source path: SETG() as a statement must produce InvalidStatementCallSyntax.
        let mut interp = Interpreter::new();
        let result = interp.exec_source(&format!("{SETG_DEF}SETG()\nPUTDEC G"));
        let err = result.expect_err("expected error");
        assert!(
            matches!(err.kind, TbxError::InvalidStatementCallSyntax { .. }),
            "expected InvalidStatementCallSyntax, got: {:?}",
            err.kind
        );
        let msg = err.to_string();
        assert!(
            !msg.contains("DROP_TO_MARKER"),
            "message must not expose DROP_TO_MARKER: {msg}"
        );
        assert!(
            !msg.contains("marker"),
            "message must not expose 'marker': {msg}"
        );
        assert!(
            msg.contains("SETG"),
            "message should mention the word name: {msg}"
        );
    }

    #[test]
    fn test_stmt_call_with_parens_exec_line_gives_proper_error() {
        // exec_line path: SETG() as a statement must produce InvalidStatementCallSyntax.
        let mut interp = Interpreter::new();
        interp.exec_source(SETG_DEF).unwrap();
        let result = interp.exec_line("SETG()", 5);
        let err = result.expect_err("expected error");
        assert!(
            matches!(err.kind, TbxError::InvalidStatementCallSyntax { .. }),
            "expected InvalidStatementCallSyntax, got: {:?}",
            err.kind
        );
        let msg = err.to_string();
        assert!(
            !msg.contains("DROP_TO_MARKER"),
            "message must not expose DROP_TO_MARKER: {msg}"
        );
        assert!(
            !msg.contains("marker"),
            "message must not expose 'marker': {msg}"
        );
    }

    #[test]
    fn test_stmt_call_with_nonempty_parens_is_currently_accepted() {
        // NAME(args...) at statement level is indistinguishable from the grouped-expression
        // form `NAME (args...)` at the token level, so it is currently accepted.
        // Only the zero-argument form NAME() is rejected (see blueprint-language.md §544).
        let mut interp = Interpreter::new();
        interp
            .exec_source("DEF FOO(X)\n  PUTDEC X\nEND\nFOO(1)")
            .expect("NAME(arg) as statement should currently succeed");
        assert_eq!(interp.take_output().trim(), "1");
    }

    #[test]
    fn test_stmt_call_without_parens_still_works() {
        // Formal statement call NAME (no parens) must continue to work.
        let mut interp = Interpreter::new();
        interp
            .exec_source(&format!("{SETG_DEF}SETG\nPUTDEC G"))
            .unwrap();
        assert_eq!(interp.take_output().trim(), "1");
    }

    #[test]
    fn test_expression_call_with_parens_still_works() {
        // NAME() inside an operand expression must continue to work.
        let mut interp = Interpreter::new();
        interp
            .exec_source(&format!("{GETG_DEF}PUTDEC GETG()"))
            .unwrap();
        assert_eq!(interp.take_output().trim(), "42");
    }
}
