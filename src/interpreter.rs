//! Outer interpreter: tokenizes source text and executes statements via the inner interpreter.

use std::collections::VecDeque;

use crate::cell::{Cell, Xt};
use crate::dict::FLAG_SYSTEM;
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::init_vm;
use crate::lexer::{Lexer, SpannedToken, Token};
use crate::vm::VM;

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
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    /// Create a new `Interpreter` backed by a fully initialized VM.
    pub fn new() -> Self {
        Self { vm: init_vm() }
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

        // Split token list into segments at each Semicolon.
        // Semicolons cannot appear inside expressions, so a flat split is correct.
        let semi_positions: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, st)| matches!(st.token, Token::Semicolon).then_some(i))
            .collect();

        let mut boundaries: Vec<(usize, usize)> = Vec::with_capacity(semi_positions.len() + 1);
        let mut start = 0;
        for &pos in &semi_positions {
            boundaries.push((start, pos));
            start = pos + 1;
        }
        boundaries.push((start, tokens.len()));

        let mut first_segment = true;
        for (seg_start_orig, seg_end) in boundaries {
            let mut seg_start = seg_start_orig;

            // Only the first segment may begin with a line number.
            if first_segment {
                first_segment = false;
                if seg_start < seg_end {
                    if let Token::LineNum(n) = tokens[seg_start].token {
                        if self.vm.compile_state.is_some() {
                            let ln_line = tokens[seg_start].pos.line;
                            let ln_col = tokens[seg_start].pos.col;
                            self.register_label(n, line, ln_line, ln_col)
                                .inspect_err(|_e| {
                                    self.vm.rollback_def();
                                })?;
                        }
                        seg_start += 1;
                    }
                }
            }

            // Skip empty segments (e.g., trailing semicolon or bare line number).
            if seg_start >= seg_end {
                continue;
            }

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

        // Handle REM: skip the rest of the segment (lexer already consumed trailing input).
        if stmt_name.eq_ignore_ascii_case("REM") {
            return Ok(());
        }

        // IMMEDIATE word dispatch: execute immediately regardless of compile/interpret mode.
        // If the looked-up word has FLAG_IMMEDIATE set, feed the remaining tokens into
        // vm.token_stream and call the primitive directly (not via vm.run()).
        {
            if let Some(xt) = self.vm.lookup(&stmt_name) {
                let flags = self.vm.headers[xt.index()].flags;
                if flags & crate::dict::FLAG_IMMEDIATE != 0 {
                    let make_err = |e: TbxError| {
                        InterpreterError::new(stmt_pos_line, stmt_pos_col, source_line, e)
                    };

                    // Get the primitive function pointer before any other borrows.
                    let prim_fn = match self.vm.headers[xt.index()].kind.clone() {
                        crate::dict::EntryKind::Primitive(f) => f,
                        _ => {
                            // TODO: user-defined IMMEDIATE words are not yet supported.
                            // Currently only EntryKind::Primitive can be flagged as IMMEDIATE.
                            return Err(make_err(TbxError::InvalidExpression {
                                reason: "IMMEDIATE word must be a primitive",
                            }));
                        }
                    };

                    // Feed remaining tokens into vm.token_stream.
                    let remaining: VecDeque<SpannedToken> = tokens[idx..].iter().cloned().collect();
                    self.vm.token_stream = Some(remaining);

                    // Save VM state for rollback on error.
                    let saved_data_stack_len = self.vm.data_stack.len();
                    let saved_return_stack_len = self.vm.return_stack.len();
                    let saved_bp = self.vm.bp;

                    // Call the primitive directly (not through vm.run() to avoid
                    // the temporary-buffer issues when the primitive writes to the dictionary).
                    let run_result = prim_fn(&mut self.vm);

                    // Clear token stream.
                    self.vm.token_stream = None;

                    // On error, rollback compile state and stacks.
                    if run_result.is_err() {
                        self.vm.rollback_def();
                        self.vm.data_stack.truncate(saved_data_stack_len);
                        self.vm.return_stack.truncate(saved_return_stack_len);
                        self.vm.bp = saved_bp;
                    }

                    return run_result.map_err(make_err);
                }
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
        // Temporarily take local_table out of compile_state so we can pass a reference to
        // ExprCompiler while also holding &mut VM.  Always restore it afterward.
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
            .vm
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
        // Unit tests for the shared lexer helper functions.
        let toks = tokenize_args("42");
        assert_eq!(crate::lexer::parse_label_number(&toks), Some(42));

        let toks = tokenize_args("I > 10, 99");
        let comma_pos = crate::lexer::last_top_level_comma(&toks);
        assert!(
            comma_pos.unwrap().is_some(),
            "should find a top-level comma"
        );

        let toks_no_comma = tokenize_args("42");
        assert_eq!(
            crate::lexer::last_top_level_comma(&toks_no_comma).unwrap(),
            None
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
    fn test_last_top_level_comma_unmatched_paren_errors() {
        // An unmatched ')' must produce an InvalidExpression error.
        let toks = tokenize_args("42 ), 99");
        let result = crate::lexer::last_top_level_comma(&toks);
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

    #[test]
    fn test_recursive_self_call_in_return_expr() {
        // Regression test for issue #222: self-recursive call inside RETURN expression
        // (compile_return path) must have its local_count back-patched.
        let src = r#"
DEF FACT(N)
  VAR R
  BIT N <= 1, 10
    LET &R, N - 1
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
    LET &R, N - 1
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
}
