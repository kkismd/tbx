//! Outer interpreter: tokenizes source text and executes statements via the inner interpreter.

use crate::cell::Cell;
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

impl std::fmt::Debug for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}:{}: {:?}", self.line, self.col, self.kind)
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

        // Look up the statement word.
        let stmt_xt = self.vm.lookup(&stmt_name).ok_or_else(|| InterpreterError {
            line: stmt_pos_line,
            col: stmt_pos_col,
            source_line: line.to_string(),
            kind: TbxError::UndefinedSymbol {
                name: stmt_name.clone(),
            },
        })?;

        // Reject system-internal words from user code.
        let stmt_flags = self.vm.headers[stmt_xt.index()].flags;
        if stmt_flags & FLAG_SYSTEM != 0 {
            return Err(InterpreterError {
                line: stmt_pos_line,
                col: stmt_pos_col,
                source_line: line.to_string(),
                kind: TbxError::UndefinedSymbol {
                    name: stmt_name.clone(),
                },
            });
        }

        // Remaining tokens are the argument expression.
        let arg_tokens = &tokens[idx..];

        // Compile the argument expression to a cell sequence.
        let arg_cells = {
            let mut compiler = ExprCompiler::new(&mut self.vm);
            compiler
                .compile_expr(arg_tokens)
                .map_err(|e| InterpreterError {
                    line: stmt_pos_line,
                    col: stmt_pos_col,
                    source_line: line.to_string(),
                    kind: e,
                })?
        };

        // Determine arity from top-level comma count.
        let arity = count_top_level_arity(arg_tokens);

        // Check whether the statement is a compiled word (needs CALL with arity/locals)
        // or a primitive/other (called directly by placing Xt in the code stream).
        let stmt_is_word = matches!(
            self.vm.headers[stmt_xt.index()].kind,
            crate::dict::EntryKind::Word(_)
        );

        // Look up required system words for building the code buffer.
        let lit_marker_xt = self.vm.lookup("LIT_MARKER").unwrap();
        let call_xt = self.vm.lookup("CALL").unwrap();
        let drop_to_marker_xt = self.vm.lookup("DROP_TO_MARKER").unwrap();
        let exit_xt = self.vm.lookup("EXIT").unwrap();

        // Save the current dictionary pointer to use as the buffer start.
        let buf_start = self.vm.dp;

        // Helper closure for wrapping TbxError into InterpreterError.
        let make_err = |e: TbxError| InterpreterError {
            line: stmt_pos_line,
            col: stmt_pos_col,
            source_line: line.to_string(),
            kind: e,
        };

        // Build temporary code buffer:
        //   Xt(LIT_MARKER)
        //   [arg_cells]
        //   For compiled words: Xt(CALL), Xt(stmt), Int(arity), Int(0)
        //   For primitives:     Xt(stmt)  (dispatched directly by the inner interpreter)
        //   Xt(DROP_TO_MARKER)
        //   Xt(EXIT)
        self.vm
            .dict_write(Cell::Xt(lit_marker_xt))
            .map_err(&make_err)?;
        for cell in arg_cells {
            self.vm.dict_write(cell).map_err(&make_err)?;
        }
        if stmt_is_word {
            self.vm.dict_write(Cell::Xt(call_xt)).map_err(&make_err)?;
            self.vm.dict_write(Cell::Xt(stmt_xt)).map_err(&make_err)?;
            self.vm
                .dict_write(Cell::Int(arity as i64))
                .map_err(&make_err)?;
            self.vm.dict_write(Cell::Int(0)).map_err(&make_err)?;
        } else {
            self.vm.dict_write(Cell::Xt(stmt_xt)).map_err(&make_err)?;
        }
        self.vm
            .dict_write(Cell::Xt(drop_to_marker_xt))
            .map_err(&make_err)?;
        self.vm.dict_write(Cell::Xt(exit_xt)).map_err(&make_err)?;

        // Execute the temporary buffer.
        let run_result = self.vm.run(buf_start);

        // Reset the dictionary pointer to discard the temporary buffer.
        self.vm.dp = buf_start;
        self.vm.dictionary.truncate(buf_start);

        run_result.map_err(make_err)
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
/// Returns 0 for an empty slice, otherwise `top_level_commas + 1`.
fn count_top_level_arity(tokens: &[SpannedToken]) -> usize {
    if tokens.is_empty() {
        return 0;
    }
    let mut depth: usize = 0;
    let mut commas: usize = 0;
    for st in tokens {
        match &st.token {
            Token::LParen => depth += 1,
            Token::RParen => {
                depth = depth.saturating_sub(1);
            }
            Token::Comma if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas + 1
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
        assert_eq!(count_top_level_arity(&[]), 0);
    }
}
