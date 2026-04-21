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

        // Skip optional line number.
        if matches!(tokens[idx].token, Token::LineNum(_)) {
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

        // Update the word header with the confirmed local_count.
        // The word was registered as the last entry at hdr_len_at_def.
        let word_hdr_idx = state.hdr_len_at_def;
        if word_hdr_idx < self.vm.headers.len() {
            self.vm.headers[word_hdr_idx].local_count = state.local_count;
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
        let stmt_xt = self.lookup_required(stmt_name, err_line, err_col, source_line)?;

        // Reject system-internal words from user code.
        let stmt_flags = self.vm.headers[stmt_xt.index()].flags;
        if stmt_flags & FLAG_SYSTEM != 0 {
            return Err(make_err(TbxError::UndefinedSymbol {
                name: stmt_name.to_string(),
            }));
        }

        // Compile the argument expression to a cell sequence.
        // Local variables in the current compile scope shadow globals (local_table checked first).
        let arg_cells = {
            let local_table_opt: Option<&HashMap<String, usize>> =
                self.compile_state.as_ref().map(|s| &s.local_table);
            let mut compiler = ExprCompiler::with_local_table_opt(&mut self.vm, local_table_opt);
            compiler.compile_expr(arg_tokens).map_err(&make_err)?
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
        for cell in arg_cells {
            self.vm.dict_write(cell).map_err(&make_err)?;
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
}
