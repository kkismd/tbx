use crate::array_ref::ArrayRef;
use crate::cell::{Cell, CompileEntry};
use crate::constants::MAX_DICTIONARY_CELLS;
use crate::dict::{EntryKind, WordEntry, FLAG_HIDDEN, FLAG_IMMEDIATE, FLAG_SYSTEM};
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::vm::{CompileState, VM};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Low-dependency primitives split out into category modules.
// `primitives.rs` remains the façade and registration entry point; the
// `pub use` re-exports keep `crate::primitives::<name>` paths working for
// existing callers and tests.
mod arrays;
mod compare;
mod logic;
mod memory;
mod numeric;
mod stack;
mod strings;

pub use arrays::*;
pub use compare::*;
pub use logic::*;
pub use memory::*;
pub use numeric::*;
pub use stack::*;
pub use strings::*;

/// GET_OUTPUT — take the current output buffer and return it as a `Cell::Str`.
/// The output buffer is cleared after this call.
///
/// This primitive is intended for testing: it lets TBX programs inspect what
/// was written to the output so far, without requiring a host-side test runner.
pub fn get_output_prim(vm: &mut VM) -> Result<(), TbxError> {
    let output = vm.take_output();
    vm.push(Cell::string(output))?;
    Ok(())
}

/// PUTCHR — output the integer value on the stack as a single ASCII character (no newline).
pub fn putchr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let code = vm.pop_int()?;
    if !(0..=127).contains(&code) {
        return Err(TbxError::TypeError {
            expected: "ASCII code (0-127)",
            got: "out of range",
        });
    }
    let ch = code as u8 as char;
    vm.write_output(&ch.to_string());
    Ok(())
}

/// PUTDEC — output the numeric value on the stack as a signed decimal number (no newline).
/// Accepts both `Int` and `Float` values.
pub fn putdec_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop_number()?;
    vm.write_output(&cell.to_string());
    Ok(())
}

/// PUTHEX — output the integer value on the stack as $-prefixed uppercase hex (no newline).
/// Negative values are output as two's complement 64-bit representation.
pub fn puthex_prim(vm: &mut VM) -> Result<(), TbxError> {
    let n = vm.pop_int()?;
    if n < 0 {
        vm.write_output(&format!("${:X}", n as u64));
    } else {
        vm.write_output(&format!("${:X}", n));
    }
    Ok(())
}

/// PUTVAL — output any user-facing Cell value to the output buffer.
///
/// Dispatches on Cell type:
///   Int    → decimal string
///   Float  → floating-point string (same as Cell::Float Display)
///   Bool   → "TRUE" or "FALSE"
///   Str    → resolved string content
///   other  → TypeError
pub fn putval_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    match cell {
        Cell::Int(n) => vm.write_output(&n.to_string()),
        Cell::Float(v) => {
            // Mirror Cell::Float Display: finite values always include a decimal
            // point (e.g. 1.0 → "1.0"), non-finite values are printed as-is.
            let s = if v.is_finite() {
                let raw = format!("{v}");
                if raw.contains('.') || raw.contains('e') {
                    raw
                } else {
                    format!("{v}.0")
                }
            } else {
                format!("{v}")
            };
            vm.write_output(&s);
        }
        Cell::Bool(b) => vm.write_output(if b { "TRUE" } else { "FALSE" }),
        Cell::Str(rc) => {
            vm.write_output(rc.as_ref());
        }
        other => {
            return Err(TbxError::TypeError {
                expected: "Int, Float, Bool, or Str",
                got: other.type_name(),
            })
        }
    }
    Ok(())
}

/// APPEND — pop a Cell and write it to dictionary[dp], advancing dp by 1.
pub fn append_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    vm.dict_write(cell)
}

/// ALLOT — pop N from the stack, advance dp by N cells, and push the start address.
pub fn allot_prim(vm: &mut VM) -> Result<(), TbxError> {
    let n = vm.pop_int()?;
    if n < 0 {
        return Err(TbxError::InvalidAllotCount);
    }
    let count = n as usize;
    let new_dp = vm.dp + count;
    if new_dp > MAX_DICTIONARY_CELLS {
        return Err(TbxError::DictionaryOverflow {
            requested: new_dp,
            limit: MAX_DICTIONARY_CELLS,
        });
    }
    let start = vm.dp;
    for _ in 0..count {
        vm.dict_write(Cell::None)?;
    }
    vm.push(Cell::DictAddr(start))?;
    Ok(())
}

/// HERE — push the current dictionary pointer as a DictAddr.
pub fn here_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::DictAddr(vm.dp))?;
    Ok(())
}

/// STATE — push the current compile mode flag as an Int (0 = execute, 1 = compile).
pub fn state_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::Int(if vm.is_compiling { 1 } else { 0 }))?;
    Ok(())
}

/// HALT — stop VM execution by returning a Halted error.
pub fn halt_prim(_vm: &mut VM) -> Result<(), TbxError> {
    Err(TbxError::Halted)
}

/// ASSERT_FAIL — raise an AssertionFailed error unconditionally.
pub fn assert_fail_prim(_vm: &mut VM) -> Result<(), TbxError> {
    Err(TbxError::AssertionFailed)
}

/// ASSERT_FAIL_MSG — pop a string message from the stack and raise AssertionFailedWithMessage.
///
/// Expects a `Cell::Str` on top of the data stack.
pub fn assert_fail_msg_prim(vm: &mut VM) -> Result<(), TbxError> {
    let message = vm.pop_string_value()?.to_string();
    Err(TbxError::AssertionFailedWithMessage { message })
}

/// INT — truncate a numeric value toward zero and return it as `Cell::Int`.
///
/// - `Cell::Float(v)` → `Cell::Int(v.trunc() as i64)` (truncation toward zero)
/// - `Cell::Int(n)` → `Cell::Int(n)` (identity)
/// - any other type → `TbxError::TypeError`
pub fn int_prim(vm: &mut VM) -> Result<(), TbxError> {
    let val = vm.pop()?;
    match val {
        Cell::Float(v) => {
            vm.push(Cell::Int(v.trunc() as i64))?;
        }
        Cell::Int(n) => {
            vm.push(Cell::Int(n))?;
        }
        other => {
            return Err(TbxError::TypeError {
                expected: "Int or Float",
                got: other.type_name(),
            });
        }
    }
    Ok(())
}

/// LITERAL — compile a literal value into the dictionary as LIT + value (2 cells).
pub fn literal_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    let lit_xt = vm.lookup("LIT").ok_or(TbxError::TypeError {
        expected: "LIT word to be registered",
        got: "not found",
    })?;
    vm.dict_write(Cell::Xt(lit_xt))?;
    vm.dict_write(value)?;
    Ok(())
}
/// HEADER — read the next token as a word name and create a new dictionary entry.
///
/// `HEADER name ( -- )` — consumes the next identifier token from `vm.token_stream`,
/// creates a new `WordEntry` with `EntryKind::Word(vm.dp)` at the current DP,
/// and registers it via `vm.register()`. The `immediate` flag is `false` (not set).
///
/// This is the TBX equivalent of Forth's `CREATE`.
pub fn header_prim(vm: &mut VM) -> Result<(), TbxError> {
    let name = vm.expect_ident("HEADER: expected identifier token")?;
    let entry = WordEntry::new_word(&name, vm.dp);
    vm.register(entry);
    Ok(())
}

/// IMMEDIATE — read the next token as a word name and set FLAG_IMMEDIATE on it.
///
/// `IMMEDIATE name ( -- )` — consumes the next identifier token from `vm.token_stream`,
/// looks up the word in the dictionary, and sets its `FLAG_IMMEDIATE` flag.
/// Returns an error if the word is not found or the token is not an identifier.
///
/// Unlike Forth's `IMMEDIATE` (which implicitly operates on the most recently defined word),
/// TBX requires the target word name to be specified explicitly.
pub fn immediate_prim(vm: &mut VM) -> Result<(), TbxError> {
    let name = vm.expect_ident("IMMEDIATE: expected identifier token")?;
    let xt = vm
        .lookup(&name)
        .ok_or_else(|| TbxError::UndefinedSymbol { name: name.clone() })?;
    vm.headers[xt.index()].flags |= FLAG_IMMEDIATE;
    Ok(())
}

// ---------------------------------------------------------------------------
// IMMEDIATE compile-time primitives
// ---------------------------------------------------------------------------

/// DEF — begin compiling a new word definition.
/// Reads word name and optional parameter list from token_stream.
pub fn def_prim(vm: &mut VM) -> Result<(), TbxError> {
    // Nested DEF is not allowed.
    if vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "nested DEF is not allowed",
        });
    }

    // Read word name from token stream.
    let name = vm.expect_ident("expected word name after DEF")?;

    // Parse optional parameter list: DEF WORD(X, Y, ...) or DEF WORD(...)
    //
    // DFA with 5 states:
    //   LParenOrEnd      — after word name: expect '(' or EOL
    //   FirstParamOrEnd  — right after '(': expect ident, '...', or ')'
    //   CommaOrRParen    — after registering a param: expect ',' or ')'
    //   NextParam        — after ',': next must be ident or '...'  (')' = trailing-comma error)
    //   AfterEllipsis    — after '...': only ')' is valid
    enum DefParseState {
        LParenOrEnd,
        FirstParamOrEnd,
        CommaOrRParen,
        NextParam,
        AfterEllipsis,
    }

    let mut local_table: HashMap<String, usize> = HashMap::new();
    let mut arity: usize = 0;
    let mut is_variadic: bool = false;
    let mut state = DefParseState::LParenOrEnd;

    loop {
        match vm.next_token() {
            Ok(tok) => match (&state, tok.token) {
                // --- LParenOrEnd ---
                (DefParseState::LParenOrEnd, crate::lexer::Token::LParen) => {
                    state = DefParseState::FirstParamOrEnd;
                }
                (DefParseState::LParenOrEnd, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected '(' or end of line after word name in DEF",
                    });
                }

                // --- FirstParamOrEnd: immediately after '(' ---
                (DefParseState::FirstParamOrEnd, crate::lexer::Token::RParen) => {
                    break; // Empty parameter list: DEF WORD().
                }
                (DefParseState::FirstParamOrEnd, crate::lexer::Token::Ident(param)) => {
                    local_table.insert(param.to_ascii_uppercase(), arity);
                    arity += 1;
                    state = DefParseState::CommaOrRParen;
                }
                (DefParseState::FirstParamOrEnd, crate::lexer::Token::Ellipsis) => {
                    // DEF WORD(...) — variadic with zero fixed parameters.
                    is_variadic = true;
                    state = DefParseState::AfterEllipsis;
                }
                (DefParseState::FirstParamOrEnd, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected identifier, '...', or ')' after '('",
                    });
                }

                // --- CommaOrRParen: after registering a parameter ---
                (DefParseState::CommaOrRParen, crate::lexer::Token::RParen) => {
                    break; // Normal end of parameter list.
                }
                (DefParseState::CommaOrRParen, crate::lexer::Token::Comma) => {
                    state = DefParseState::NextParam;
                }
                (DefParseState::CommaOrRParen, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected ',' or ')' after parameter name",
                    });
                }

                // --- NextParam: after ',' ---
                (DefParseState::NextParam, crate::lexer::Token::Ident(param)) => {
                    let param = param.to_ascii_uppercase();
                    if local_table.contains_key(&param) {
                        return Err(TbxError::InvalidExpression {
                            reason: "duplicate parameter name in parameter list",
                        });
                    }
                    local_table.insert(param, arity);
                    arity += 1;
                    state = DefParseState::CommaOrRParen;
                }
                (DefParseState::NextParam, crate::lexer::Token::Ellipsis) => {
                    // DEF WORD(X, ...) — variadic with one or more fixed parameters.
                    is_variadic = true;
                    state = DefParseState::AfterEllipsis;
                }
                (DefParseState::NextParam, crate::lexer::Token::RParen) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "trailing comma before ')' is not allowed",
                    });
                }
                (DefParseState::NextParam, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected identifier or '...' after ',' in parameter list",
                    });
                }

                // --- AfterEllipsis: after '...' — only ')' is valid ---
                (DefParseState::AfterEllipsis, crate::lexer::Token::RParen) => {
                    break; // '...' followed by ')': valid variadic end.
                }
                (DefParseState::AfterEllipsis, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected ')' after '...' in parameter list",
                    });
                }
            },
            Err(TbxError::TokenStreamEmpty) => match state {
                DefParseState::LParenOrEnd => break, // No parameter list — normal end.
                _ => {
                    return Err(TbxError::InvalidExpression {
                        reason: "unclosed '(' in parameter list",
                    });
                }
            },
            Err(e) => return Err(e),
        }
    }

    // Snapshot for rollback.
    let dp_at_def = vm.dp;
    let hdr_len_at_def = vm.headers.len();
    let saved_latest = vm.latest;

    // Register the new word (smudged until END).
    let entry = crate::dict::WordEntry::new_word(&name, vm.dp);
    vm.register(entry);
    // Smudge: hide the word from lookup until END completes.
    vm.headers[hdr_len_at_def].flags |= crate::dict::FLAG_HIDDEN;

    vm.is_compiling = true;
    vm.compile_state = Some(CompileState::new_for_def(
        name,
        dp_at_def,
        hdr_len_at_def,
        saved_latest,
        local_table,
        arity,
        is_variadic,
    ));

    Ok(())
}

/// VA_COUNT ( -- n ) — return the total number of arguments passed to the current call.
///
/// Returns `actual_arity` from the innermost `ReturnFrame::Call` on the return stack.
/// This includes both fixed (named) parameters and any variadic arguments.
/// Useful in variadic words defined with `DEF WORD(X, ...)` to determine how many
/// arguments were actually passed.
pub fn va_count_prim(vm: &mut VM) -> Result<(), TbxError> {
    use crate::cell::ReturnFrame;
    let actual_arity = match vm.return_stack.last() {
        Some(ReturnFrame::Call { actual_arity, .. }) => *actual_arity,
        Some(ReturnFrame::TopLevel) | None => {
            return Err(TbxError::InvalidReturn);
        }
    };
    vm.push(Cell::Int(actual_arity as i64))?;
    Ok(())
}

/// ARG_ADDR ( index -- addr ) — return the StackAddr for the argument at the given index.
///
/// Pops `index` (zero-based) from the stack, validates it against `actual_arity` from
/// the current return frame, and pushes `Cell::StackAddr(index)`.  The caller can then
/// use `FETCH` or `STORE` to read or write the argument value at `data_stack[bp + index]`.
///
/// Argument indices are always in `[0, actual_arity)` and are well below
/// `VARIADIC_LOCAL_BASE`, so `resolve_local_idx` maps them directly to `bp + index`.
///
/// Returns `TbxError::IndexOutOfBounds` if `index >= actual_arity`.
pub fn arg_addr_prim(vm: &mut VM) -> Result<(), TbxError> {
    use crate::cell::ReturnFrame;
    let actual_arity = match vm.return_stack.last() {
        Some(ReturnFrame::Call { actual_arity, .. }) => *actual_arity,
        Some(ReturnFrame::TopLevel) | None => {
            return Err(TbxError::InvalidReturn);
        }
    };
    let index_raw = vm.pop_int()?;
    if index_raw < 0 || index_raw as usize >= actual_arity {
        return Err(TbxError::IndexOutOfBounds {
            index: index_raw.max(0) as usize,
            size: actual_arity,
        });
    }
    // Argument indices are in [0, actual_arity) which is always < VARIADIC_LOCAL_BASE,
    // so resolve_local_idx maps StackAddr(index) directly to bp + index. No adjustment needed.
    vm.push(Cell::StackAddr(index_raw as usize))?;
    Ok(())
}

/// END — finish compiling the current word definition.
pub fn end_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "END outside DEF",
        });
    }

    // Check for unpatched compile-stack items before finalising the word.
    if !vm.compile_stack.is_empty() {
        let count = vm.compile_stack.len();
        vm.rollback_def();
        return Err(TbxError::CompileStackNotEmpty { count });
    }

    // Write EXIT to terminate the word body.
    let exit_xt =
        vm.find_by_kind(|k| matches!(k, EntryKind::Exit))
            .ok_or(TbxError::UndefinedSymbol {
                name: "EXIT".to_string(),
            })?;
    vm.dict_write(Cell::Xt(exit_xt))?;

    // Check for unresolved forward label references BEFORE taking compile_state.
    if let Some(state) = &vm.compile_state {
        if let Some(&(label, _)) = state.patch_list.first() {
            vm.rollback_def();
            return Err(TbxError::UndefinedLabel { label });
        }
    }

    // Save rollback information before consuming compile_state.
    // If dict_write_at fails after take(), we need these to restore the VM.
    let (dp_at_def, hdr_len_at_def, saved_latest) =
        vm.compile_state.as_ref().map(|s| s.rollback_info()).ok_or(
            TbxError::InvalidExpression {
                reason: "END without matching DEF",
            },
        )?;

    // Take the compile state.
    let state = vm.compile_state.take().ok_or(TbxError::InvalidExpression {
        reason: "END without matching DEF",
    })?;

    // Patch all self-recursive CALL instructions with the confirmed local_count.
    // If patching fails, perform a full rollback so the VM is left in a clean state.
    for &pos in &state.call_patch_list {
        if let Err(e) = vm.dict_write_at(pos, Cell::Int(state.local_count as i64)) {
            vm.rollback_def_explicit(dp_at_def, hdr_len_at_def, saved_latest);
            return Err(e);
        }
    }
    // Update word header: confirm arity, local_count, is_variadic, unsmudge.
    let word_hdr_idx = state.word_hdr_idx();
    if word_hdr_idx < vm.headers.len() {
        vm.headers[word_hdr_idx].arity = state.arity;
        vm.headers[word_hdr_idx].local_count = state.local_count;
        vm.headers[word_hdr_idx].is_variadic = state.is_variadic;
        vm.headers[word_hdr_idx].flags &= !crate::dict::FLAG_HIDDEN;
    }

    vm.seal_user();
    vm.is_compiling = false;

    Ok(())
}

/// VAR — declare one or more local variables (in compile mode) or global variables (in execute
/// mode). Accepts a comma-separated list of identifiers: `VAR A`, `VAR A, B, C`.
///
/// In compile mode, the optional `= expr` initializer syntax is supported for a single
/// variable: `VAR X = expr`.  This emits `LIT StackAddr(X) <expr> SET` immediately
/// after the declaration, equivalent to `VAR X` followed by `SET &X, expr`.
/// Initializers are not supported in global (execute-mode) declarations.
pub fn var_prim(vm: &mut VM) -> Result<(), TbxError> {
    loop {
        // Read the next identifier.
        let name = vm.expect_ident("expected variable name after VAR")?;

        if vm.is_compiling {
            // Local variable: add to compile state's local table.
            let state = vm
                .compile_state
                .as_mut()
                .ok_or(TbxError::InvalidExpression {
                    reason: "VAR in compile mode but no compile_state",
                })?;

            // Reject duplicate local variable names regardless of whether an initializer is present.
            if state.local_table.contains_key(&name) {
                return Err(TbxError::InvalidExpression {
                    reason: "duplicate local variable name in VAR declaration",
                });
            }

            // For variadic words, use the VARIADIC_LOCAL_BASE offset so that local-variable
            // StackAddr indices are in a disjoint range from argument indices.
            // This allows ARG_ADDR to return raw argument indices without ambiguity.
            let idx = if state.is_variadic {
                crate::constants::VARIADIC_LOCAL_BASE + state.local_count
            } else {
                state.arity + state.local_count
            };
            state.local_table.insert(name.clone(), idx);
            state.local_count += 1;

            // Check for optional `= expr` initializer.
            match vm.next_token() {
                Ok(tok) if matches!(tok.token, crate::lexer::Token::Op(ref s) if s == "=") => {
                    // Initializer present: drain remaining tokens as the RHS expression.
                    let expr_tokens: Vec<crate::lexer::SpannedToken> = {
                        let stream = vm.token_stream.as_mut().ok_or(TbxError::TokenStreamEmpty)?;
                        stream.drain(..).collect()
                    };
                    if expr_tokens.is_empty() {
                        return Err(TbxError::InvalidExpression {
                            reason: "VAR initializer: expected expression after '='",
                        });
                    }

                    // Emit: LIT StackAddr(idx)
                    let lit_xt = vm.find_by_kind(|k| matches!(k, EntryKind::Lit)).ok_or(
                        TbxError::UndefinedSymbol {
                            name: "LIT".to_string(),
                        },
                    )?;
                    vm.dict_write(Cell::Xt(lit_xt))?;
                    vm.dict_write(Cell::StackAddr(idx))?;

                    // Compile and emit the RHS expression.
                    let (expr_cells, patch_offsets) =
                        compile_expr_taking_local_table(vm, &expr_tokens)?;
                    let base_dp = vm.dp;
                    for cell in expr_cells {
                        vm.dict_write(cell)?;
                    }
                    if let Some(state) = vm.compile_state.as_mut() {
                        for offset in patch_offsets {
                            state.call_patch_list.push(base_dp + offset);
                        }
                    }

                    // Emit: SET
                    let set_xt = vm.lookup("SET").ok_or(TbxError::UndefinedSymbol {
                        name: "SET".to_string(),
                    })?;
                    vm.dict_write(Cell::Xt(set_xt))?;

                    // `VAR X = expr` covers the rest of the token stream; stop the loop.
                    break;
                }
                Ok(tok) if matches!(tok.token, crate::lexer::Token::Comma) => {
                    // Comma: continue to the next identifier (no initializer on this var).
                }
                Ok(tok) => {
                    // Not `=` and not `,`: return the token to the front of the stream and stop.
                    if let Some(stream) = vm.token_stream.as_mut() {
                        stream.push_front(tok);
                    }
                    break;
                }
                Err(TbxError::TokenStreamEmpty) => {
                    // End of stream: stop normally.
                    break;
                }
                Err(e) => return Err(e),
            }
        } else {
            // Peek at the next token before registering the global variable.
            // If it is `=`, reject it immediately without touching the dictionary.
            // If it is a comma, consume it and register the variable, then continue.
            // Otherwise push the token back and register the variable, then stop.
            match vm.next_token() {
                Ok(tok) if matches!(tok.token, crate::lexer::Token::Op(ref s) if s == "=") => {
                    // `=` found at top level: VAR initializers are only allowed inside DEF.
                    // Return the error *before* registering the variable so that the
                    // dictionary is not modified on failure.
                    return Err(TbxError::InvalidExpression {
                        reason: "VAR initializer '= expr' is not allowed outside DEF",
                    });
                }
                peek => {
                    // No `=`: register the global variable now that we know it is safe.
                    let storage_idx = vm.dp;
                    vm.dict_write(Cell::None)?;
                    let entry = crate::dict::WordEntry::new_variable(&name, storage_idx);
                    vm.register(entry);
                    vm.seal_user();

                    match peek {
                        Ok(tok) if matches!(tok.token, crate::lexer::Token::Comma) => {
                            // Comma consumed; loop to read the next identifier.
                        }
                        Ok(tok) => {
                            // Not a comma: return the token to the front of the stream and stop.
                            if let Some(stream) = vm.token_stream.as_mut() {
                                stream.push_front(tok);
                            }
                            break;
                        }
                        Err(TbxError::TokenStreamEmpty) => {
                            // End of stream: stop normally.
                            break;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    Ok(())
}

/// DIM — declare an array binding `DIM @A[n]`.
///
/// Syntax: `DIM @ Ident [ size_expr ]`
///
/// In compile mode (inside DEF..END):
///   - Registers the bare identifier in `local_table` as a new local slot.
///   - Emits code that creates an array of `size_expr` elements at runtime and
///     stores it in the local slot: `LIT StackAddr(idx) [size_expr] ARRAY SET`.
///
/// In execute mode (top level):
///   - Creates a global variable entry (`EntryKind::Variable`) backed by a
///     `Cell::Array` in the array pool.
///   - The size expression is evaluated immediately via a temporary code buffer.
///
/// Collision rules (bare name used as the key in both modes):
///   - A name already present in `local_table` is rejected (duplicate local).
///   - A name already present in the global dictionary (`vm.lookup`) is rejected.
pub fn dim_prim(vm: &mut VM) -> Result<(), TbxError> {
    use crate::lexer::Token;

    // Consume `@`.
    let at_tok = vm.next_token()?;
    if at_tok.token != Token::At {
        return Err(TbxError::InvalidExpression {
            reason: "DIM: expected '@' after DIM",
        });
    }

    // Consume the binding identifier.
    let name = vm.expect_ident("DIM: expected identifier after '@'")?;

    // Consume `[`.
    let lb_tok = match vm.next_token() {
        Ok(tok) => tok,
        Err(TbxError::TokenStreamEmpty) => {
            return Err(TbxError::InvalidExpression {
                reason: "DIM: expected '[' after identifier (use DIM @A[n] syntax)",
            });
        }
        Err(e) => return Err(e),
    };
    if lb_tok.token != Token::LBracket {
        return Err(TbxError::InvalidExpression {
            reason: "DIM: expected '[' after identifier (use DIM @A[n] syntax)",
        });
    }

    // Collect tokens up to the matching `]` (depth-aware in case size_expr
    // contains nested brackets, e.g. tuple projection in a future phase).
    let size_tokens: Vec<crate::lexer::SpannedToken> = {
        let stream = vm.token_stream.as_mut().ok_or(TbxError::TokenStreamEmpty)?;
        let mut depth: usize = 1;
        let mut collected: Vec<crate::lexer::SpannedToken> = Vec::new();
        loop {
            let tok = stream.pop_front().ok_or(TbxError::InvalidExpression {
                reason: "DIM: unterminated '[' — expected ']'",
            })?;
            match &tok.token {
                Token::LBracket => {
                    depth += 1;
                    collected.push(tok);
                }
                Token::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    collected.push(tok);
                }
                Token::Newline | Token::Eof => {
                    return Err(TbxError::InvalidExpression {
                        reason: "DIM: unterminated '[' — unexpected end of line",
                    });
                }
                _ => {
                    collected.push(tok);
                }
            }
        }
        collected
    };

    // Reject empty size expression: `DIM @A[]`.
    if size_tokens.is_empty() {
        return Err(TbxError::InvalidExpression {
            reason: "DIM: array size expression must not be empty",
        });
    }

    if vm.is_compiling {
        // --- Compile mode (inside DEF) ---

        // Check for name collision in local_table.
        let has_local = vm
            .compile_state
            .as_ref()
            .map(|s| s.local_table.contains_key(&name))
            .unwrap_or(false);
        if has_local {
            return Err(TbxError::InvalidExpression {
                reason: "DIM: array binding name already declared as a local",
            });
        }

        // Check for name collision in the global dictionary.
        if vm.lookup(&name).is_some() {
            return Err(TbxError::InvalidExpression {
                reason: "DIM: array binding name already declared as a global",
            });
        }

        // Allocate a new local slot for the array handle.
        let state = vm
            .compile_state
            .as_mut()
            .ok_or(TbxError::InvalidExpression {
                reason: "DIM: compile mode but no compile_state",
            })?;
        let idx = if state.is_variadic {
            crate::constants::VARIADIC_LOCAL_BASE + state.local_count
        } else {
            state.arity + state.local_count
        };
        state.local_table.insert(name.clone(), idx);
        state.local_count += 1;

        // Compile the size expression.
        let (size_cells, patch_offsets) = compile_expr_taking_local_table(vm, &size_tokens)?;

        // Look up primitives needed for code generation.
        let lit_xt =
            vm.find_by_kind(|k| matches!(k, EntryKind::Lit))
                .ok_or(TbxError::UndefinedSymbol {
                    name: "LIT".to_string(),
                })?;
        let array_xt = vm
            .lookup_hidden_system("ARRAY")
            .ok_or(TbxError::UndefinedSymbol {
                name: "ARRAY".to_string(),
            })?;
        // Use ARRAY_STORE_LOCAL instead of SET so that the array handle bypasses
        // the Cell::Array rejection guard in set_prim.
        let array_store_local_xt =
            vm.lookup_hidden_system("ARRAY_STORE_LOCAL")
                .ok_or(TbxError::UndefinedSymbol {
                    name: "ARRAY_STORE_LOCAL".to_string(),
                })?;

        // Emit: LIT StackAddr(idx)  — push the address of the local slot.
        vm.dict_write(Cell::Xt(lit_xt))?;
        vm.dict_write(Cell::StackAddr(idx))?;

        // Emit: [size_cells]  — evaluate the size at runtime.
        let base_dp = vm.dp;
        for cell in size_cells {
            vm.dict_write(cell)?;
        }
        // Register self-recursive call patch positions.
        if let Some(state) = vm.compile_state.as_mut() {
            for offset in patch_offsets {
                state.call_patch_list.push(base_dp + offset);
            }
        }

        // Emit: ARRAY  — create the array and push its handle.
        vm.dict_write(Cell::Xt(array_xt))?;

        // Emit: ARRAY_STORE_LOCAL  — store the array handle into the local slot.
        // This hidden primitive accepts Cell::Array; surface SET / STORE do not.
        vm.dict_write(Cell::Xt(array_store_local_xt))?;
    } else {
        // --- Execute mode (top level) ---

        // Check for name collision in the global dictionary.
        if vm.lookup(&name).is_some() {
            return Err(TbxError::InvalidExpression {
                reason: "DIM: array binding name already declared as a global",
            });
        }

        // Evaluate the size expression by compiling it to a temporary code
        // buffer and running it.  The result (Cell::Int) is left on the stack.
        let (size_cells, _patch_offsets) = compile_expr_taking_local_table(vm, &size_tokens)?;

        // Build temporary code buffer: [size_cells, EXIT].
        let buf_start = vm.dp;
        for cell in &size_cells {
            vm.dict_write(cell.clone())?;
        }
        let exit_xt =
            vm.find_by_kind(|k| matches!(k, EntryKind::Exit))
                .ok_or(TbxError::UndefinedSymbol {
                    name: "EXIT".to_string(),
                })?;
        vm.dict_write(Cell::Xt(exit_xt))?;

        // Snapshot VM state before running the temporary buffer so we can
        // fully restore it if the size expression evaluation fails.
        let saved_stack_len = vm.data_stack.len();
        let saved_return_stack_len = vm.return_stack.len();
        let saved_pc = vm.pc;
        let saved_bp = vm.bp;

        let run_result = vm.run(buf_start);

        // Clean up the temporary code buffer regardless of outcome.
        vm.dp = buf_start;
        vm.dictionary.truncate(buf_start);

        if let Err(e) = run_result {
            // Restore all VM state that vm.run() may have mutated before
            // the error, including return_stack frames pushed by user words
            // called inside the size expression.
            vm.data_stack.truncate(saved_stack_len);
            vm.return_stack.truncate(saved_return_stack_len);
            vm.pc = saved_pc;
            vm.bp = saved_bp;
            return Err(e);
        }

        // Pop the evaluated size from the stack.
        let size_val = vm.pop()?;
        let size = match size_val {
            Cell::Int(n) => {
                if n <= 0 {
                    return Err(TbxError::InvalidArgument {
                        message: format!("DIM: array size must be positive, got {n}"),
                    });
                }
                n as usize
            }
            other => {
                return Err(TbxError::TypeError {
                    expected: "Int",
                    got: other.type_name(),
                });
            }
        };

        // Allocate the global variable slot in the dictionary.
        let storage_idx = vm.dp;
        vm.dict_write(Cell::None)?;

        // Create the array in the array pool.
        let pool_idx = vm.arrays.len();
        vm.arrays.push(ArrayRef::new(vec![Cell::None; size]));

        // Promote to the global array region so it persists across word calls.
        vm.global_array_pool_len = vm.global_array_pool_len.max(pool_idx + 1);

        // Store the array handle in the variable slot.
        vm.dict_write_at(storage_idx, Cell::Array(pool_idx))?;

        // Register the variable in the dictionary.
        let entry = crate::dict::WordEntry::new_variable(&name, storage_idx);
        vm.register(entry);
        vm.seal_user();
    }

    Ok(())
}

/// GOTO — compile GOTO N into the dictionary (compile mode only).
pub fn goto_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "GOTO outside DEF",
        });
    }

    // Drain remaining tokens and parse the label number, skipping Newline/Eof,
    // consistent with bif_prim/bit_prim which also use parse_label_number().
    let remaining: Vec<crate::lexer::SpannedToken> = {
        let stream = vm.token_stream.as_mut().ok_or(TbxError::TokenStreamEmpty)?;
        stream.drain(..).collect()
    };
    let label_n =
        crate::lexer::parse_label_number(&remaining).ok_or(TbxError::InvalidExpression {
            reason: "GOTO requires an integer label",
        })?;

    // Find the runtime Goto entry by kind (not by name, to avoid shadowing by this primitive).
    let goto_xt =
        vm.find_by_kind(|k| matches!(k, EntryKind::Goto))
            .ok_or(TbxError::UndefinedSymbol {
                name: "GOTO".to_string(),
            })?;
    vm.dict_write(Cell::Xt(goto_xt))?;
    emit_jump_target_to_dict(vm, label_n)
}

/// BIF — compile BIF cond, label into the dictionary (compile mode only).
pub fn bif_prim(vm: &mut VM) -> Result<(), TbxError> {
    compile_branch_prim(vm, false)
}

/// BIT — compile BIT cond, label into the dictionary (compile mode only).
pub fn bit_prim(vm: &mut VM) -> Result<(), TbxError> {
    compile_branch_prim(vm, true)
}

/// Shared implementation for BIF and BIT primitives.
fn compile_branch_prim(vm: &mut VM, is_truthy: bool) -> Result<(), TbxError> {
    if !vm.is_compiling {
        let reason = if is_truthy {
            "BIT outside DEF"
        } else {
            "BIF outside DEF"
        };
        return Err(TbxError::InvalidExpression { reason });
    }

    // Drain all remaining tokens from the token stream.
    let all_tokens: Vec<crate::lexer::SpannedToken> = {
        let stream = vm.token_stream.as_mut().ok_or(TbxError::TokenStreamEmpty)?;
        stream.drain(..).collect()
    };

    // Split at the last top-level comma: left=cond_tokens, right=label_tokens.
    let split_pos =
        crate::lexer::last_top_level_comma(&all_tokens)?.ok_or(TbxError::InvalidExpression {
            reason: "BIF/BIT requires syntax: BIF cond, label",
        })?;
    let cond_tokens = &all_tokens[..split_pos];
    let label_tokens = &all_tokens[split_pos + 1..];

    // Parse label number.
    let label_n =
        crate::lexer::parse_label_number(label_tokens).ok_or(TbxError::InvalidExpression {
            reason: "BIF/BIT label must be an integer",
        })?;

    // Compile condition expression.
    let (cond_cells, patch_offsets) = compile_expr_taking_local_table(vm, cond_tokens)?;

    let base_dp = vm.dp;
    for cell in cond_cells {
        vm.dict_write(cell)?;
    }
    // Register self-recursive local_count placeholder positions.
    if let Some(state) = vm.compile_state.as_mut() {
        for offset in patch_offsets {
            state.call_patch_list.push(base_dp + offset);
        }
    }

    // Emit BIF or BIT runtime instruction (found by kind to avoid shadowing).
    let branch_xt = if is_truthy {
        vm.find_by_kind(|k| matches!(k, EntryKind::BranchIfTrue))
            .ok_or(TbxError::UndefinedSymbol {
                name: "BIT".to_string(),
            })?
    } else {
        vm.find_by_kind(|k| matches!(k, EntryKind::BranchIfFalse))
            .ok_or(TbxError::UndefinedSymbol {
                name: "BIF".to_string(),
            })?
    };
    vm.dict_write(Cell::Xt(branch_xt))?;

    emit_jump_target_to_dict(vm, label_n)
}

/// RETURN — compile a RETURN statement inside a DEF body.
pub fn return_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "RETURN outside DEF",
        });
    }

    // Drain remaining tokens; require token_stream to be set (same contract as goto_prim / bif_prim).
    let expr_tokens: Vec<crate::lexer::SpannedToken> = {
        let stream = vm.token_stream.as_mut().ok_or(TbxError::TokenStreamEmpty)?;
        stream.drain(..).collect()
    };

    if expr_tokens.is_empty() {
        // Void return: emit EXIT.
        let exit_xt =
            vm.find_by_kind(|k| matches!(k, EntryKind::Exit))
                .ok_or(TbxError::UndefinedSymbol {
                    name: "EXIT".to_string(),
                })?;
        vm.dict_write(Cell::Xt(exit_xt))?;
    } else {
        // Compile return expression.
        let (expr_cells, patch_offsets) = compile_expr_taking_local_table(vm, &expr_tokens)?;

        let base_dp = vm.dp;
        for cell in expr_cells {
            vm.dict_write(cell)?;
        }
        if let Some(state) = vm.compile_state.as_mut() {
            for offset in patch_offsets {
                state.call_patch_list.push(base_dp + offset);
            }
        }
        // Find RETURN_VAL by kind.
        let return_val_xt = vm
            .find_by_kind(|k| matches!(k, EntryKind::ReturnVal))
            .ok_or(TbxError::UndefinedSymbol {
                name: "RETURN_VAL".to_string(),
            })?;
        vm.dict_write(Cell::Xt(return_val_xt))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helper functions for IMMEDIATE primitives
// ---------------------------------------------------------------------------

/// Compile an expression while temporarily taking `local_table` out of `compile_state`.
///
/// `ExprCompiler::with_context` requires `&mut VM`, but `local_table` lives inside
/// `vm.compile_state`.  By taking it out first we can pass `&mut vm` to `ExprCompiler`
/// and reference `local_table` separately without violating the borrow checker.
/// The table is always restored to `compile_state` after compilation, even on error.
fn compile_expr_taking_local_table(
    vm: &mut VM,
    tokens: &[crate::lexer::SpannedToken],
) -> Result<(Vec<Cell>, Vec<usize>), TbxError> {
    let self_word = vm.compile_state.as_ref().map(|s| s.word_name.clone());
    let self_hdr_idx = vm.compile_state.as_ref().map(|s| s.word_hdr_idx());
    let local_table = vm
        .compile_state
        .as_mut()
        .map(|s| std::mem::take(&mut s.local_table));
    let result: Result<(Vec<Cell>, Vec<usize>), TbxError> = {
        let local_table_ref = local_table.as_ref();
        let mut compiler = ExprCompiler::with_context(vm, local_table_ref, self_word, self_hdr_idx);
        compiler.compile_expr(tokens).map(|cells| {
            let offsets = std::mem::take(&mut compiler.patch_offsets);
            (cells, offsets)
        })
    };
    // Restore local_table regardless of success or failure.
    if let (Some(state), Some(lt)) = (vm.compile_state.as_mut(), local_table) {
        state.local_table = lt;
    }
    result
}

/// Emit a jump target into the dictionary, with forward-reference back-patch support.
///
/// # Design note: why `Cell::DictAddr`, not `Cell::Int`
///
/// Jump targets are execution-address indices into the dictionary.
/// Using `Cell::DictAddr` makes the semantic explicit at the type level and prevents
/// accidental confusion between arithmetic integers and program-counter values.
/// `PATCH_ADDR` follows the same convention: it takes a `DictAddr` operand (where to write)
/// and writes `Cell::DictAddr(dp)` (the target pc value).
fn emit_jump_target_to_dict(vm: &mut VM, label_n: i64) -> Result<(), TbxError> {
    let target_opt = vm
        .compile_state
        .as_ref()
        .ok_or(TbxError::InvalidExpression {
            reason: "GOTO/BIF/BIT outside compile mode",
        })?
        .label_table
        .get(&label_n)
        .copied();

    if let Some(target) = target_opt {
        vm.dict_write(Cell::DictAddr(target))?;
    } else {
        let patch_pos = vm.dp;
        vm.dict_write(Cell::DictAddr(0))?;
        vm.compile_state
            .as_mut()
            .ok_or(TbxError::InvalidExpression {
                reason: "GOTO/BIF/BIT outside compile mode",
            })?
            .patch_list
            .push((label_n, patch_pos));
    }
    Ok(())
}

/// CS_PUSH — move a value from the data stack to the compile stack.
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn cs_push_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_PUSH outside compile mode",
        });
    }
    let val = vm.pop()?;
    vm.compile_stack.push(CompileEntry::Cell(val));
    Ok(())
}

/// CS_POP — move a value from the compile stack to the data stack.
///
/// Only `CompileEntry::Cell` entries can be moved; a `CompileEntry::Tag` on top
/// returns `TypeError` (the tag is left on the compile stack unchanged).
/// Must be called in compile mode (inside a IMMEDIATE word invocation).
fn cs_pop_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_POP outside compile mode",
        });
    }
    let entry = vm.compile_stack.pop().ok_or(TbxError::StackUnderflow)?;
    match entry {
        CompileEntry::Cell(val) => {
            vm.push(val)?;
            Ok(())
        }
        CompileEntry::Tag(s) => {
            // Restore the tag and signal a type error: CS_POP cannot pop a tag.
            vm.compile_stack.push(CompileEntry::Tag(s));
            Err(TbxError::TypeError {
                expected: "Cell",
                got: "Tag",
            })
        }
    }
}

/// CS_SWAP — swap the top two values on the compile stack: ( a b -- b a ).
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn cs_swap_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_SWAP outside compile mode",
        });
    }
    let len = vm.compile_stack.len();
    if len < 2 {
        return Err(TbxError::StackUnderflow);
    }
    vm.compile_stack.swap(len - 1, len - 2);
    Ok(())
}

/// CS_DROP — discard the top value on the compile stack: ( a -- ).
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn cs_drop_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_DROP outside compile mode",
        });
    }
    vm.compile_stack.pop().ok_or(TbxError::StackUnderflow)?;
    Ok(())
}

/// CS_DUP — duplicate the top value on the compile stack: ( a -- a a ).
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn cs_dup_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_DUP outside compile mode",
        });
    }
    let top = vm
        .compile_stack
        .last()
        .ok_or(TbxError::StackUnderflow)?
        .clone();
    vm.compile_stack.push(top);
    Ok(())
}

/// CS_OVER — copy the second value on the compile stack to the top: ( a b -- a b a ).
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn cs_over_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_OVER outside compile mode",
        });
    }
    let len = vm.compile_stack.len();
    if len < 2 {
        return Err(TbxError::StackUnderflow);
    }
    let second = vm.compile_stack[len - 2].clone();
    vm.compile_stack.push(second);
    Ok(())
}

/// CS_ROT — rotate the top three values on the compile stack: ( a b c -- b c a ).
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn cs_rot_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_ROT outside compile mode",
        });
    }
    let len = vm.compile_stack.len();
    if len < 3 {
        return Err(TbxError::StackUnderflow);
    }
    // ( a b c -- b c a ): swap positions to achieve rotation in O(1).
    vm.compile_stack.swap(len - 3, len - 2); // [a,b,c] → [b,a,c]
    vm.compile_stack.swap(len - 2, len - 1); // [b,a,c] → [b,c,a]
    Ok(())
}

/// CS_OPEN_TAG — pop a string value from the data stack and push a `CompileEntry::Tag`
/// onto the compile stack.
///
/// Used by IMMEDIATE words (e.g. WHILE, IF) to mark the start of a control-structure
/// scope.  The string (e.g. `"WHILE"` or `"IF"`) is matched by a later CS_CLOSE_TAG
/// call to validate correct nesting.
/// Must be called in compile mode.
/// Expects a `Cell::Str` on top of the data stack.
fn cs_open_tag_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_OPEN_TAG outside compile mode",
        });
    }
    // `CompileEntry::Tag` holds an owned `String`, so materialise one from
    // the `Rc<str>` returned by `pop_string_value`.
    let tag = vm.pop_string_value()?.to_string();
    vm.compile_stack.push(CompileEntry::Tag(tag));
    Ok(())
}

/// CS_CLOSE_TAG — pop a string value from the data stack, then validate and remove the
/// matching `CompileEntry::Tag` from the top of the compile stack.
///
/// Returns `NoOpenTag` if the compile stack is empty or its top entry is a `Cell`
/// (not a `Tag`).  Returns `MismatchedTag` if the top is a `Tag` but does not match
/// the expected string.
/// Must be called in compile mode.
/// Expects a `Cell::Str` on top of the data stack.
fn cs_close_tag_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_CLOSE_TAG outside compile mode",
        });
    }
    // `TbxError::NoOpenTag` / `MismatchedTag` carry owned `String` values,
    // so we materialise an owned copy from the popped `Rc<str>`.
    let expected = vm.pop_string_value()?.to_string();
    match vm.compile_stack.pop() {
        None => Err(TbxError::NoOpenTag { expected }),
        Some(CompileEntry::Tag(found)) if found == expected => Ok(()),
        Some(CompileEntry::Tag(found)) => Err(TbxError::MismatchedTag { expected, found }),
        Some(CompileEntry::Cell(c)) => {
            // Restore the cell and report no matching open tag.
            vm.compile_stack.push(CompileEntry::Cell(c));
            Err(TbxError::NoOpenTag { expected })
        }
    }
}

/// PATCH_ADDR — pop a DictAddr from the data stack, then write Cell::DictAddr(dp) at that address.
///
/// Used by ENDIF, ENDWH, and future ELSE to back-patch a previously emitted
/// jump-target placeholder.  The address on the stack is typically saved by IF/WHILE via
/// CS_PUSH/CS_POP.
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation).
fn patch_addr_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "PATCH_ADDR outside compile mode",
        });
    }
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => vm.dict_write_at(a, Cell::DictAddr(vm.dp)),
        _ => Err(TbxError::TypeError {
            expected: "DictAddr",
            got: addr.type_name(),
        }),
    }
}

/// COMPILE_EXPR — compile the remaining tokens in the token stream as an expression
/// and write the result to the dictionary.
///
/// Consumes all remaining tokens from `token_stream`.
///
/// # Rollback contract
///
/// If `dict_write` fails partway through writing compiled cells, the dictionary
/// may be left in a partially-written state. The caller is responsible for
/// invoking `rollback_def()` to restore the dictionary to a consistent state.
/// In practice, `COMPILE_EXPR` is only called from within IMMEDIATE word bodies
/// (themselves compiled into a DEF..END definition), so any error will propagate
/// to `compile_program`, which calls `rollback_def()` on any `Err` return.
fn compile_expr_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "COMPILE_EXPR outside compile mode",
        });
    }
    // Drain all remaining tokens from the stream.
    let tokens: Vec<crate::lexer::SpannedToken> = match vm.token_stream.as_mut() {
        Some(stream) => stream.drain(..).collect(),
        None => return Err(TbxError::TokenStreamEmpty),
    };
    if tokens.is_empty() {
        return Err(TbxError::TokenStreamEmpty);
    }
    // Compile the expression using the current local variable table.
    // Use the take-compile-restore pattern to satisfy borrow checker:
    // take local_table out of compile_state, pass &mut VM to ExprCompiler,
    // then restore local_table unconditionally.
    let self_word = vm.compile_state.as_ref().map(|s| s.word_name.clone());
    let self_hdr_idx = vm.compile_state.as_ref().map(|s| s.word_hdr_idx());
    let local_table = vm
        .compile_state
        .as_mut()
        .map(|s| std::mem::take(&mut s.local_table));
    let compile_result: Result<(Vec<Cell>, Vec<usize>), TbxError> = {
        let local_table_ref = local_table.as_ref();
        let mut compiler =
            crate::expr::ExprCompiler::with_context(vm, local_table_ref, self_word, self_hdr_idx);
        compiler.compile_expr(&tokens).map(|cells| {
            let offsets = std::mem::take(&mut compiler.patch_offsets);
            (cells, offsets)
        })
    };
    // Restore local_table regardless of success or failure.
    if let (Some(state), Some(lt)) = (vm.compile_state.as_mut(), local_table) {
        state.local_table = lt;
    }
    let (cells, patch_offsets) = compile_result?;
    // Write compiled cells to the dictionary.
    let base_dp = vm.dp;
    for cell in &cells {
        vm.dict_write(cell.clone())?;
    }
    // Register patch offsets (adjust by base_dp to get absolute dictionary positions).
    if let Some(state) = vm.compile_state.as_mut() {
        for offset in patch_offsets {
            state.call_patch_list.push(base_dp + offset);
        }
    }
    Ok(())
}

/// SKIP_COMMA — read the next token from the token stream and validate it is `,`.
///
/// Used by FOR to consume the comma separator between the loop variable reference
/// and the start expression.
///
/// Must be called in compile mode. Returns `InvalidExpression` if the token is
/// not `Token::Comma`.
fn skip_comma_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "SKIP_COMMA outside compile mode",
        });
    }

    vm.expect_comma("SKIP_COMMA: expected ','")
}

/// COMPILE_LVALUE_SAVE — emit `LIT addr` to the dictionary and push addr onto the compile stack.
///
/// Combines the behaviour of `COMPILE_LVALUE` with a compile-stack push so that the
/// loop variable address is preserved across statement boundaries for use by FOR/NEXT.
///
/// Unlike pushing to the data stack (which would be discarded by `DROP_TO_MARKER` at
/// the end of each statement), the compile stack persists between statements inside an
/// IMMEDIATE word body.
fn compile_lvalue_save_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "COMPILE_LVALUE_SAVE outside compile mode",
        });
    }

    // Consume the leading `&` (address-of operator) before the variable name.
    vm.expect_ampersand("COMPILE_LVALUE_SAVE: expected '&' before variable name")?;

    let name = vm.expect_ident("COMPILE_LVALUE_SAVE: expected variable name")?;

    // Resolve address: local table first, then global dictionary.
    // Use the take→use→restore pattern to satisfy the borrow checker.
    let addr_cell = {
        let local_table = vm
            .compile_state
            .as_mut()
            .map(|s| std::mem::take(&mut s.local_table));

        let result: Result<Cell, TbxError> =
            if let Some(idx) = local_table.as_ref().and_then(|lt| lt.get(&name)).copied() {
                Ok(Cell::StackAddr(idx))
            } else {
                match vm.lookup(&name) {
                    None => Err(TbxError::UndefinedSymbol { name }),
                    Some(xt) => match &vm.headers[xt.index()].kind {
                        EntryKind::Variable(addr) => Ok(Cell::DictAddr(*addr)),
                        _ => Err(TbxError::TypeError {
                            expected: "variable",
                            got: "non-variable",
                        }),
                    },
                }
            };

        if let (Some(state), Some(lt)) = (vm.compile_state.as_mut(), local_table) {
            state.local_table = lt;
        }
        result?
    };

    // Emit LIT <addr> to the dictionary.
    let lit_xt =
        vm.find_by_kind(|k| matches!(k, EntryKind::Lit))
            .ok_or(TbxError::UndefinedSymbol {
                name: "LIT".to_string(),
            })?;
    vm.dict_write(Cell::Xt(lit_xt))?;
    vm.dict_write(addr_cell.clone())?;

    // Push addr onto the compile stack so it survives statement boundaries.
    vm.compile_stack.push(CompileEntry::Cell(addr_cell));
    Ok(())
}

/// COMPILE_LVALUE — read a variable name (or `@A[i]` array element form) from the token
/// stream and emit the corresponding lvalue address sequence to the dictionary.
///
/// Two forms are supported:
///
/// 1. Scalar: `LET A = expr`
///    Emits `LIT addr`, where `addr` is the variable's stack or dictionary address.
///    This is the compile-time counterpart to the `&var` address-of operator in expressions.
///    Locals (from `compile_state.local_table`) resolve to `StackAddr`; global variables
///    (`EntryKind::Variable`) resolve to `DictAddr`.
///
/// 2. Array element: `LET @A[i] = expr`
///    Emits `<array handle read>  <index expr>  ARRAY_ADDR`, which leaves the element
///    address on the stack, ready for the subsequent `SET` instruction emitted by the
///    caller (`LET` in `basic.tbx`).  This sequence is equivalent to `SET &@A[i], expr`.
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation that runs during a
/// DEF body compilation). Requires `token_stream` to be set.
fn compile_lvalue_prim(vm: &mut VM) -> Result<(), TbxError> {
    use crate::lexer::Token;

    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "COMPILE_LVALUE outside compile mode",
        });
    }

    // Peek at the next token to choose the scalar or array-element path.
    let first_tok = vm.next_token()?;

    match first_tok.token {
        Token::At => {
            // Array element lvalue: `LET @A[i] = expr`
            compile_at_array_lvalue(vm)
        }
        Token::Ident(raw_name) => {
            let name = raw_name.to_ascii_uppercase();
            // Scalar lvalue: resolve address and emit LIT <addr>.
            let addr_cell = {
                // Take local_table out to avoid borrow conflict with &mut vm below.
                let local_table = vm
                    .compile_state
                    .as_mut()
                    .map(|s| std::mem::take(&mut s.local_table));

                let result: Result<Cell, TbxError> =
                    if let Some(idx) = local_table.as_ref().and_then(|lt| lt.get(&name)).copied() {
                        Ok(Cell::StackAddr(idx))
                    } else {
                        // No `?` here — collect the result and restore local_table first.
                        match vm.lookup(&name) {
                            None => Err(TbxError::UndefinedSymbol { name }),
                            Some(xt) => match &vm.headers[xt.index()].kind {
                                EntryKind::Variable(addr) => Ok(Cell::DictAddr(*addr)),
                                _ => Err(TbxError::TypeError {
                                    expected: "variable",
                                    got: "non-variable",
                                }),
                            },
                        }
                    };

                // Restore local_table unconditionally before propagating any error.
                if let (Some(state), Some(lt)) = (vm.compile_state.as_mut(), local_table) {
                    state.local_table = lt;
                }
                result?
            };

            // Emit LIT <addr> to the dictionary.
            let lit_xt = vm.find_by_kind(|k| matches!(k, EntryKind::Lit)).ok_or(
                TbxError::UndefinedSymbol {
                    name: "LIT".to_string(),
                },
            )?;
            vm.dict_write(Cell::Xt(lit_xt))?;
            vm.dict_write(addr_cell)?;
            Ok(())
        }
        _ => Err(TbxError::InvalidExpression {
            reason: "COMPILE_LVALUE: expected variable name or '@' for array element assignment",
        }),
    }
}

/// Compile the lvalue sequence for `LET @A[i] = expr`.
///
/// Called after `@` has already been consumed from the token stream.
///
/// Parses `Ident [ index_expr ]` and emits:
///   `<array handle read>  <compiled index_expr>  ARRAY_ADDR`
///
/// This leaves a `Cell::DictAddr` (element address) on the runtime stack, ready
/// for the subsequent `SET` instruction that the `LET` word in `basic.tbx` appends.
///
/// Error cases — all produce compile-time errors (no panics):
///   - No identifier after `@`        → `InvalidExpression`
///   - No `[` after identifier        → `InvalidExpression` (`LET @A = expr`)
///   - Empty index expression `@A[]`  → `InvalidExpression`
///   - Unterminated `[`               → `InvalidExpression`
///   - Undefined array name           → `UndefinedSymbol`
///   - Name is not an array variable  → `TypeError`
fn compile_at_array_lvalue(vm: &mut VM) -> Result<(), TbxError> {
    use crate::lexer::Token;

    // Expect the array binding identifier (bare name, e.g. `A` for `@A`).
    let array_name_tok = vm.next_token()?;
    let array_name = match array_name_tok.token {
        Token::Ident(n) => n.to_ascii_uppercase(),
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "LET @: expected array binding identifier (e.g. LET @A[i] = expr)",
            });
        }
    };

    // Expect `[`.
    let lb_tok = vm.next_token().map_err(|_| TbxError::InvalidExpression {
        reason: "LET @A: expected '[' after array name (e.g. LET @A[i] = expr)",
    })?;
    if lb_tok.token != Token::LBracket {
        return Err(TbxError::InvalidExpression {
            reason: "LET @A: expected '[' — bare '@A' is not a valid lvalue (use LET @A[i] = expr)",
        });
    }

    // Collect tokens up to the matching `]`, depth-aware.
    let index_tokens: Vec<crate::lexer::SpannedToken> = {
        let stream = vm.token_stream.as_mut().ok_or(TbxError::TokenStreamEmpty)?;
        let mut depth: usize = 1;
        let mut collected: Vec<crate::lexer::SpannedToken> = Vec::new();
        loop {
            let tok = stream.pop_front().ok_or(TbxError::InvalidExpression {
                reason: "LET @A[: unterminated '[' — expected ']'",
            })?;
            match &tok.token {
                Token::LBracket => {
                    depth += 1;
                    collected.push(tok);
                }
                Token::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    collected.push(tok);
                }
                Token::Newline | Token::Eof => {
                    return Err(TbxError::InvalidExpression {
                        reason: "LET @A[: unterminated '[' — unexpected end of line",
                    });
                }
                _ => {
                    collected.push(tok);
                }
            }
        }
        collected
    };

    // Reject empty index: `LET @A[] = expr`.
    if index_tokens.is_empty() {
        return Err(TbxError::InvalidExpression {
            reason: "LET @A[]: array index expression must not be empty (use LET @A[i] = expr)",
        });
    }

    // Compile the index expression using the current local table.
    let (index_cells, index_patch_offsets) = compile_expr_taking_local_table(vm, &index_tokens)?;

    // Resolve the array handle: local binding takes priority over global.
    let local_idx: Option<usize> = vm
        .compile_state
        .as_ref()
        .and_then(|s| s.local_table.get(&array_name).copied());

    let lit_xt =
        vm.find_by_kind(|k| matches!(k, EntryKind::Lit))
            .ok_or(TbxError::UndefinedSymbol {
                name: "LIT".to_string(),
            })?;
    let fetch_xt = vm.lookup("FETCH").ok_or(TbxError::UndefinedSymbol {
        name: "FETCH".to_string(),
    })?;
    let array_addr_xt = vm
        .lookup_hidden_system("ARRAY_ADDR")
        .ok_or(TbxError::UndefinedSymbol {
            name: "ARRAY_ADDR".to_string(),
        })?;

    if let Some(idx) = local_idx {
        // Emit: LIT StackAddr(idx)  FETCH  — load the local array handle.
        vm.dict_write(Cell::Xt(lit_xt))?;
        vm.dict_write(Cell::StackAddr(idx))?;
        vm.dict_write(Cell::Xt(fetch_xt))?;
    } else {
        // Look up in the global dictionary.
        let xt = vm.lookup(&array_name).ok_or(TbxError::UndefinedSymbol {
            name: array_name.clone(),
        })?;
        let addr = match &vm.headers[xt.index()].kind {
            EntryKind::Variable(addr) => *addr,
            _ => {
                return Err(TbxError::TypeError {
                    expected: "array variable for LET @-sigil lvalue",
                    got: "non-variable",
                });
            }
        };
        // Emit: LIT DictAddr(addr)  FETCH  — load the global array handle.
        vm.dict_write(Cell::Xt(lit_xt))?;
        vm.dict_write(Cell::DictAddr(addr))?;
        vm.dict_write(Cell::Xt(fetch_xt))?;
    }

    // Emit the compiled index expression.
    let base_dp = vm.dp;
    for cell in index_cells {
        vm.dict_write(cell)?;
    }
    // Register any self-recursive call patch positions from the index expression.
    if let Some(state) = vm.compile_state.as_mut() {
        for offset in index_patch_offsets {
            state.call_patch_list.push(base_dp + offset);
        }
    }

    // Emit ARRAY_ADDR — pops (Array, index) and pushes the element address.
    vm.dict_write(Cell::Xt(array_addr_xt))?;

    Ok(())
}

/// SKIP_EQ — read the next token from the token stream and validate it is `=`.
///
/// Used by the `LET` compile word to consume the `=` separator between the
/// left-hand variable name and the right-hand expression.
///
/// Must be called in compile mode. Returns `InvalidExpression` if the token is
/// not `Token::Op("=")`.
fn skip_eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "SKIP_EQ outside compile mode",
        });
    }

    vm.expect_op("=", "SKIP_EQ: expected '='")?;
    Ok(())
}

/// LOOKUP — pop a string from the stack, look up the named word, and push its Xt.
///
/// Expects a `Cell::Str` on top of the data stack.
fn lookup_prim(vm: &mut VM) -> Result<(), TbxError> {
    let name_rc = vm.pop_string_value()?;
    let xt = vm.lookup(name_rc.as_ref()).ok_or_else(|| {
        // Materialise an owned `String` for the error payload only on the
        // failure path.
        TbxError::UndefinedSymbol {
            name: name_rc.to_string(),
        }
    })?;
    vm.push(Cell::Xt(xt))
}

/// USE — load and execute a TBX source file at compile time.
///
/// Syntax: `USE "path/to/file.tbx"`
///
/// Reads the next token from the token stream, expecting a `StringLit`.
/// Stores the path in `vm.pending_use_path` so that the outer interpreter
/// (`exec_immediate_word`) can read the file and call `exec_source` after
/// this primitive returns.
/// Returns an error if additional tokens follow the path argument on the
/// same statement, since USE accepts exactly one argument.
/// Returns an error if called inside a DEF body (`is_compiling` is true),
/// because `exec_source` would corrupt the active compile state.
fn use_prim(vm: &mut VM) -> Result<(), TbxError> {
    // Guard: USE inside a DEF body would corrupt compile_state via exec_source.
    if vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "USE cannot be called inside a DEF body",
        });
    }

    let path = vm.expect_string_lit("USE expects a string literal as its argument")?;

    // Reject any extra tokens on the same statement (e.g. USE "f.tbx" EXTRA).
    if let Some(stream) = &vm.token_stream {
        if !stream.is_empty() {
            return Err(TbxError::InvalidExpression {
                reason: "USE does not accept tokens after the path argument",
            });
        }
    }

    vm.pending_use_path = Some(path);
    Ok(())
}

/// Read one line from the VM's input source and return it as a `String`.
///
/// Internal helper used by `getdec_prim`. Reads until a newline (or EOF) and
/// strips the trailing newline characters.
fn accept_prim(vm: &mut VM) -> Result<String, TbxError> {
    // Flush any pending output before blocking on user input, so that prompt
    // strings written with PUTSTR are visible before the interpreter waits.
    if !vm.output_buffer.is_empty() {
        let pending = std::mem::take(&mut vm.output_buffer);
        vm.output_writer
            .write_all(pending.as_bytes())
            .map_err(|e| TbxError::OutputIoError {
                reason: e.to_string(),
            })?;
        vm.output_writer
            .flush()
            .map_err(|e| TbxError::OutputIoError {
                reason: e.to_string(),
            })?;
    }
    let mut line = String::new();
    vm.input_reader
        .read_line(&mut line)
        .map_err(|e| TbxError::InputIoError {
            reason: e.to_string(),
        })?;
    // Strip trailing CR and LF so the stored string never includes line-ending bytes.
    let trimmed = line
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string();
    Ok(trimmed)
}

/// GETDEC — read one line from the input and push its integer value onto the data stack.
///
/// Calls `accept_prim` internally to read a line, then parses the result as a signed
/// decimal integer (leading/trailing whitespace is ignored) and pushes it as `Cell::Int`.
/// No prior `ACCEPT` call is needed.
///
/// Returns `TbxError::ParseIntError` if the input cannot be parsed as a signed decimal
/// integer (including when the input is empty or EOF).
///
/// Stack signature: `( -- n )`
pub fn getdec_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s = accept_prim(vm)?;
    let n = s
        .trim()
        .parse::<i64>()
        .map_err(|_| TbxError::ParseIntError { input: s })?;
    vm.push(Cell::Int(n))
}

/// GETSTR — read one line from the input and push it as a `Cell::Str` onto the data stack.
///
/// Calls `accept_prim` internally to read a line, then wraps the result in
/// an `Rc<str>` and pushes it as `Cell::Str`.  The trailing newline is
/// stripped by `accept_prim`.
///
/// This is the string counterpart of `GETDEC`.  The resulting `Cell::Str` is compatible
/// with all existing string primitives (`PUTSTR`, `STR`, `STR_CONCAT`, `STR_LEN`,
/// `STR_EQ`, etc.) without any additional conversion.
///
/// Stack signature: `( -- s )`
pub fn getstr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s = accept_prim(vm)?;
    vm.push(Cell::string(s))
}

/// RND — generate a random integer in the range [1, n].
///
/// Pops `Cell::Int(n)` from the stack (n > 0) and pushes a random integer in [1, n].
///
/// Stack signature: `( n:Int -- result:Int )`
pub fn rnd_prim(vm: &mut VM) -> Result<(), TbxError> {
    use rand::Rng;
    let n = vm.pop_int()?;
    if n <= 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("RND requires a positive integer, got {n}"),
        });
    }
    let result = vm.rng.gen_range(1..=n);
    vm.push(Cell::Int(result))
}

/// RANDOMIZE — re-seed the RNG from OS entropy.
///
/// Replaces the VM's RNG with a new `SmallRng` seeded from the operating system's
/// entropy source, breaking any previously deterministic sequence.
///
/// Stack signature: `( -- )`
pub fn randomize_prim(vm: &mut VM) -> Result<(), TbxError> {
    use rand::SeedableRng;
    vm.rng = rand::rngs::SmallRng::from_entropy();
    Ok(())
}

/// UNIXTIME — return the current time as seconds since the Unix epoch.
///
/// Uses `std::time::SystemTime` to obtain the current UTC time and returns
/// the elapsed seconds as `f64`, preserving sub-second precision in the
/// fractional part.
///
/// Stack signature: `( -- t:Float )`
pub fn unixtime_prim(vm: &mut VM) -> Result<(), TbxError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    vm.push(Cell::Float(secs))
}

/// HOUR — extract the UTC hour (0–23) from a Unix timestamp.
///
/// Accepts both `Float` and `Int`; promotes `Int` to `f64` for the computation.
/// Returns `InvalidArgument` if `t` is negative.
///
/// Stack signature: `( t:Float -- h:Int )`
pub fn hour_prim(vm: &mut VM) -> Result<(), TbxError> {
    let t = match vm.pop_number()? {
        Cell::Float(f) => f,
        Cell::Int(i) => i as f64,
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    if t < 0.0 {
        return Err(TbxError::InvalidArgument {
            message: "HOUR requires a non-negative timestamp".to_string(),
        });
    }
    let h = (t as i64 / 3600) % 24;
    vm.push(Cell::Int(h))
}

/// MINUTE — extract the UTC minute (0–59) from a Unix timestamp.
///
/// Accepts both `Float` and `Int`; promotes `Int` to `f64` for the computation.
/// Returns `InvalidArgument` if `t` is negative.
///
/// Stack signature: `( t:Float -- m:Int )`
pub fn minute_prim(vm: &mut VM) -> Result<(), TbxError> {
    let t = match vm.pop_number()? {
        Cell::Float(f) => f,
        Cell::Int(i) => i as f64,
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    if t < 0.0 {
        return Err(TbxError::InvalidArgument {
            message: "MINUTE requires a non-negative timestamp".to_string(),
        });
    }
    let m = (t as i64 / 60) % 60;
    vm.push(Cell::Int(m))
}

/// SECOND — extract the UTC second (0.000–59.999) from a Unix timestamp.
///
/// Returns a `Float` that preserves the sub-second fractional part of `t`.
/// Accepts both `Float` and `Int`; promotes `Int` to `f64` for the computation.
/// Returns `InvalidArgument` if `t` is negative.
///
/// Stack signature: `( t:Float -- s:Float )`
pub fn second_prim(vm: &mut VM) -> Result<(), TbxError> {
    let t = match vm.pop_number()? {
        Cell::Float(f) => f,
        Cell::Int(i) => i as f64,
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    if t < 0.0 {
        return Err(TbxError::InvalidArgument {
            message: "SECOND requires a non-negative timestamp".to_string(),
        });
    }
    let s = (t as i64 % 60) as f64 + t.fract();
    vm.push(Cell::Float(s))
}

/// Register all stack primitives into the VM's dictionary.
pub fn register_all(vm: &mut VM) {
    vm.register(WordEntry::new_primitive("DROP", drop_prim));
    vm.register(WordEntry::new_primitive("DUP", dup_prim));
    vm.register(WordEntry::new_primitive("SWAP", swap_prim));
    vm.register(WordEntry::new_primitive("FETCH", fetch_prim));
    vm.register(WordEntry::new_primitive("STORE", store_prim));
    vm.register(WordEntry::new_primitive("SET", set_prim));
    vm.register(WordEntry::new_primitive("ADD", add_prim));
    vm.register(WordEntry::new_primitive("SUB", sub_prim));
    vm.register(WordEntry::new_primitive("MUL", mul_prim));
    vm.register(WordEntry::new_primitive("DIV", div_prim));
    vm.register(WordEntry::new_primitive("MOD", mod_prim));
    vm.register(WordEntry::new_primitive("SQRT", sqrt_prim));
    vm.register(WordEntry::new_primitive("EQ", eq_prim));
    vm.register(WordEntry::new_primitive("NEQ", neq_prim));
    vm.register(WordEntry::new_primitive("LT", lt_prim));
    vm.register(WordEntry::new_primitive("GT", gt_prim));
    vm.register(WordEntry::new_primitive("LE", le_prim));
    vm.register(WordEntry::new_primitive("GE", ge_prim));
    vm.register(WordEntry::new_primitive("AND", and_prim));
    vm.register(WordEntry::new_primitive("OR", or_prim));
    vm.register(WordEntry::new_primitive("BAND", band_prim));
    vm.register(WordEntry::new_primitive("BOR", bor_prim));
    vm.register(WordEntry::new_primitive("NEGATE", negate_prim));
    vm.register(WordEntry::new_primitive("INT", int_prim));
    vm.register(WordEntry::new_primitive("PUTSTR", putstr_prim));
    vm.register(WordEntry::new_primitive("GET_OUTPUT", get_output_prim));
    // Runtime string primitives.
    // STR converts any value to a string; STR_CONCAT concatenates two strings;
    // STR_LEN returns the character count; STR_EQ compares by content;
    // STR_INDEXOF, STR_SLICE, STR_TRIM, STR_UPPER, STR_LOWER,
    // STR_REPLACE_FIRST, and STR_REPLACE_ALL provide core string manipulation.
    vm.register(WordEntry::new_primitive("STR", str_prim));
    vm.register(WordEntry::new_primitive("STR_CONCAT", str_concat_prim));
    vm.register(WordEntry::new_primitive("STR_LEN", str_len_prim));
    vm.register(WordEntry::new_primitive("STR_EQ", str_eq_prim));
    vm.register(WordEntry::new_primitive("STR_INDEXOF", str_indexof_prim));
    vm.register(WordEntry::new_primitive("STR_SLICE", str_slice_prim));
    vm.register(WordEntry::new_primitive("STR_TRIM", str_trim_prim));
    vm.register(WordEntry::new_primitive("STR_UPPER", str_upper_prim));
    vm.register(WordEntry::new_primitive("STR_LOWER", str_lower_prim));
    vm.register(WordEntry::new_primitive(
        "STR_REPLACE_FIRST",
        str_replace_first_prim,
    ));
    vm.register(WordEntry::new_primitive(
        "STR_REPLACE_ALL",
        str_replace_all_prim,
    ));
    vm.register(WordEntry::new_primitive("PUTCHR", putchr_prim));
    vm.register(WordEntry::new_primitive("PUTDEC", putdec_prim));
    vm.register(WordEntry::new_primitive("PUTHEX", puthex_prim));
    vm.register(WordEntry::new_primitive("PUTVAL", putval_prim));
    vm.register(WordEntry::new_primitive("GETDEC", getdec_prim));
    vm.register(WordEntry::new_primitive("GETSTR", getstr_prim));
    vm.register(WordEntry::new_primitive("APPEND", append_prim));
    vm.register(WordEntry::new_primitive("ALLOT", allot_prim));
    vm.register(WordEntry::new_primitive("HERE", here_prim));
    vm.register(WordEntry::new_primitive("STATE", state_prim));
    vm.register(WordEntry::new_primitive("HALT", halt_prim));
    vm.register(WordEntry::new_primitive("ASSERT_FAIL", assert_fail_prim));
    vm.register(WordEntry::new_primitive(
        "ASSERT_FAIL_MSG",
        assert_fail_msg_prim,
    ));
    vm.register(WordEntry {
        name: "CALL".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Call,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    vm.register(WordEntry {
        name: "EXIT".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Exit,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    vm.register(WordEntry {
        name: "RETURN_VAL".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::ReturnVal,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    vm.register(WordEntry {
        name: "DROP_TO_MARKER".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::DropToMarker,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    vm.register(WordEntry {
        name: "GOTO".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Goto,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    vm.register(WordEntry {
        name: "BIF".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::BranchIfFalse,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    vm.register(WordEntry {
        name: "BIT".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::BranchIfTrue,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    let mut lit_marker_entry = WordEntry::new_primitive("LIT_MARKER", lit_marker_prim);
    lit_marker_entry.flags |= FLAG_SYSTEM;
    vm.register(lit_marker_entry);
    vm.register(WordEntry {
        name: "LIT".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Lit,
        arity: 0,
        local_count: 0,
        is_variadic: false,
        prev: None,
    });
    // LITERAL: compile-time primitive that emits `LIT <value>` to the dictionary.
    // Not IMMEDIATE — it must not be caught by the interpreter's IMMEDIATE dispatch,
    // because it reads its argument from the data stack (not from the token stream).
    // No FLAG_SYSTEM — LITERAL is part of the IMMEDIATE-word authoring API, callable
    // as a statement inside DEF bodies (e.g. `LITERAL CS_POP`), just like CS_PUSH/CS_POP.
    vm.register(WordEntry::new_primitive("LITERAL", literal_prim));
    // HEADER: IMMEDIATE so the outer interpreter feeds the token stream before calling it.
    // Also FLAG_SYSTEM to mark it as a system word consistent with other compile-time words.
    let mut header_entry = WordEntry::new_primitive("HEADER", header_prim);
    header_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(header_entry);
    // IMMEDIATE: reads next token and sets FLAG_IMMEDIATE on the named word.
    let mut immediate_entry = WordEntry::new_primitive("IMMEDIATE", immediate_prim);
    immediate_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(immediate_entry);
    // IMMEDIATE system words: DEF, END, VAR, GOTO, BIF, BIT, RETURN
    let mut def_entry = WordEntry::new_primitive("DEF", def_prim);
    def_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(def_entry);
    let mut end_entry = WordEntry::new_primitive("END", end_prim);
    end_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(end_entry);
    let mut var_entry = WordEntry::new_primitive("VAR", var_prim);
    var_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(var_entry);
    let mut dim_entry = WordEntry::new_primitive("DIM", dim_prim);
    dim_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(dim_entry);
    let mut goto_entry = WordEntry::new_primitive("GOTO", goto_prim);
    goto_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(goto_entry);
    let mut bif_entry = WordEntry::new_primitive("BIF", bif_prim);
    bif_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(bif_entry);
    let mut bit_entry = WordEntry::new_primitive("BIT", bit_prim);
    bit_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(bit_entry);
    let mut return_entry = WordEntry::new_primitive("RETURN", return_prim);
    return_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(return_entry);
    // Compile-stack primitives for IMMEDIATE word authoring.
    // No FLAG_IMMEDIATE or FLAG_SYSTEM: these are compiled into DEF bodies as statements
    // and called at runtime by IMMEDIATE words (e.g. IF/ENDIF, WHILE/ENDWH).
    vm.register(WordEntry::new_primitive("CS_PUSH", cs_push_prim));
    vm.register(WordEntry::new_primitive("CS_POP", cs_pop_prim));
    vm.register(WordEntry::new_primitive("CS_SWAP", cs_swap_prim));
    vm.register(WordEntry::new_primitive("CS_DROP", cs_drop_prim));
    vm.register(WordEntry::new_primitive("CS_DUP", cs_dup_prim));
    vm.register(WordEntry::new_primitive("CS_OVER", cs_over_prim));
    vm.register(WordEntry::new_primitive("CS_ROT", cs_rot_prim));
    vm.register(WordEntry::new_primitive("PATCH_ADDR", patch_addr_prim));
    vm.register(WordEntry::new_primitive("COMPILE_EXPR", compile_expr_prim));
    // Tag-based control-structure scope primitives.
    // CS_OPEN_TAG pushes a string tag onto the compile stack to mark the start of a
    // control-structure scope; CS_CLOSE_TAG validates and pops the matching tag.
    vm.register(WordEntry::new_primitive("CS_OPEN_TAG", cs_open_tag_prim));
    vm.register(WordEntry::new_primitive("CS_CLOSE_TAG", cs_close_tag_prim));

    // Runtime branch/jump Xt constants — allows TBX code to write:
    //   APPEND JUMP_FALSE, APPEND JUMP_ALWAYS, etc.
    let bif_xt = vm
        .find_by_kind(|k| matches!(k, EntryKind::BranchIfFalse))
        .expect("BIF runtime entry must exist");
    vm.register(WordEntry::new_constant("JUMP_FALSE", Cell::Xt(bif_xt)));

    let bit_xt = vm
        .find_by_kind(|k| matches!(k, EntryKind::BranchIfTrue))
        .expect("BIT runtime entry must exist");
    vm.register(WordEntry::new_constant("JUMP_TRUE", Cell::Xt(bit_xt)));

    let goto_xt = vm
        .find_by_kind(|k| matches!(k, EntryKind::Goto))
        .expect("GOTO runtime entry must exist");
    vm.register(WordEntry::new_constant("JUMP_ALWAYS", Cell::Xt(goto_xt)));

    // USE: IMMEDIATE so the outer interpreter feeds the token stream before calling it.
    // No FLAG_SYSTEM: USE is user-redefinable.
    let mut use_entry = WordEntry::new_primitive("USE", use_prim);
    use_entry.flags = FLAG_IMMEDIATE;
    vm.register(use_entry);

    // COMPILE_LVALUE / SKIP_EQ: compile-helper primitives for LET and similar
    // compile words. No IMMEDIATE/SYSTEM — called as statements inside IMMEDIATE
    // word bodies, exactly like COMPILE_EXPR, CS_PUSH, PATCH_ADDR, etc.
    vm.register(WordEntry::new_primitive(
        "COMPILE_LVALUE",
        compile_lvalue_prim,
    ));
    vm.register(WordEntry::new_primitive("SKIP_EQ", skip_eq_prim));

    // LOOKUP: look up a word by name string and push its Xt.
    // Replaces the xxx_XT constant pattern: `APPEND LOOKUP("SET")` instead of `APPEND ASSIGN_XT`.
    vm.register(WordEntry::new_primitive("LOOKUP", lookup_prim));

    // FOR/NEXT compile-helper primitives.
    // These are used inside IMMEDIATE word bodies (FOR, NEXT) defined in basic.tbx.
    vm.register(WordEntry::new_primitive("SKIP_COMMA", skip_comma_prim));
    vm.register(WordEntry::new_primitive(
        "COMPILE_LVALUE_SAVE",
        compile_lvalue_save_prim,
    ));

    // Array primitives.
    // ARRAY is a hidden system entry used internally by the DIM @A[n] compiler.
    // It is NOT a user-facing surface primitive; user code uses DIM @A[n] instead.
    // dim_prim looks this up via vm.lookup_hidden_system("ARRAY") to emit its Xt
    // into the compiled word body.
    let mut array_entry = WordEntry::new_primitive("ARRAY", array_prim);
    array_entry.flags = FLAG_SYSTEM | FLAG_HIDDEN;
    vm.register(array_entry);
    // ARRAY_STORE_LOCAL is a hidden system entry used exclusively by the DIM @A[n]
    // compiler to write a Cell::Array handle into a local stack-frame slot.
    // It bypasses the Cell::Array rejection guard in SET / STORE, which is intentional:
    // only compiler-generated DIM code may write array handles to local slots.
    let mut array_store_local_entry =
        WordEntry::new_primitive("ARRAY_STORE_LOCAL", arrays::array_store_local_prim);
    array_store_local_entry.flags = FLAG_SYSTEM | FLAG_HIDDEN;
    vm.register(array_store_local_entry);
    // ARRAY_GET reads an element (`@A[i]` compiles to
    // `<array handle read> <index expr> ARRAY_GET`); ARRAY_ADDR computes an element
    // address (`&@A[i]` compiles to `<array handle read> <index expr> ARRAY_ADDR`);
    // ARRAY_LEN returns the length of an array (`ARRAY_LEN(@A)` compiles to
    // `<array handle read> ARRAY_LEN`).
    // All three are hidden system helpers: they cannot be called from user code
    // directly; the compiler accesses them via lookup_hidden_system().
    // TUPLE packs stack values into a new immutable Cell::Tuple.
    let mut tuple_entry = WordEntry::new_primitive("TUPLE", to_tuple_prim);
    tuple_entry.is_variadic = true;
    // arity stays 0: TUPLE accepts zero or more arguments (empty tuple is allowed).
    vm.register(tuple_entry);
    let mut array_len_entry = WordEntry::new_primitive("ARRAY_LEN", array_len_prim);
    array_len_entry.flags = FLAG_SYSTEM | FLAG_HIDDEN;
    vm.register(array_len_entry);
    vm.register(WordEntry::new_primitive("TUPLE_LEN", tuple_len_prim));
    let mut array_get_entry = WordEntry::new_primitive("ARRAY_GET", array_get_prim);
    array_get_entry.flags = FLAG_SYSTEM | FLAG_HIDDEN;
    vm.register(array_get_entry);
    let mut array_addr_entry = WordEntry::new_primitive("ARRAY_ADDR", array_addr_prim);
    array_addr_entry.flags = FLAG_SYSTEM | FLAG_HIDDEN;
    vm.register(array_addr_entry);
    let mut tuple_get_entry = WordEntry::new_primitive("TUPLE_GET", tuple_get_prim);
    tuple_get_entry.flags = FLAG_SYSTEM;
    vm.register(tuple_get_entry);

    // Variadic argument primitives.
    // VA_COUNT returns the total argument count of the current call.
    // ARG_ADDR converts an argument index to a StackAddr for FETCH/STORE.
    vm.register(WordEntry::new_primitive("VA_COUNT", va_count_prim));
    vm.register(WordEntry::new_primitive("ARG_ADDR", arg_addr_prim));

    // Random number primitives.
    // RND(n) returns a random integer in [1, n]; RANDOMIZE re-seeds the RNG from OS entropy.
    vm.register(WordEntry::new_primitive("RND", rnd_prim));
    vm.register(WordEntry::new_primitive("RANDOMIZE", randomize_prim));

    // Time primitives.
    // UNIXTIME returns the current Unix timestamp as a Float (seconds since epoch).
    // HOUR / MINUTE / SECOND extract UTC hour, minute, and second from a timestamp.
    vm.register(WordEntry::new_primitive("UNIXTIME", unixtime_prim));
    vm.register(WordEntry::new_primitive("HOUR", hour_prim));
    vm.register(WordEntry::new_primitive("MINUTE", minute_prim));
    vm.register(WordEntry::new_primitive("SECOND", second_prim));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CompileEntry};
    use crate::constants::MAX_DICTIONARY_CELLS;

    // --- drop_prim ---

    #[test]
    fn test_drop_removes_top() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        drop_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_drop_underflow() {
        let mut vm = VM::new();
        assert_eq!(drop_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- dup_prim ---

    #[test]
    fn test_dup_duplicates_top() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        dup_prim(&mut vm).unwrap();
        // Both copies must be on the stack; the original is below.
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_dup_underflow() {
        let mut vm = VM::new();
        assert_eq!(dup_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- swap_prim ---

    #[test]
    fn test_swap_exchanges_top_two() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        swap_prim(&mut vm).unwrap();
        // After swap: 1 is on top, 2 is below.
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
        assert_eq!(vm.pop(), Ok(Cell::Int(2)));
    }

    #[test]
    fn test_swap_underflow_empty() {
        let mut vm = VM::new();
        assert_eq!(swap_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_swap_underflow_one_element() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        assert_eq!(swap_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- register_all ---

    #[test]
    fn test_register_all_registers_drop_dup_swap() {
        let mut vm = VM::new();
        register_all(&mut vm);
        assert!(vm.lookup("DROP").is_some());
        assert!(vm.lookup("DUP").is_some());
        assert!(vm.lookup("SWAP").is_some());
    }

    #[test]
    fn test_register_all_drop_callable_via_inner_interpreter() {
        // Verify that the registered DROP word can be invoked through the inner interpreter.
        let mut vm = VM::new();
        register_all(&mut vm);
        let drop_xt = vm.lookup("DROP").unwrap();
        let exit_xt = vm.lookup("EXIT").unwrap();

        // Write a tiny program: [Xt(DROP), Xt(EXIT)]
        let start = vm.dp;
        vm.dict_write(Cell::Xt(drop_xt)).unwrap();
        vm.dict_write(Cell::Xt(exit_xt)).unwrap();

        vm.push(Cell::Int(99)).unwrap();
        vm.run(start).unwrap();

        // DROP must have consumed the only stack element.
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_fetch_dict_addr() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::Int(123)); // dict[0] = 123
        vm.push(Cell::DictAddr(0)).unwrap();
        fetch_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(123)));
    }

    #[test]
    fn test_fetch_stack_addr() {
        // This test also verifies that fetch_prim correctly adds vm.bp to the address.
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap(); // data_stack[0] = 10
        vm.push(Cell::Int(20)).unwrap(); // data_stack[1] = 20
        vm.bp = 1; // base pointer at index 1
        vm.push(Cell::StackAddr(0)).unwrap(); // address of data_stack[bp + 0] = data_stack[1] = 20
        fetch_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_fetch_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap(); // Not an address
        assert_eq!(
            fetch_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "address",
                got: "non-address"
            })
        );
    }

    #[test]
    fn test_fetch_underflow() {
        let mut vm = VM::new();
        assert_eq!(fetch_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_store_dict_addr() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::Int(0)); // dict[0] = 0
        vm.push(Cell::Int(123)).unwrap(); // value to store
        vm.push(Cell::DictAddr(0)).unwrap(); // address to store at
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.dictionary[0], Cell::Int(123));
    }

    #[test]
    fn test_store_stack_addr() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap(); // data_stack[0] = 0
        vm.bp = 0;
        vm.push(Cell::Int(123)).unwrap(); // value to store
        vm.push(Cell::StackAddr(0)).unwrap(); // address to store at
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.data_stack[0], Cell::Int(123));
    }

    #[test]
    fn test_store_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(123)).unwrap(); // value to store
        vm.push(Cell::Int(0)).unwrap(); // Not an address
        assert_eq!(
            store_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "address",
                got: "non-address"
            })
        );
    }

    #[test]
    fn test_store_underflow() {
        let mut vm = VM::new();
        assert_eq!(store_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_store_underflow_one_value() {
        let mut vm = VM::new();
        vm.push(Cell::Int(123)).unwrap(); // value to store
        assert_eq!(store_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_add_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(5)));
    }

    #[test]
    fn test_add_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        vm.push(Cell::Float(3.5)).unwrap();
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(6.0)));
    }

    #[test]
    fn test_add_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Float(3.5)).unwrap();
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(5.5)));
    }

    #[test]
    fn test_add_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        add_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(5.5)));
    }

    #[test]
    fn test_add_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Bool(true)).unwrap(); // Not a number
        assert!(matches!(
            add_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "number",
                ..
            })
        ));
    }

    #[test]
    fn test_add_overflow() {
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MAX)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert_eq!(add_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    #[test]
    fn test_add_overflow_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MIN)).unwrap();
        vm.push(Cell::Int(-1)).unwrap();
        assert_eq!(add_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    // --- sub_prim ---

    #[test]
    fn test_sub_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        sub_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(7)));
    }

    #[test]
    fn test_sub_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(5.5)).unwrap();
        vm.push(Cell::Float(2.0)).unwrap();
        sub_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_sub_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        vm.push(Cell::Float(1.5)).unwrap();
        sub_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_sub_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(sub_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_sub_overflow() {
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MIN)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert_eq!(sub_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    #[test]
    fn test_sub_overflow_positive() {
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MAX)).unwrap();
        vm.push(Cell::Int(-1)).unwrap();
        assert_eq!(sub_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    // --- mul_prim ---

    #[test]
    fn test_mul_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(4)).unwrap();
        vm.push(Cell::Int(5)).unwrap();
        mul_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_mul_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        vm.push(Cell::Float(4.0)).unwrap();
        mul_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(10.0)));
    }

    #[test]
    fn test_mul_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        vm.push(Cell::Int(4)).unwrap();
        mul_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(10.0)));
    }

    #[test]
    fn test_mul_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(mul_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_mul_overflow() {
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MAX)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        assert_eq!(mul_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    #[test]
    fn test_mul_overflow_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MIN)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        assert_eq!(mul_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    // --- div_prim ---

    #[test]
    fn test_div_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(3))); // truncation toward zero
    }

    #[test]
    fn test_div_int_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-3))); // truncation toward zero
    }

    #[test]
    fn test_div_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(7.0)).unwrap();
        vm.push(Cell::Float(2.0)).unwrap();
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_div_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7)).unwrap();
        vm.push(Cell::Float(2.0)).unwrap();
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_div_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(7.0)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        div_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(3.5)));
    }

    #[test]
    fn test_div_by_zero_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_by_zero_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(5.0)).unwrap();
        vm.push(Cell::Float(0.0)).unwrap();
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_by_zero_int_float() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        vm.push(Cell::Float(0.0)).unwrap();
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_by_zero_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(5.0)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert_eq!(div_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_div_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(div_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_div_overflow() {
        // i64::MIN / -1 overflows because the result (i64::MAX + 1) is out of range.
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MIN)).unwrap();
        vm.push(Cell::Int(-1)).unwrap();
        assert_eq!(div_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    // --- mod_prim ---

    #[test]
    fn test_mod_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        mod_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_mod_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        mod_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-1))); // truncation toward zero
    }

    #[test]
    fn test_mod_by_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert_eq!(mod_prim(&mut vm), Err(TbxError::DivisionByZero));
    }

    #[test]
    fn test_mod_float_rejected() {
        let mut vm = VM::new();
        vm.push(Cell::Float(7.0)).unwrap();
        vm.push(Cell::Float(3.0)).unwrap();
        assert!(matches!(mod_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_mod_int_float_rejected() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7)).unwrap();
        vm.push(Cell::Float(3.0)).unwrap();
        assert!(matches!(mod_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_mod_overflow() {
        // i64::MIN % -1 overflows for the same reason as i64::MIN / -1.
        let mut vm = VM::new();
        vm.push(Cell::Int(i64::MIN)).unwrap();
        vm.push(Cell::Int(-1)).unwrap();
        assert_eq!(mod_prim(&mut vm), Err(TbxError::IntegerOverflow));
    }

    // --- SQRT tests ---

    #[test]
    fn test_sqrt_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(7)).unwrap();
        sqrt_prim(&mut vm).unwrap();
        let expected = (7.0f64).sqrt();
        assert_eq!(vm.pop(), Ok(Cell::Float(expected)));
    }

    #[test]
    fn test_sqrt_float() {
        let float_num = 1.23f64;
        let mut vm = VM::new();
        vm.push(Cell::Float(float_num)).unwrap();
        sqrt_prim(&mut vm).unwrap();
        let expected = float_num.sqrt();
        assert_eq!(vm.pop(), Ok(Cell::Float(expected)));
    }

    #[test]
    fn test_sqrt_negative_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7)).unwrap();
        assert!(matches!(
            sqrt_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_sqrt_negative_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(-7.0)).unwrap();
        assert!(matches!(
            sqrt_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_sqrt_nan() {
        let mut vm = VM::new();
        vm.push(Cell::Float(f64::NAN)).unwrap();
        assert!(matches!(
            sqrt_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_sqrt_infinity() {
        let mut vm = VM::new();
        vm.push(Cell::Float(f64::INFINITY)).unwrap();
        assert!(matches!(
            sqrt_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_sqrt_negative_zero() {
        // -0.0 should be normalized to +0.0, yielding 0.0
        let mut vm = VM::new();
        vm.push(Cell::Float(-0.0f64)).unwrap();
        sqrt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(0.0)));
    }

    #[test]
    fn test_sqrt_type_error() {
        // Non-numeric type should produce a type error
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(sqrt_prim(&mut vm).is_err());
    }

    // --- EQ / NEQ tests ---

    #[test]
    fn test_eq_int_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        vm.push(Cell::Int(42)).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_int_not_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_eq_different_types() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Bool(true)).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_eq_int_float_promotion() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Float(1.0)).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_float_int_promotion() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.0)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_int_float_not_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Float(1.5)).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_eq_str_compares_content() {
        let mut vm = VM::new();
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::string("hello")).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_eq_str_different_handles_same_content_is_true() {
        // Two distinct Cell::Str handles holding identical content compare equal.
        let mut vm = VM::new();
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::string("hello")).unwrap();
        eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_neq_int_float_promotion() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Float(1.0)).unwrap();
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_neq_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_neq_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        vm.push(Cell::Int(5)).unwrap();
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_neq_str_compares_content() {
        let mut vm = VM::new();
        vm.push(Cell::string("foo")).unwrap();
        vm.push(Cell::string("bar")).unwrap();
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_neq_str_different_handles_different_content_is_true() {
        // Two distinct Cell::Str handles with different content compare not-equal.
        let mut vm = VM::new();
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::string("world")).unwrap();
        neq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    // --- LT / GT / LE / GE tests ---

    #[test]
    fn test_lt_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        lt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_lt_int_false() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        lt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_lt_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.5)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        lt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_lt_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(lt_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_gt_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        gt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_gt_int_false() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        gt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_gt_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(3.5)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        gt_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_gt_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(gt_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_le_int_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_le_int_less() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_le_int_greater() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_le_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.5)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        le_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_le_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(le_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_ge_int_equal() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_ge_int_greater() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_ge_int_less() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_ge_float_int() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.0)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        ge_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_ge_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(ge_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    // --- AND / OR tests ---

    #[test]
    fn test_and_true_true() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Bool(true)).unwrap();
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_and_true_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Bool(false)).unwrap();
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_and_int_truthy() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_and_int_zero_falsy() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        and_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_or_false_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false)).unwrap();
        vm.push(Cell::Bool(false)).unwrap();
        or_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(false)));
    }

    #[test]
    fn test_or_true_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Bool(false)).unwrap();
        or_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    #[test]
    fn test_or_int_zero_and_nonzero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        vm.push(Cell::Int(5)).unwrap();
        or_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Bool(true)));
    }

    // --- BAND / BOR tests ---

    #[test]
    fn test_band_basic() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        band_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_band_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        vm.push(Cell::Int(5)).unwrap();
        band_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(0)));
    }

    #[test]
    fn test_band_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(
            band_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_band_type_error_top() {
        // b (stack top) is non-Int; first pop should fail with TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            band_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_band_underflow() {
        let mut vm = VM::new();
        assert_eq!(band_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_bor_basic() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        bor_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(3)));
    }

    #[test]
    fn test_bor_same() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        vm.push(Cell::Int(5)).unwrap();
        bor_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(5)));
    }

    #[test]
    fn test_bor_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(bor_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_bor_type_error_top() {
        // b (stack top) is non-Int; first pop should fail with TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Bool(false)).unwrap();
        assert!(matches!(bor_prim(&mut vm), Err(TbxError::TypeError { .. })));
    }

    #[test]
    fn test_bor_underflow() {
        let mut vm = VM::new();
        assert_eq!(bor_prim(&mut vm), Err(TbxError::StackUnderflow));
    }
    // --- positive Cell::Str dict store (replaces legacy StringFrameEscape tests) ---

    #[test]
    fn test_str_stored_via_store_to_dict_succeeds() {
        // With `Cell::Str(Rc<str>)`, dict store no longer depends on the legacy
        // string-pool lifetime classification.  Both frame-local- and top-level-
        // originated strings are safe to store in a dict slot, so the previous
        // `StringFrameEscape` distinction is gone.
        let mut vm = VM::new();
        vm.dictionary.push(Cell::None);
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::DictAddr(0)).unwrap();
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.dict_read(0).unwrap(), Cell::string("hello"));
    }

    // --- PUTCHR tests ---

    #[test]
    fn test_putchr_basic() {
        let mut vm = VM::new();
        vm.push(Cell::Int(65)).unwrap(); // 'A'
        putchr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "A");
    }

    #[test]
    fn test_putchr_newline() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap(); // '\n'
        putchr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "\n");
    }

    #[test]
    fn test_putchr_out_of_range() {
        let mut vm = VM::new();
        vm.push(Cell::Int(128)).unwrap();
        assert!(matches!(
            putchr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putchr_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap();
        assert!(matches!(
            putchr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putchr_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            putchr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- PUTDEC tests ---

    #[test]
    fn test_putdec_positive() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "42");
    }

    #[test]
    fn test_putdec_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-7)).unwrap();
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "-7");
    }

    #[test]
    fn test_putdec_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "0");
    }

    #[test]
    fn test_putdec_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.0)).unwrap();
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "1.0");
    }

    #[test]
    fn test_putdec_float_fractional() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        putdec_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "2.5");
    }

    #[test]
    fn test_putdec_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            putdec_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- PUTHEX tests ---

    #[test]
    fn test_puthex_positive() {
        let mut vm = VM::new();
        vm.push(Cell::Int(255)).unwrap();
        puthex_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "$FF");
    }

    #[test]
    fn test_puthex_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        puthex_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "$0");
    }

    #[test]
    fn test_puthex_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap();
        puthex_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "$FFFFFFFFFFFFFFFF");
    }

    #[test]
    fn test_puthex_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            puthex_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- PUTVAL tests ---

    #[test]
    fn test_putval_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        putval_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "42");
    }

    #[test]
    fn test_putval_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.0)).unwrap();
        putval_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "1.0");
    }

    #[test]
    fn test_putval_float_fractional() {
        let mut vm = VM::new();
        vm.push(Cell::Float(2.5)).unwrap();
        putval_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "2.5");
    }

    #[test]
    fn test_putval_bool_true() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        putval_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "TRUE");
    }

    #[test]
    fn test_putval_bool_false() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false)).unwrap();
        putval_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "FALSE");
    }

    #[test]
    fn test_putval_str() {
        let mut vm = VM::new();
        vm.push(Cell::string("world")).unwrap();
        putval_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "world");
    }

    #[test]
    fn test_putval_none_error() {
        let mut vm = VM::new();
        vm.push(Cell::None).unwrap();
        assert!(matches!(
            putval_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putval_array_error() {
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None; 3]));
        vm.push(Cell::Array(0)).unwrap();
        assert!(matches!(
            putval_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- append_prim ---

    #[test]
    fn test_append_writes_to_dictionary() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        append_prim(&mut vm).unwrap();
        assert_eq!(vm.dictionary.len(), 1);
        assert_eq!(vm.dp, 1);
        assert!(matches!(vm.dictionary[0], Cell::Int(42)));
    }

    #[test]
    fn test_append_xt_value() {
        let mut vm = VM::new();
        vm.push(Cell::Xt(crate::cell::Xt(5))).unwrap();
        append_prim(&mut vm).unwrap();
        assert_eq!(vm.dictionary.len(), 1);
        assert!(matches!(vm.dictionary[0], Cell::Xt(_)));
    }

    #[test]
    fn test_append_multiple() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(20)).unwrap();
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(30)).unwrap();
        append_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 3);
        assert!(matches!(vm.dictionary[0], Cell::Int(10)));
        assert!(matches!(vm.dictionary[1], Cell::Int(20)));
        assert!(matches!(vm.dictionary[2], Cell::Int(30)));
    }

    #[test]
    fn test_append_empty_stack() {
        let mut vm = VM::new();
        assert!(matches!(
            append_prim(&mut vm),
            Err(TbxError::StackUnderflow)
        ));
    }

    #[test]
    fn test_append_overflow() {
        let mut vm = VM::new();
        vm.dp = MAX_DICTIONARY_CELLS;
        // Manually grow dictionary to match dp invariant
        vm.dictionary.resize(MAX_DICTIONARY_CELLS, Cell::None);
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(
            append_prim(&mut vm),
            Err(TbxError::DictionaryOverflow { .. })
        ));
    }

    // --- allot_prim ---

    #[test]
    fn test_allot_reserves_cells() {
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        allot_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 5);
        assert_eq!(vm.dictionary.len(), 5);
        // Returns start address
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(0));
    }

    #[test]
    fn test_allot_after_append() {
        let mut vm = VM::new();
        vm.push(Cell::Int(100)).unwrap();
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        allot_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 4);
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(1));
    }

    #[test]
    fn test_allot_zero() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        allot_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, 0);
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(0));
    }

    #[test]
    fn test_allot_negative() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap();
        assert!(matches!(
            allot_prim(&mut vm),
            Err(TbxError::InvalidAllotCount)
        ));
    }

    #[test]
    fn test_allot_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            allot_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- here_prim ---

    #[test]
    fn test_here_initial() {
        let mut vm = VM::new();
        here_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(0));
    }

    #[test]
    fn test_here_after_append() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        append_prim(&mut vm).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        append_prim(&mut vm).unwrap();
        here_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::DictAddr(2));
    }

    // --- dict_write overflow ---

    #[test]
    fn test_allot_overflow() {
        let mut vm = VM::new();
        vm.push(Cell::Int((MAX_DICTIONARY_CELLS + 1) as i64))
            .unwrap();
        assert!(matches!(
            allot_prim(&mut vm),
            Err(TbxError::DictionaryOverflow { .. })
        ));
    }

    // --- state_prim ---

    #[test]
    fn test_state_execute_mode() {
        let mut vm = VM::new();
        state_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(0));
    }

    #[test]
    fn test_state_compile_mode() {
        let mut vm = VM::new();
        vm.is_compiling = true;
        state_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(1));
    }

    // --- halt_prim ---

    #[test]
    fn test_halt_returns_halted() {
        let mut vm = VM::new();
        assert!(matches!(halt_prim(&mut vm), Err(TbxError::Halted)));
    }

    #[test]
    fn test_halt_leaves_stack_unchanged() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        let _ = halt_prim(&mut vm);
        assert_eq!(vm.data_stack.len(), 1);
        assert_eq!(vm.pop().unwrap(), Cell::Int(42));
    }

    // --- assert_fail_prim ---

    #[test]
    fn test_assert_fail_returns_assertion_failed() {
        let mut vm = VM::new();
        assert!(matches!(
            assert_fail_prim(&mut vm),
            Err(TbxError::AssertionFailed)
        ));
    }

    #[test]
    fn test_assert_fail_leaves_stack_unchanged() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        let _ = assert_fail_prim(&mut vm);
        assert_eq!(vm.data_stack.len(), 1);
        assert_eq!(vm.pop().unwrap(), Cell::Int(1));
    }

    // --- assert_fail_msg_prim ---

    #[test]
    fn test_assert_fail_msg_returns_assertion_failed_with_message() {
        let mut vm = VM::new();
        vm.push(Cell::string("SIGN(7) should be 1")).unwrap();
        let result = assert_fail_msg_prim(&mut vm);
        assert!(matches!(
            result,
            Err(TbxError::AssertionFailedWithMessage { .. })
        ));
        if let Err(TbxError::AssertionFailedWithMessage { message }) = result {
            assert_eq!(message, "SIGN(7) should be 1");
        }
    }

    #[test]
    fn test_assert_fail_msg_pops_message_from_stack() {
        let mut vm = VM::new();
        vm.push(Cell::string("msg")).unwrap();
        let _ = assert_fail_msg_prim(&mut vm);
        assert_eq!(vm.data_stack.len(), 0);
    }

    #[test]
    fn test_literal_compiles_lit_and_value() {
        // LITERAL should write [Xt(LIT), value] into the dictionary.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);

        let lit_xt = vm.lookup("LIT").unwrap();
        let dp_before = vm.dp;

        vm.push(Cell::Int(123)).unwrap();
        crate::primitives::literal_prim(&mut vm).unwrap();

        assert_eq!(vm.dictionary[dp_before], Cell::Xt(lit_xt));
        assert_eq!(vm.dictionary[dp_before + 1], Cell::Int(123));
        assert_eq!(vm.dp, dp_before + 2);
    }

    #[test]
    fn test_literal_prim_is_not_immediate() {
        // LITERAL must NOT have FLAG_IMMEDIATE; it is a system-internal compile-time primitive
        // that must not be caught by the interpreter's IMMEDIATE dispatch.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);
        let xt = vm.lookup("LITERAL").unwrap();
        assert!(!vm.headers[xt.index()].is_immediate());
    }

    // --- header_prim ---

    fn make_ident_token(name: &str) -> crate::lexer::SpannedToken {
        crate::lexer::SpannedToken {
            token: crate::lexer::Token::Ident(name.to_string()),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: name.len(),
        }
    }

    #[test]
    fn test_header_prim_registers_entry_with_ident() {
        // HEADER with an Ident token should register a new word entry at current DP.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        let dp_before = vm.dp;
        vm.token_stream = Some(VecDeque::from([make_ident_token("MYWORD")]));
        header_prim(&mut vm).unwrap();

        let xt = vm.latest.unwrap();
        let entry = &vm.headers[xt.index()];
        assert_eq!(entry.name, "MYWORD");
        assert!(matches!(entry.kind, crate::dict::EntryKind::Word(d) if d == dp_before));
        assert!(!entry.is_immediate());
        // Must be visible via normal lookup (not smudged).
        assert!(vm.lookup("MYWORD").is_some());
    }

    #[test]
    fn test_header_prim_does_not_advance_dp() {
        // HEADER must not modify vm.dp — data allocation is the caller's responsibility.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        let dp_before = vm.dp;
        vm.token_stream = Some(VecDeque::from([make_ident_token("WORD2")]));
        header_prim(&mut vm).unwrap();
        assert_eq!(vm.dp, dp_before);
    }

    #[test]
    fn test_header_prim_non_ident_token_returns_error() {
        // A non-Ident token should produce an InvalidExpression error.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        let tok = crate::lexer::SpannedToken {
            token: crate::lexer::Token::IntLit(42),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 2,
        };
        vm.token_stream = Some(VecDeque::from([tok]));
        let err = header_prim(&mut vm).unwrap_err();
        assert!(matches!(err, TbxError::InvalidExpression { .. }));
    }

    #[test]
    fn test_header_prim_no_stream_returns_token_stream_empty() {
        // token_stream is None → TokenStreamEmpty.
        let mut vm = VM::new();
        assert_eq!(header_prim(&mut vm), Err(TbxError::TokenStreamEmpty));
    }

    #[test]
    fn test_header_prim_empty_stream_returns_token_stream_empty() {
        // token_stream is an empty VecDeque → TokenStreamEmpty.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        vm.token_stream = Some(VecDeque::new());
        assert_eq!(header_prim(&mut vm), Err(TbxError::TokenStreamEmpty));
    }

    #[test]
    fn test_header_prim_registered_in_register_all() {
        // register_all() must include HEADER in the dictionary with FLAG_IMMEDIATE.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);
        let xt = vm.lookup("HEADER").unwrap();
        assert!(vm.headers[xt.index()].is_immediate());
    }

    // --- immediate_prim ---

    #[test]
    fn test_immediate_prim_sets_flag() {
        // IMMEDIATE FOO should set FLAG_IMMEDIATE on the word "FOO".
        use std::collections::VecDeque;
        let mut vm = VM::new();
        // Register a plain word entry so lookup("FOO") succeeds.
        let entry = crate::dict::WordEntry::new_word("FOO", 0);
        vm.register(entry);
        assert!(!vm.headers[vm.lookup("FOO").unwrap().index()].is_immediate());
        vm.token_stream = Some(VecDeque::from([make_ident_token("FOO")]));
        immediate_prim(&mut vm).unwrap();
        assert!(vm.headers[vm.lookup("FOO").unwrap().index()].is_immediate());
    }

    #[test]
    fn test_immediate_prim_is_idempotent() {
        // Calling IMMEDIATE twice on the same word must not corrupt the flags.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        let entry = crate::dict::WordEntry::new_word("BAR", 0);
        vm.register(entry);
        for _ in 0..2 {
            vm.token_stream = Some(VecDeque::from([make_ident_token("BAR")]));
            immediate_prim(&mut vm).unwrap();
        }
        let xt = vm.lookup("BAR").unwrap();
        // Only FLAG_IMMEDIATE should be set (bit-OR idempotent).
        assert_eq!(
            vm.headers[xt.index()].flags & crate::dict::FLAG_IMMEDIATE,
            crate::dict::FLAG_IMMEDIATE
        );
    }

    #[test]
    fn test_immediate_prim_non_ident_token_returns_error() {
        // A non-Ident token must produce an InvalidExpression error.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        let tok = crate::lexer::SpannedToken {
            token: crate::lexer::Token::IntLit(1),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 1,
        };
        vm.token_stream = Some(VecDeque::from([tok]));
        let err = immediate_prim(&mut vm).unwrap_err();
        assert!(matches!(err, TbxError::InvalidExpression { .. }));
    }

    #[test]
    fn test_immediate_prim_undefined_word_returns_error() {
        // Specifying a word name that is not in the dictionary must return UndefinedSymbol.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        vm.token_stream = Some(VecDeque::from([make_ident_token("NOSUCHWORD")]));
        let err = immediate_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::UndefinedSymbol { ref name } if name == "NOSUCHWORD"),
            "expected UndefinedSymbol(NOSUCHWORD), got {err:?}"
        );
    }

    #[test]
    fn test_immediate_prim_no_stream_returns_token_stream_empty() {
        // token_stream is None → TokenStreamEmpty.
        let mut vm = VM::new();
        assert_eq!(immediate_prim(&mut vm), Err(TbxError::TokenStreamEmpty));
    }

    #[test]
    fn test_immediate_prim_registered_in_register_all() {
        // register_all() must include IMMEDIATE in the dictionary with FLAG_IMMEDIATE.
        let mut vm = VM::new();
        crate::primitives::register_all(&mut vm);
        let xt = vm.lookup("IMMEDIATE").unwrap();
        assert!(vm.headers[xt.index()].is_immediate());
    }

    // ---------------------------------------------------------------------------
    // Error-path tests for IMMEDIATE primitives
    // ---------------------------------------------------------------------------

    /// Helper: build a VM with all primitives registered and return a minimal token stream.
    fn make_vm_with_tokens(tokens: Vec<crate::lexer::Token>) -> VM {
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        let spanned: Vec<crate::lexer::SpannedToken> = tokens
            .into_iter()
            .map(|t| crate::lexer::SpannedToken {
                token: t,
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 1,
            })
            .collect();
        vm.token_stream = Some(VecDeque::from(spanned));
        vm
    }

    // --- def_prim error paths ---

    #[test]
    fn test_def_nested_error() {
        // DEF inside an already-compiling context must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.is_compiling = true;
        vm.token_stream = Some(VecDeque::from([make_ident_token("FOO")]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_def_unexpected_token_after_name_error() {
        // A token other than '(' or end-of-stream after the word name must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // Supply: WORD <IntLit>  — IntLit is not LParen.
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(42),
                pos: crate::lexer::Position { line: 1, col: 6 },
                source_offset: 5,
                source_len: 2,
            },
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for unexpected token after word name, got {err:?}"
        );
    }

    // --- def_prim error paths: unclosed parentheses and trailing comma ---

    /// Helper: build a SpannedToken with LParen.
    fn make_lparen_token() -> crate::lexer::SpannedToken {
        crate::lexer::SpannedToken {
            token: crate::lexer::Token::LParen,
            pos: crate::lexer::Position { line: 1, col: 5 },
            source_offset: 4,
            source_len: 1,
        }
    }

    /// Helper: build a SpannedToken with Comma.
    fn make_comma_token() -> crate::lexer::SpannedToken {
        crate::lexer::SpannedToken {
            token: crate::lexer::Token::Comma,
            pos: crate::lexer::Position { line: 1, col: 7 },
            source_offset: 6,
            source_len: 1,
        }
    }

    /// Helper: build a SpannedToken with RParen.
    fn make_rparen_token() -> crate::lexer::SpannedToken {
        crate::lexer::SpannedToken {
            token: crate::lexer::Token::RParen,
            pos: crate::lexer::Position { line: 1, col: 9 },
            source_offset: 8,
            source_len: 1,
        }
    }

    #[test]
    fn test_def_unclosed_paren_no_params() {
        // DEF WORD( — unclosed '(' with no parameters must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for unclosed '(', got {err:?}"
        );
    }

    #[test]
    fn test_def_unclosed_paren_with_param() {
        // DEF WORD(X — unclosed '(' after one parameter must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for unclosed '(' after param, got {err:?}"
        );
    }

    #[test]
    fn test_def_unclosed_paren_after_comma() {
        // DEF WORD(X, — unclosed '(' after comma must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_comma_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for unclosed '(' after comma, got {err:?}"
        );
    }

    #[test]
    fn test_def_trailing_comma() {
        // DEF WORD(X,) — trailing comma before ')' must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_comma_token(),
            make_rparen_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for trailing comma, got {err:?}"
        );
    }

    #[test]
    fn test_def_params_without_comma() {
        // DEF WORD(X Y) — missing comma between parameters must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_ident_token("Y"),
            make_rparen_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for missing comma between params, got {err:?}"
        );
    }

    #[test]
    fn test_def_leading_comma() {
        // DEF WORD(,X) — leading comma must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_comma_token(),
            make_ident_token("X"),
            make_rparen_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for leading comma, got {err:?}"
        );
    }

    #[test]
    fn test_def_duplicate_param_name() {
        // DEF WORD(X, X) — duplicate parameter name must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_comma_token(),
            make_ident_token("X"),
            make_rparen_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("duplicate parameter name")),
            "expected InvalidExpression for duplicate param name, got {err:?}"
        );
    }

    #[test]
    fn test_def_duplicate_param_name_first_and_third() {
        // DEF WORD(X, Y, X) — duplicate between 1st and 3rd param must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_comma_token(),
            make_ident_token("Y"),
            make_comma_token(),
            make_ident_token("X"),
            make_rparen_token(),
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("duplicate parameter name")),
            "expected InvalidExpression for first-and-third duplicate param, got {err:?}"
        );
    }

    #[test]
    fn test_def_invalid_token_after_comma() {
        // DEF WORD(X, 42) — non-ident token after comma must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_comma_token(),
            crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(42),
                pos: crate::lexer::Position { line: 1, col: 9 },
                source_offset: 8,
                source_len: 2,
            },
        ]));
        let err = def_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for non-ident after comma, got {err:?}"
        );
    }

    // --- def_prim normal cases ---

    #[test]
    fn test_def_prim_no_params_enters_compile_mode() {
        // DEF WORD (no parameter list) must set is_compiling to true with no locals.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // Provide only the word name token; token stream ends after that.
        vm.token_stream = Some(VecDeque::from([make_ident_token("MYWORD")]));
        def_prim(&mut vm).unwrap();
        assert!(vm.is_compiling, "is_compiling must be true after DEF");
        let state = vm
            .compile_state
            .as_ref()
            .expect("compile_state must be set");
        assert_eq!(state.word_name, "MYWORD");
        assert_eq!(state.arity, 0);
        assert!(state.local_table.is_empty());
    }

    #[test]
    fn test_def_prim_with_params_sets_local_table_and_arity() {
        // DEF WORD(X, Y) must enter compile mode with arity=2 and local_table {X:0, Y:1}.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // Tokens: WORD ( X , Y )
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_ident_token("X"),
            make_comma_token(),
            make_ident_token("Y"),
            make_rparen_token(),
        ]));
        def_prim(&mut vm).unwrap();
        assert!(vm.is_compiling, "is_compiling must be true after DEF");
        let state = vm
            .compile_state
            .as_ref()
            .expect("compile_state must be set");
        assert_eq!(state.arity, 2);
        assert_eq!(state.local_table.get("X").copied(), Some(0));
        assert_eq!(state.local_table.get("Y").copied(), Some(1));
    }

    #[test]
    fn test_def_prim_empty_params_enters_compile_mode() {
        // DEF WORD() — explicit empty parameter list must enter compile mode with arity=0.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            make_lparen_token(),
            make_rparen_token(),
        ]));
        def_prim(&mut vm).unwrap();
        assert!(vm.is_compiling, "is_compiling must be true after DEF");
        let state = vm
            .compile_state
            .as_ref()
            .expect("compile_state must be set");
        assert_eq!(state.word_name, "WORD");
        assert_eq!(state.arity, 0);
        assert!(state.local_table.is_empty());
    }

    // --- end_prim normal case ---

    #[test]
    fn test_end_prim_normal() {
        // end_prim called after def_prim should:
        // - write EXIT into the dictionary
        // - clear FLAG_HIDDEN on the word header (unsmudge)
        // - set is_compiling to false
        let mut vm = make_compiling_vm("MYWORD");

        // Record the word header index before calling end_prim.
        let word_hdr_idx = vm
            .compile_state
            .as_ref()
            .map(|s| s.word_hdr_idx())
            .expect("compile_state must be set");

        // The word should be hidden (smudged) while being compiled.
        assert!(
            vm.headers[word_hdr_idx].flags & crate::dict::FLAG_HIDDEN != 0,
            "word must be hidden during compilation"
        );

        end_prim(&mut vm).unwrap();

        // is_compiling must be cleared.
        assert!(!vm.is_compiling, "is_compiling must be false after END");

        // FLAG_HIDDEN must be cleared (unsmudged).
        assert_eq!(
            vm.headers[word_hdr_idx].flags & crate::dict::FLAG_HIDDEN,
            0,
            "FLAG_HIDDEN must be cleared after END"
        );

        // The last cell written to the dictionary must be EXIT (an Xt pointing to
        // an Exit entry).
        let exit_cell = vm.dict_read(vm.dp - 1).expect("dict_read should succeed");
        assert!(
            matches!(exit_cell, crate::cell::Cell::Xt(_)),
            "last written cell must be an Xt (EXIT), got {exit_cell:?}"
        );
        if let crate::cell::Cell::Xt(xt) = exit_cell {
            assert!(
                matches!(vm.headers[xt.index()].kind, crate::dict::EntryKind::Exit),
                "EXIT xt must point to an Exit entry"
            );
        }
    }

    #[test]
    fn test_end_outside_def_error() {
        // END called when is_compiling == false must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        // is_compiling is false by default; compile_state is None.
        let err = end_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_end_unresolved_label_error() {
        // END must return UndefinedLabel when patch_list contains forward references
        // that were never resolved (i.e., a GOTO target label was never defined).
        let mut vm = make_compiling_vm("LABELWORD");
        // Manually inject an unresolved forward reference (label 99) into patch_list.
        if let Some(state) = vm.compile_state.as_mut() {
            state.patch_list.push((99, 0));
        }
        let err = end_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::UndefinedLabel { label: 99 }),
            "expected UndefinedLabel {{ label: 99 }}, got {err:?}"
        );
    }

    // --- goto_prim error paths ---

    #[test]
    fn test_goto_outside_def_error() {
        // GOTO outside a DEF body must return InvalidExpression.
        let mut vm = make_vm_with_tokens(vec![crate::lexer::Token::IntLit(10)]);
        let err = goto_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    // --- bif_prim error paths ---

    #[test]
    fn test_bif_outside_def_error() {
        // BIF outside a DEF body must return InvalidExpression.
        let mut vm = make_vm_with_tokens(vec![]);
        let err = bif_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    // --- bit_prim error paths ---

    #[test]
    fn test_bit_outside_def_error() {
        // BIT outside a DEF body must return InvalidExpression.
        let mut vm = make_vm_with_tokens(vec![]);
        let err = bit_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    // --- return_prim error paths ---

    #[test]
    fn test_return_outside_def_error() {
        // RETURN outside a DEF body must return InvalidExpression.
        let mut vm = make_vm_with_tokens(vec![]);
        let err = return_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    // --- var_prim error paths ---

    #[test]
    fn test_var_no_name_token_stream_empty() {
        // VAR with an empty token stream must return TokenStreamEmpty.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::new());
        let err = var_prim(&mut vm).unwrap_err();
        assert_eq!(err, TbxError::TokenStreamEmpty);
    }

    // ---------------------------------------------------------------------------
    // Normal-case (happy-path) tests for IMMEDIATE primitives
    // ---------------------------------------------------------------------------

    /// Helper: create a VM in compile mode by calling def_prim with the given word name.
    /// Returns the VM with is_compiling == true and a fresh CompileState.
    fn make_compiling_vm(word_name: &str) -> VM {
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // Feed the word name token; def_prim will also try to read a second token
        // (checking for LParen), but TokenStreamEmpty is tolerated there.
        vm.token_stream = Some(VecDeque::from([make_ident_token(word_name)]));
        def_prim(&mut vm).unwrap();
        vm
    }

    // --- var_prim normal cases ---

    #[test]
    fn test_var_prim_local_variable() {
        // VAR X inside DEF should register X in compile_state.local_table.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        vm.token_stream = Some(VecDeque::from([make_ident_token("X")]));
        var_prim(&mut vm).unwrap();
        let state = vm.compile_state.as_ref().unwrap();
        assert_eq!(state.local_table.get("X").copied(), Some(0));
        assert_eq!(state.local_count, 1);
    }

    #[test]
    fn test_var_prim_global_variable() {
        // VAR MYVAR outside DEF should register a Variable entry in the dictionary.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // is_compiling is false by default.
        vm.token_stream = Some(VecDeque::from([make_ident_token("MYVAR")]));
        var_prim(&mut vm).unwrap();
        let xt = vm.lookup("MYVAR").expect("MYVAR should be registered");
        assert!(
            matches!(
                vm.headers[xt.index()].kind,
                crate::dict::EntryKind::Variable(_)
            ),
            "expected Variable entry, got {:?}",
            vm.headers[xt.index()].kind
        );
    }

    #[test]
    fn test_var_prim_multi_local_variables() {
        // VAR A, B, C inside DEF should register three independent local-variable slots.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("MULTIWORD");
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("A"),
            make_comma_token(),
            make_ident_token("B"),
            make_comma_token(),
            make_ident_token("C"),
        ]));
        var_prim(&mut vm).unwrap();
        let state = vm.compile_state.as_ref().unwrap();
        assert_eq!(state.local_count, 3);
        assert_eq!(state.local_table.get("A").copied(), Some(0));
        assert_eq!(state.local_table.get("B").copied(), Some(1));
        assert_eq!(state.local_table.get("C").copied(), Some(2));
    }

    #[test]
    fn test_var_prim_multi_global_variables() {
        // VAR X, Y outside DEF should register two independent global Variable entries.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("X"),
            make_comma_token(),
            make_ident_token("Y"),
        ]));
        var_prim(&mut vm).unwrap();
        let xt_x = vm.lookup("X").expect("X should be registered");
        let xt_y = vm.lookup("Y").expect("Y should be registered");
        assert!(
            matches!(
                vm.headers[xt_x.index()].kind,
                crate::dict::EntryKind::Variable(_)
            ),
            "expected Variable entry for X, got {:?}",
            vm.headers[xt_x.index()].kind
        );
        assert!(
            matches!(
                vm.headers[xt_y.index()].kind,
                crate::dict::EntryKind::Variable(_)
            ),
            "expected Variable entry for Y, got {:?}",
            vm.headers[xt_y.index()].kind
        );
        // Each variable should occupy a distinct storage cell.
        let addr_x = match vm.headers[xt_x.index()].kind {
            crate::dict::EntryKind::Variable(a) => a,
            _ => panic!("expected Variable"),
        };
        let addr_y = match vm.headers[xt_y.index()].kind {
            crate::dict::EntryKind::Variable(a) => a,
            _ => panic!("expected Variable"),
        };
        assert_ne!(addr_x, addr_y, "X and Y must use different storage cells");
    }

    #[test]
    fn test_var_prim_comma_without_ident_returns_error() {
        // VAR A, 1 (non-ident after comma) should return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("BADWORD");
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("A"),
            make_comma_token(),
            crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(1),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 1,
            },
        ]));
        let err = var_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_var_prim_non_comma_token_returned_to_stream() {
        // After VAR A followed by a non-comma token, the non-comma token must be
        // pushed back so later consumers can still read it.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("PUSHBACKWORD");
        let newline_tok = crate::lexer::SpannedToken {
            token: crate::lexer::Token::Newline,
            pos: crate::lexer::Position { line: 1, col: 2 },
            source_offset: 1,
            source_len: 1,
        };
        vm.token_stream = Some(VecDeque::from([make_ident_token("A"), newline_tok.clone()]));
        var_prim(&mut vm).unwrap();
        // The Newline token must have been pushed back to the front.
        let remaining = vm
            .token_stream
            .as_ref()
            .expect("stream should still be Some");
        assert_eq!(remaining.len(), 1);
        assert!(
            matches!(remaining[0].token, crate::lexer::Token::Newline),
            "expected Newline to be pushed back, got {:?}",
            remaining[0].token
        );
    }

    // --- var_prim with initializer ---

    #[test]
    fn test_var_prim_init_registers_local_and_emits_set() {
        // VAR X = 42 inside DEF should register X in local_table and emit
        // [Xt(LIT), StackAddr(0), Xt(LIT), Int(42), Xt(SET)] to the dictionary.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("INITWORD");
        let dp_before = vm.dp;
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("X"),
            make_op_token("="),
            crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(42),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 2,
            },
        ]));
        var_prim(&mut vm).unwrap();

        // X should be in local_table with index 0.
        let state = vm.compile_state.as_ref().unwrap();
        assert_eq!(state.local_table.get("X").copied(), Some(0));
        assert_eq!(state.local_count, 1);

        // Dictionary should have grown by at least 4 cells:
        // [Xt(LIT), StackAddr(0), Xt(LIT), Int(42), Xt(SET)]
        assert!(
            vm.dp >= dp_before + 5,
            "expected at least 5 cells emitted, dp_before={dp_before} dp={}",
            vm.dp
        );

        // Cell 0: Xt(LIT)
        assert!(
            matches!(vm.dict_read(dp_before).unwrap(), Cell::Xt(_)),
            "expected Xt(LIT) at dp_before"
        );
        // Cell 1: StackAddr(0) — the address of local variable X
        assert_eq!(
            vm.dict_read(dp_before + 1).unwrap(),
            Cell::StackAddr(0),
            "expected StackAddr(0) for local X"
        );
        // Cell 4: Xt(SET)
        assert!(
            matches!(vm.dict_read(dp_before + 4).unwrap(), Cell::Xt(_)),
            "expected Xt(SET) at dp_before+4"
        );
    }

    #[test]
    fn test_var_prim_init_empty_expr_is_error() {
        // VAR X = (nothing) should return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("EMPTYINIT");
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("X"),
            make_op_token("="),
            // no expression tokens after '='
        ]));
        let err = var_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for empty initializer, got {err:?}"
        );
    }

    #[test]
    fn test_var_prim_init_duplicate_name_is_error() {
        // VAR X followed by VAR X = expr should return InvalidExpression for duplicate name.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("DUPWORD");
        // First: declare X without initializer.
        vm.token_stream = Some(VecDeque::from([make_ident_token("X")]));
        var_prim(&mut vm).unwrap();

        // Second: attempt VAR X = 1 — X already in local_table.
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("X"),
            make_op_token("="),
            crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(1),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 1,
            },
        ]));
        let err = var_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("duplicate")),
            "expected InvalidExpression for duplicate local variable, got {err:?}"
        );
    }

    // --- var_prim top-level initializer error ---

    #[test]
    fn test_var_prim_global_with_initializer_is_error() {
        // VAR X = 1 outside DEF must return InvalidExpression.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // is_compiling is false by default (top-level / execute mode).
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("X"),
            make_op_token("="),
            crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(1),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 1,
            },
        ]));
        let err = var_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("outside DEF")),
            "expected InvalidExpression mentioning 'outside DEF', got {err:?}"
        );
        // The variable must NOT have been registered: the dictionary must remain unchanged.
        assert!(
            vm.lookup("X").is_none(),
            "variable 'X' must not be registered when VAR initializer is rejected at top level"
        );
    }

    #[test]
    fn test_var_prim_global_without_initializer_still_works() {
        // VAR X outside DEF (without initializer) must still register a global variable.
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([make_ident_token("X")]));
        var_prim(&mut vm).unwrap();
        let xt = vm
            .lookup("X")
            .expect("X should be registered as a global variable");
        assert!(
            matches!(
                vm.headers[xt.index()].kind,
                crate::dict::EntryKind::Variable(_)
            ),
            "expected Variable entry for X, got {:?}",
            vm.headers[xt.index()].kind
        );
    }

    // --- goto_prim normal case ---

    #[test]
    fn test_goto_prim_writes_dict() {
        // GOTO 10 inside DEF should write [Xt(goto_rt), DictAddr(0)] to the dictionary
        // (forward reference: label not yet seen, so placeholder DictAddr(0) is emitted
        // and (10, dict_offset) is pushed to patch_list).
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("GOTOWORD");
        let dp_before = vm.dp;
        vm.token_stream = Some(VecDeque::from([crate::lexer::SpannedToken {
            token: crate::lexer::Token::IntLit(10),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 2,
        }]));
        goto_prim(&mut vm).unwrap();
        // dict[dp_before] = Xt(goto runtime entry), dict[dp_before+1] = DictAddr(0) placeholder.
        let goto_cell = vm.dict_read(dp_before).unwrap();
        let target_cell = vm.dict_read(dp_before + 1).unwrap();
        assert!(
            matches!(goto_cell, Cell::Xt(_)),
            "expected Xt for GOTO opcode, got {:?}",
            goto_cell
        );
        assert_eq!(
            target_cell,
            Cell::DictAddr(0),
            "expected forward-ref placeholder DictAddr(0)"
        );
        // patch_list should record the forward reference.
        let state = vm.compile_state.as_ref().unwrap();
        assert_eq!(state.patch_list, vec![(10, dp_before + 1)]);
    }

    // --- bif_prim normal case ---

    #[test]
    fn test_bif_prim_writes_dict() {
        // BIF 1, 20 inside DEF should compile condition (LIT, Int(1)),
        // then emit [Xt(bif_rt), DictAddr(0)] as a forward reference placeholder.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("BIFWORD");
        let dp_before = vm.dp;
        // Token stream: condition=IntLit(1), Comma, label=IntLit(20)
        let make_tok = |t| crate::lexer::SpannedToken {
            token: t,
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 1,
        };
        vm.token_stream = Some(VecDeque::from([
            make_tok(crate::lexer::Token::IntLit(1)),
            make_tok(crate::lexer::Token::Comma),
            make_tok(crate::lexer::Token::IntLit(20)),
        ]));
        bif_prim(&mut vm).unwrap();
        // Condition expression for literal 1: [Xt(LIT), Int(1)] then [Xt(bif_rt), DictAddr(0)].
        let lit_cell = vm.dict_read(dp_before).unwrap();
        let val_cell = vm.dict_read(dp_before + 1).unwrap();
        let bif_cell = vm.dict_read(dp_before + 2).unwrap();
        let target_cell = vm.dict_read(dp_before + 3).unwrap();
        assert!(
            matches!(lit_cell, Cell::Xt(_)),
            "expected LIT Xt, got {:?}",
            lit_cell
        );
        assert_eq!(val_cell, Cell::Int(1));
        assert!(
            matches!(bif_cell, Cell::Xt(_)),
            "expected BIF Xt, got {:?}",
            bif_cell
        );
        assert_eq!(
            target_cell,
            Cell::DictAddr(0),
            "expected forward-ref placeholder"
        );
        // patch_list should record label 20.
        let state = vm.compile_state.as_ref().unwrap();
        assert!(
            state.patch_list.iter().any(|&(lbl, _)| lbl == 20),
            "expected patch_list to contain label 20, got {:?}",
            state.patch_list
        );
    }

    // --- bit_prim normal case ---

    #[test]
    fn test_bit_prim_writes_dict() {
        // BIT 1, 30 inside DEF should compile condition then emit [Xt(bit_rt), DictAddr(0)].
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("BITWORD");
        let dp_before = vm.dp;
        let make_tok = |t| crate::lexer::SpannedToken {
            token: t,
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 1,
        };
        vm.token_stream = Some(VecDeque::from([
            make_tok(crate::lexer::Token::IntLit(1)),
            make_tok(crate::lexer::Token::Comma),
            make_tok(crate::lexer::Token::IntLit(30)),
        ]));
        bit_prim(&mut vm).unwrap();
        // [Xt(LIT), Int(1), Xt(bit_rt), DictAddr(0)]
        let lit_cell = vm.dict_read(dp_before).unwrap();
        let val_cell = vm.dict_read(dp_before + 1).unwrap();
        let bit_cell = vm.dict_read(dp_before + 2).unwrap();
        let target_cell = vm.dict_read(dp_before + 3).unwrap();
        assert!(
            matches!(lit_cell, Cell::Xt(_)),
            "expected LIT Xt, got {:?}",
            lit_cell
        );
        assert_eq!(val_cell, Cell::Int(1));
        assert!(
            matches!(bit_cell, Cell::Xt(_)),
            "expected BIT Xt, got {:?}",
            bit_cell
        );
        assert_eq!(
            target_cell,
            Cell::DictAddr(0),
            "expected forward-ref placeholder"
        );
        let state = vm.compile_state.as_ref().unwrap();
        assert!(
            state.patch_list.iter().any(|&(lbl, _)| lbl == 30),
            "expected patch_list to contain label 30, got {:?}",
            state.patch_list
        );
    }

    // --- return_prim normal case ---

    #[test]
    fn test_return_prim_void_writes_exit() {
        // RETURN with no expression inside DEF should emit Xt(EXIT) to the dictionary.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("RETWORD");
        let dp_before = vm.dp;
        // Empty token stream → void return.
        vm.token_stream = Some(VecDeque::new());
        return_prim(&mut vm).unwrap();
        let cell = vm.dict_read(dp_before).unwrap();
        assert!(
            matches!(cell, Cell::Xt(_)),
            "expected Xt(EXIT), got {:?}",
            cell
        );
        // Verify it is the EXIT entry by checking kind.
        if let Cell::Xt(xt) = cell {
            assert!(
                matches!(vm.headers[xt.index()].kind, crate::dict::EntryKind::Exit),
                "expected Exit kind, got {:?}",
                vm.headers[xt.index()].kind
            );
        }
    }

    #[test]
    fn test_return_prim_with_expr_writes_return_val() {
        // RETURN 42 inside DEF should:
        //   1. compile the expression (emitting Xt(LIT), Cell::Int(42) to the dictionary),
        //   2. emit Xt(RETURN_VAL) immediately after,
        //   3. restore local_table in compile_state.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("RETEXPR");
        let dp_before = vm.dp;

        // Provide token stream with the integer literal 42.
        vm.token_stream = Some(VecDeque::from([crate::lexer::SpannedToken {
            token: crate::lexer::Token::IntLit(42),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 2,
        }]));
        return_prim(&mut vm).unwrap();

        // ExprCompiler emits Xt(LIT) then the value for integer literals.
        // Dictionary layout: [Xt(LIT), Int(42), Xt(RETURN_VAL)]
        let cell0 = vm.dict_read(dp_before).unwrap();
        assert!(
            matches!(cell0, Cell::Xt(_)),
            "expected Xt(LIT) at dp+0, got {:?}",
            cell0
        );

        let cell1 = vm.dict_read(dp_before + 1).unwrap();
        assert_eq!(
            cell1,
            Cell::Int(42),
            "expected Int(42) at dp+1, got {:?}",
            cell1
        );

        let cell2 = vm.dict_read(dp_before + 2).unwrap();
        assert!(
            matches!(cell2, Cell::Xt(_)),
            "expected Xt(RETURN_VAL) at dp+2, got {:?}",
            cell2
        );
        if let Cell::Xt(xt) = cell2 {
            assert!(
                matches!(
                    vm.headers[xt.index()].kind,
                    crate::dict::EntryKind::ReturnVal
                ),
                "expected ReturnVal kind, got {:?}",
                vm.headers[xt.index()].kind
            );
        }

        // local_table must have been restored in compile_state.
        assert!(
            vm.compile_state.is_some(),
            "compile_state should still be set after return_prim"
        );
    }

    // --- cs_push_prim ---

    #[test]
    fn test_cs_push_prim_outside_compile_mode_error() {
        // CS_PUSH called when is_compiling == false must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.push(Cell::Int(42)).unwrap();
        let err = cs_push_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_cs_push_prim_moves_value_to_compile_stack() {
        // CS_PUSH must pop the top of the data stack and push it onto compile_stack.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.push(Cell::Int(7)).unwrap();
        cs_push_prim(&mut vm).unwrap();
        // data stack must be empty.
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
        // compile_stack must hold the value.
        assert_eq!(
            vm.compile_stack.last(),
            Some(&CompileEntry::Cell(Cell::Int(7)))
        );
    }

    // --- cs_pop_prim ---

    #[test]
    fn test_cs_pop_prim_outside_compile_mode_error() {
        // CS_POP called when is_compiling == false must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        let err = cs_pop_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_cs_pop_prim_empty_compile_stack_error() {
        // CS_POP with an empty compile_stack must return StackUnderflow.
        let mut vm = make_compiling_vm("TESTWORD");
        assert!(vm.compile_stack.is_empty());
        let err = cs_pop_prim(&mut vm).unwrap_err();
        assert_eq!(err, TbxError::StackUnderflow);
    }

    #[test]
    fn test_cs_pop_prim_moves_value_to_data_stack() {
        // CS_POP must pop the top of compile_stack and push it onto the data stack.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(99)));
        cs_pop_prim(&mut vm).unwrap();
        assert!(vm.compile_stack.is_empty());
        assert_eq!(vm.pop(), Ok(Cell::Int(99)));
    }

    #[test]
    fn test_cs_pop_prim_tag_on_top_type_error() {
        // CS_POP with a Tag on top must return TypeError and leave the tag intact.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Tag("IF".to_string()));
        let err = cs_pop_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::TypeError { .. }),
            "expected TypeError, got {err:?}"
        );
        // Tag must be preserved on the compile_stack.
        assert_eq!(
            vm.compile_stack.last(),
            Some(&CompileEntry::Tag("IF".to_string()))
        );
    }

    #[test]
    fn test_cs_swap_outside_compile_mode_error() {
        let mut vm = VM::new();
        assert_eq!(
            cs_swap_prim(&mut vm),
            Err(TbxError::InvalidExpression {
                reason: "CS_SWAP outside compile mode"
            })
        );
    }

    #[test]
    fn test_cs_swap_underflow_one_element() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        assert_eq!(cs_swap_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_swap_underflow_empty() {
        let mut vm = make_compiling_vm("TESTWORD");
        assert_eq!(cs_swap_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_swap_swaps_top_two() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(10)));
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(20)));
        cs_swap_prim(&mut vm).unwrap();
        assert_eq!(
            vm.compile_stack.pop(),
            Some(CompileEntry::Cell(Cell::Int(10)))
        );
        assert_eq!(
            vm.compile_stack.pop(),
            Some(CompileEntry::Cell(Cell::Int(20)))
        );
        assert!(vm.compile_stack.is_empty());
    }

    #[test]
    fn test_cs_swap_swaps_dict_addr_values() {
        // CS_SWAP must work with Cell::DictAddr values, as used in WHILE/ENDWH.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack
            .push(CompileEntry::Cell(Cell::DictAddr(10)));
        vm.compile_stack
            .push(CompileEntry::Cell(Cell::DictAddr(20)));
        cs_swap_prim(&mut vm).unwrap();
        assert_eq!(
            vm.compile_stack.pop(),
            Some(CompileEntry::Cell(Cell::DictAddr(10)))
        );
        assert_eq!(
            vm.compile_stack.pop(),
            Some(CompileEntry::Cell(Cell::DictAddr(20)))
        );
        assert!(vm.compile_stack.is_empty());
    }

    // --- cs_drop_prim ---

    #[test]
    fn test_cs_drop_outside_compile_mode_error() {
        let mut vm = VM::new();
        assert_eq!(
            cs_drop_prim(&mut vm),
            Err(TbxError::InvalidExpression {
                reason: "CS_DROP outside compile mode"
            })
        );
    }

    #[test]
    fn test_cs_drop_underflow() {
        let mut vm = make_compiling_vm("TESTWORD");
        assert_eq!(cs_drop_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_drop_removes_top() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(2)));
        cs_drop_prim(&mut vm).unwrap();
        assert_eq!(vm.compile_stack.len(), 1);
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::Int(1)));
    }

    // --- cs_dup_prim ---

    #[test]
    fn test_cs_dup_outside_compile_mode_error() {
        let mut vm = VM::new();
        assert_eq!(
            cs_dup_prim(&mut vm),
            Err(TbxError::InvalidExpression {
                reason: "CS_DUP outside compile mode"
            })
        );
    }

    #[test]
    fn test_cs_dup_underflow() {
        let mut vm = make_compiling_vm("TESTWORD");
        assert_eq!(cs_dup_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_dup_duplicates_top() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(42)));
        cs_dup_prim(&mut vm).unwrap();
        assert_eq!(vm.compile_stack.len(), 2);
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::Int(42)));
        assert_eq!(vm.compile_stack[1], CompileEntry::Cell(Cell::Int(42)));
    }

    #[test]
    fn test_cs_dup_duplicates_dict_addr() {
        // CS_DUP must work with Cell::DictAddr values.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack
            .push(CompileEntry::Cell(Cell::DictAddr(42)));
        cs_dup_prim(&mut vm).unwrap();
        assert_eq!(vm.compile_stack.len(), 2);
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::DictAddr(42)));
        assert_eq!(vm.compile_stack[1], CompileEntry::Cell(Cell::DictAddr(42)));
    }

    // --- cs_over_prim ---

    #[test]
    fn test_cs_over_outside_compile_mode_error() {
        let mut vm = VM::new();
        assert_eq!(
            cs_over_prim(&mut vm),
            Err(TbxError::InvalidExpression {
                reason: "CS_OVER outside compile mode"
            })
        );
    }

    #[test]
    fn test_cs_over_underflow_empty() {
        let mut vm = make_compiling_vm("TESTWORD");
        assert_eq!(cs_over_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_over_underflow_one_element() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        assert_eq!(cs_over_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_over_copies_second_to_top() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(10))); // bottom
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(20))); // top
        cs_over_prim(&mut vm).unwrap();
        // Stack should be [10, 20, 10] with 10 on top
        assert_eq!(vm.compile_stack.len(), 3);
        assert_eq!(vm.compile_stack[2], CompileEntry::Cell(Cell::Int(10)));
        assert_eq!(vm.compile_stack[1], CompileEntry::Cell(Cell::Int(20)));
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::Int(10)));
    }

    #[test]
    fn test_cs_over_copies_dict_addr() {
        // CS_OVER must work with Cell::DictAddr values.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack
            .push(CompileEntry::Cell(Cell::DictAddr(10)));
        vm.compile_stack
            .push(CompileEntry::Cell(Cell::DictAddr(20)));
        cs_over_prim(&mut vm).unwrap();
        assert_eq!(vm.compile_stack.len(), 3);
        assert_eq!(vm.compile_stack[2], CompileEntry::Cell(Cell::DictAddr(10)));
        assert_eq!(vm.compile_stack[1], CompileEntry::Cell(Cell::DictAddr(20)));
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::DictAddr(10)));
    }

    // --- cs_rot_prim ---

    #[test]
    fn test_cs_rot_outside_compile_mode_error() {
        let mut vm = VM::new();
        assert_eq!(
            cs_rot_prim(&mut vm),
            Err(TbxError::InvalidExpression {
                reason: "CS_ROT outside compile mode"
            })
        );
    }

    #[test]
    fn test_cs_rot_underflow_empty() {
        let mut vm = make_compiling_vm("TESTWORD");
        assert_eq!(cs_rot_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_rot_underflow_one_element() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        assert_eq!(cs_rot_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_rot_underflow_two_elements() {
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(2)));
        assert_eq!(cs_rot_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    #[test]
    fn test_cs_rot_rotates_top_three() {
        // ( a b c -- b c a )  where a=1 (bottom), b=2, c=3 (top)
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1))); // a
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(2))); // b
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(3))); // c
        cs_rot_prim(&mut vm).unwrap();
        // Result: [b=2, c=3, a=1] with a=1 on top
        assert_eq!(vm.compile_stack.len(), 3);
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::Int(2)));
        assert_eq!(vm.compile_stack[1], CompileEntry::Cell(Cell::Int(3)));
        assert_eq!(vm.compile_stack[2], CompileEntry::Cell(Cell::Int(1)));
    }

    #[test]
    fn test_cs_rot_rotates_dict_addr_values() {
        // CS_ROT must work with Cell::DictAddr values (as used in WHILE/ENDWH).
        // ( a b c -- b c a ) with DictAddr values
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::DictAddr(1))); // a
        vm.compile_stack.push(CompileEntry::Cell(Cell::DictAddr(2))); // b
        vm.compile_stack.push(CompileEntry::Cell(Cell::DictAddr(3))); // c
        cs_rot_prim(&mut vm).unwrap();
        assert_eq!(vm.compile_stack.len(), 3);
        assert_eq!(vm.compile_stack[0], CompileEntry::Cell(Cell::DictAddr(2)));
        assert_eq!(vm.compile_stack[1], CompileEntry::Cell(Cell::DictAddr(3)));
        assert_eq!(vm.compile_stack[2], CompileEntry::Cell(Cell::DictAddr(1)));
    }

    // --- compile_expr_prim ---

    #[test]
    fn test_end_prim_compile_stack_not_empty_error() {
        // end_prim must return CompileStackNotEmpty and rollback when compile_stack
        // has leftover items at the end of the word definition.
        let mut vm = make_compiling_vm("MYWORD");
        // Manually leave an item on compile_stack to simulate an incomplete definition.
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(1)));
        let err = end_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::CompileStackNotEmpty { count: 1 }),
            "expected CompileStackNotEmpty {{ count: 1 }}, got {err:?}"
        );
        // VM must have been rolled back: is_compiling should be false.
        assert!(
            !vm.is_compiling,
            "is_compiling must be false after rollback"
        );
        // compile_stack must be cleared after rollback to prevent state leakage.
        assert!(
            vm.compile_stack.is_empty(),
            "compile_stack must be empty after rollback"
        );
    }

    #[test]
    fn test_end_prim_tag_on_compile_stack_error() {
        // end_prim must return CompileStackNotEmpty and rollback when a Tag entry
        // is left on compile_stack (simulates an unclosed IF or WHILE).
        let mut vm = make_compiling_vm("MYWORD3");
        vm.compile_stack.push(CompileEntry::Tag("IF".to_string()));
        let err = end_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::CompileStackNotEmpty { count: 1 }),
            "expected CompileStackNotEmpty {{ count: 1 }}, got {err:?}"
        );
        // VM must have been rolled back.
        assert!(
            !vm.is_compiling,
            "is_compiling must be false after rollback"
        );
        assert!(
            vm.compile_stack.is_empty(),
            "compile_stack must be empty after rollback"
        );
    }

    #[test]
    fn test_compile_expr_prim_outside_compile_mode_error() {
        // COMPILE_EXPR called when is_compiling == false must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(
            vec![crate::lexer::SpannedToken {
                token: crate::lexer::Token::Ident("X".to_string()),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 1,
            }]
            .into(),
        );
        let err = compile_expr_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_compile_expr_prim_no_token_stream_error() {
        // COMPILE_EXPR with token_stream == None must return TokenStreamEmpty.
        let mut vm = make_compiling_vm("TESTWORD");
        // Explicitly set token_stream to None.
        vm.token_stream = None;
        let err = compile_expr_prim(&mut vm).unwrap_err();
        assert_eq!(err, TbxError::TokenStreamEmpty);
    }

    #[test]
    fn test_compile_expr_prim_empty_token_stream_error() {
        // COMPILE_EXPR with no tokens in the stream must return TokenStreamEmpty.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.token_stream = Some(std::collections::VecDeque::new());
        let err = compile_expr_prim(&mut vm).unwrap_err();
        assert_eq!(err, TbxError::TokenStreamEmpty);
    }

    #[test]
    fn test_compile_expr_prim_compiles_literal_to_dict() {
        // COMPILE_EXPR with a single integer literal must emit cells to dict.
        let mut vm = make_compiling_vm("TESTWORD");
        let dp_before = vm.dp;
        vm.token_stream = Some(
            vec![crate::lexer::SpannedToken {
                token: crate::lexer::Token::IntLit(42),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 2,
            }]
            .into(),
        );
        compile_expr_prim(&mut vm).unwrap();
        // At least one cell must have been written.
        assert!(vm.dp > dp_before, "dict must grow after COMPILE_EXPR");
        // token_stream must be drained.
        assert!(
            vm.token_stream
                .as_ref()
                .map(|s| s.is_empty())
                .unwrap_or(true),
            "token_stream must be empty after COMPILE_EXPR"
        );
    }

    // --- patch_addr_prim ---

    #[test]
    fn test_patch_addr_prim_outside_compile_mode_error() {
        // PATCH_ADDR called when is_compiling == false must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.push(Cell::DictAddr(0)).unwrap();
        let err = patch_addr_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_patch_addr_prim_wrong_type_error() {
        // PATCH_ADDR with a non-DictAddr on the stack must return TypeError.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.push(Cell::Int(99)).unwrap();
        let err = patch_addr_prim(&mut vm).unwrap_err();
        assert!(
            matches!(
                err,
                TbxError::TypeError {
                    expected: "DictAddr",
                    ..
                }
            ),
            "expected TypeError(DictAddr), got {err:?}"
        );
    }

    #[test]
    fn test_patch_addr_prim_writes_dict_addr_at_addr() {
        // PATCH_ADDR must pop DictAddr(a) and write Cell::DictAddr(dp) at dict[a].
        let mut vm = make_compiling_vm("TESTWORD");
        // Write a placeholder at a known position.
        let placeholder_pos = vm.dp;
        vm.dict_write(Cell::DictAddr(0)).unwrap();
        // Push some more cells so dp advances past the placeholder.
        vm.dict_write(Cell::Int(1)).unwrap();
        vm.dict_write(Cell::Int(2)).unwrap();
        let expected_dp = vm.dp;
        // Push the placeholder address onto the data stack and call PATCH_ADDR.
        vm.push(Cell::DictAddr(placeholder_pos)).unwrap();
        patch_addr_prim(&mut vm).unwrap();
        // dict[placeholder_pos] must now hold Cell::DictAddr(dp).
        assert_eq!(
            vm.dict_read(placeholder_pos).unwrap(),
            Cell::DictAddr(expected_dp)
        );
        // Data stack must be empty.
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
    }

    // --- cs_open_tag_prim ---

    #[test]
    fn test_cs_open_tag_outside_compile_mode_error() {
        // CS_OPEN_TAG outside compile mode must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.push(Cell::string("IF")).unwrap();
        let err = cs_open_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_cs_open_tag_pushes_tag_to_compile_stack() {
        // CS_OPEN_TAG must pop a Cell::Str and push the corresponding Tag onto
        // the compile_stack.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.push(Cell::string("WHILE")).unwrap();
        cs_open_tag_prim(&mut vm).unwrap();
        // data stack must be empty.
        assert_eq!(vm.pop(), Err(TbxError::StackUnderflow));
        // compile_stack must hold Tag("WHILE").
        assert_eq!(
            vm.compile_stack.last(),
            Some(&CompileEntry::Tag("WHILE".to_string()))
        );
    }

    #[test]
    fn test_cs_open_tag_type_error_non_string() {
        // CS_OPEN_TAG with a non-Str on the data stack must return TypeError.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.push(Cell::Int(42)).unwrap();
        let err = cs_open_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::TypeError { .. }),
            "expected TypeError, got {err:?}"
        );
    }

    #[test]
    fn test_cs_open_tag_empty_data_stack_error() {
        // CS_OPEN_TAG with empty data stack must return StackUnderflow.
        let mut vm = make_compiling_vm("TESTWORD");
        let err = cs_open_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::StackUnderflow),
            "expected StackUnderflow, got {err:?}"
        );
    }

    #[test]
    fn test_cs_close_tag_outside_compile_mode_error() {
        // CS_CLOSE_TAG outside compile mode must return InvalidExpression.
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.push(Cell::string("IF")).unwrap();
        let err = cs_close_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_cs_close_tag_matching_pops_tag() {
        // CS_CLOSE_TAG with matching Tag must succeed and pop it from compile_stack.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack
            .push(CompileEntry::Tag("WHILE".to_string()));
        vm.push(Cell::string("WHILE")).unwrap();
        cs_close_tag_prim(&mut vm).unwrap();
        assert!(vm.compile_stack.is_empty());
    }

    #[test]
    fn test_cs_close_tag_mismatched_tag_error() {
        // CS_CLOSE_TAG with a tag that does not match must return MismatchedTag.
        // Unlike the Cell-on-top case, a mismatched Tag is consumed (not restored):
        // the caller always encounters a compile error and rollback_def() clears
        // compile_stack anyway.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Tag("IF".to_string()));
        vm.push(Cell::string("WHILE")).unwrap();
        let err = cs_close_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(
                err,
                TbxError::MismatchedTag {
                    ref expected,
                    ref found
                } if expected == "WHILE" && found == "IF"
            ),
            "expected MismatchedTag(WHILE/IF), got {err:?}"
        );
        // After MismatchedTag the tag is consumed (not restored), which is intentional:
        // a compile error always triggers rollback_def() that clears compile_stack.
        assert!(
            vm.compile_stack.is_empty(),
            "mismatched tag must be consumed, not restored"
        );
    }

    #[test]
    fn test_cs_close_tag_empty_stack_error() {
        // CS_CLOSE_TAG with an empty compile_stack must return NoOpenTag.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.push(Cell::string("WHILE")).unwrap();
        let err = cs_close_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::NoOpenTag { ref expected } if expected == "WHILE"),
            "expected NoOpenTag(WHILE), got {err:?}"
        );
    }

    #[test]
    fn test_cs_close_tag_cell_on_top_error() {
        // CS_CLOSE_TAG with a Cell (not Tag) on top of compile_stack must return NoOpenTag.
        let mut vm = make_compiling_vm("TESTWORD");
        vm.compile_stack.push(CompileEntry::Cell(Cell::Int(42)));
        vm.push(Cell::string("IF")).unwrap();
        let err = cs_close_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::NoOpenTag { ref expected } if expected == "IF"),
            "expected NoOpenTag(IF), got {err:?}"
        );
        // The cell must be restored on the compile_stack.
        assert_eq!(
            vm.compile_stack.last(),
            Some(&CompileEntry::Cell(Cell::Int(42)))
        );
    }

    #[test]
    fn test_cs_open_close_tag_correct_nesting() {
        // CS_OPEN_TAG and CS_CLOSE_TAG must support correct IF/WHILE nesting.
        let mut vm = make_compiling_vm("TESTWORD");
        // Simulate: IF ... WHILE ... ENDWH ... ENDIF
        vm.push(Cell::string("IF")).unwrap();
        cs_open_tag_prim(&mut vm).unwrap(); // push Tag("IF")

        vm.push(Cell::string("WHILE")).unwrap();
        cs_open_tag_prim(&mut vm).unwrap(); // push Tag("WHILE")

        // Close WHILE
        vm.push(Cell::string("WHILE")).unwrap();
        cs_close_tag_prim(&mut vm).unwrap(); // pop Tag("WHILE")

        // Close IF
        vm.push(Cell::string("IF")).unwrap();
        cs_close_tag_prim(&mut vm).unwrap(); // pop Tag("IF")

        assert!(vm.compile_stack.is_empty());
    }

    // --- compile_lvalue_prim ---

    fn make_op_token(op: &str) -> crate::lexer::SpannedToken {
        crate::lexer::SpannedToken {
            token: crate::lexer::Token::Op(op.to_string()),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: op.len(),
        }
    }

    #[test]
    fn test_compile_lvalue_outside_compile_mode_error() {
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([make_ident_token("X")]));
        let result = compile_lvalue_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidExpression { .. })),
            "expected InvalidExpression outside compile mode"
        );
    }

    #[test]
    fn test_compile_lvalue_local_variable_emits_stack_addr() {
        // COMPILE_LVALUE with a known local variable should emit LIT StackAddr(idx).
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        // Declare a local variable X (index 0).
        vm.token_stream = Some(VecDeque::from([make_ident_token("X")]));
        var_prim(&mut vm).unwrap();

        let dp_before = vm.dp;
        vm.token_stream = Some(VecDeque::from([make_ident_token("X")]));
        compile_lvalue_prim(&mut vm).unwrap();

        // Two cells should have been written: Xt(LIT) and StackAddr(0).
        assert_eq!(vm.dp, dp_before + 2);
        let cell1 = vm.dict_read(dp_before).unwrap();
        let cell2 = vm.dict_read(dp_before + 1).unwrap();
        assert!(
            matches!(cell1, crate::cell::Cell::Xt(_)),
            "expected Xt(LIT)"
        );
        assert_eq!(cell2, crate::cell::Cell::StackAddr(0));
    }

    #[test]
    fn test_compile_lvalue_global_variable_emits_dict_addr() {
        // COMPILE_LVALUE with a global Variable entry should emit LIT DictAddr(addr).
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        // Register a global variable GVAR.
        vm.token_stream = Some(VecDeque::from([make_ident_token("GVAR")]));
        var_prim(&mut vm).unwrap();

        // Switch to compile mode to call compile_lvalue_prim.
        vm.token_stream = Some(VecDeque::from([make_ident_token("HELPER")]));
        def_prim(&mut vm).unwrap();

        let dp_before = vm.dp;
        vm.token_stream = Some(VecDeque::from([make_ident_token("GVAR")]));
        compile_lvalue_prim(&mut vm).unwrap();

        assert_eq!(vm.dp, dp_before + 2);
        let cell2 = vm.dict_read(dp_before + 1).unwrap();
        assert!(
            matches!(cell2, crate::cell::Cell::DictAddr(_)),
            "expected DictAddr for global variable"
        );
    }

    #[test]
    fn test_compile_lvalue_undefined_variable_error() {
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        vm.token_stream = Some(VecDeque::from([make_ident_token("NOSUCH")]));
        let result = compile_lvalue_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::UndefinedSymbol { .. })),
            "expected UndefinedSymbol for unknown variable"
        );
        // local_table must be restored even on error.
        assert!(
            vm.compile_state.is_some(),
            "compile_state should still exist"
        );
    }

    #[test]
    fn test_compile_lvalue_non_variable_identifier_error() {
        // Passing a non-variable identifier (e.g. a primitive word) should give TypeError.
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        // "DROP" is a known word but not a Variable.
        vm.token_stream = Some(VecDeque::from([make_ident_token("DROP")]));
        let result = compile_lvalue_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { .. })),
            "expected TypeError for non-variable identifier"
        );
        // local_table must be restored even on error.
        assert!(
            vm.compile_state.is_some(),
            "compile_state should still exist"
        );
    }

    #[test]
    fn test_compile_lvalue_non_ident_token_error() {
        // Passing a non-identifier token (e.g. an integer literal) as lvalue should
        // produce an InvalidExpression error (the `_ => ...` branch in compile_lvalue_prim).
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        let int_token = crate::lexer::SpannedToken {
            token: crate::lexer::Token::IntLit(10),
            pos: crate::lexer::Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 2,
        };
        vm.token_stream = Some(VecDeque::from([int_token]));
        let result = compile_lvalue_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidExpression { .. })),
            "expected InvalidExpression for non-Ident lvalue token"
        );
    }

    // --- skip_eq_prim ---

    #[test]
    fn test_skip_eq_outside_compile_mode_error() {
        use std::collections::VecDeque;
        let mut vm = VM::new();
        register_all(&mut vm);
        vm.token_stream = Some(VecDeque::from([make_op_token("=")]));
        let result = skip_eq_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidExpression { .. })),
            "expected InvalidExpression outside compile mode"
        );
    }

    #[test]
    fn test_skip_eq_consumes_equals_token() {
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        vm.token_stream = Some(VecDeque::from([make_op_token("=")]));
        skip_eq_prim(&mut vm).unwrap();
        // Token stream should be empty after consuming '='.
        assert!(vm.token_stream.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_skip_eq_non_equals_token_error() {
        use std::collections::VecDeque;
        let mut vm = make_compiling_vm("TESTWORD");
        vm.token_stream = Some(VecDeque::from([make_op_token("+")]));
        let result = skip_eq_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidExpression { .. })),
            "expected InvalidExpression for non-'=' token"
        );
    }

    // --- accept_prim ---

    #[test]
    fn test_accept_reads_line() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("hello\n"));
        let result = accept_prim(&mut vm).unwrap();
        assert_eq!(result, "hello".to_string());
    }

    #[test]
    fn test_accept_strips_trailing_newline() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("world\r\n"));
        let result = accept_prim(&mut vm).unwrap();
        assert_eq!(result, "world".to_string());
    }

    #[test]
    fn test_accept_does_not_push_to_stack() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("42\n"));
        accept_prim(&mut vm).unwrap();
        // Stack must remain empty — accept_prim only reads; it does not push.
        assert_eq!(vm.data_stack.len(), 0);
    }

    #[test]
    fn test_accept_empty_line() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("\n"));
        let result = accept_prim(&mut vm).unwrap();
        assert_eq!(result, "".to_string());
    }

    // --- getdec_prim ---

    #[test]
    fn test_getdec_pushes_integer() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("42\n"));
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
    }

    #[test]
    fn test_getdec_negative_integer() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("-7\n"));
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-7)));
    }

    #[test]
    fn test_getdec_trims_whitespace() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("  100  \n"));
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(100)));
    }

    #[test]
    fn test_getdec_does_not_use_input_buffer() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("10\n"));
        getdec_prim(&mut vm).unwrap();
        // getdec_prim reads directly via accept_prim; input_buffer is not used.
        assert_eq!(vm.input_buffer, None);
    }

    #[test]
    fn test_getdec_empty_buffer_returns_error() {
        use std::io::Cursor;
        let mut vm = VM::new();
        // EOF (empty reader) yields an empty string, which fails to parse as integer.
        vm.input_reader = Box::new(Cursor::new(""));
        let result = getdec_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::ParseIntError { .. })),
            "expected ParseIntError for empty input, got: {:?}",
            result
        );
    }

    #[test]
    fn test_getdec_non_integer_returns_error() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("abc\n"));
        let result = getdec_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::ParseIntError { .. })),
            "expected ParseIntError, got: {:?}",
            result
        );
    }

    #[test]
    fn test_getdec_reads_from_reader_directly() {
        use std::io::Cursor;
        // Verify that getdec_prim works without a prior accept_prim call.
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("123\n"));
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(123)));
    }

    // --- getstr_prim ---

    #[test]
    fn test_getstr_pushes_str() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("hello\n"));
        getstr_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::string("hello")));
    }

    #[test]
    fn test_getstr_empty_line() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("\n"));
        getstr_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::string("")));
    }

    #[test]
    fn test_getstr_strips_newline() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("world\r\n"));
        getstr_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("world"));
    }

    #[test]
    fn test_getstr_content_matches_input() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("foo bar\n"));
        getstr_prim(&mut vm).unwrap();
        let cell = vm.pop().unwrap();
        if let Cell::Str(s) = cell {
            assert_eq!(s.as_ref(), "foo bar");
        } else {
            panic!("expected Cell::Str, got {:?}", cell);
        }
    }

    #[test]
    fn test_getstr_flushes_output_before_read() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.output_buffer = "prompt: ".to_string();
        vm.input_reader = Box::new(Cursor::new("answer\n"));
        getstr_prim(&mut vm).unwrap();
        // After reading, the output buffer should have been flushed (empty).
        assert!(vm.output_buffer.is_empty());
    }

    // --- array_get_prim ---

    #[test]
    fn test_array_get_prim_reads_element() {
        // User index 1 maps to internal index 0.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![
            Cell::Int(10),
            Cell::Int(20),
            Cell::Int(30),
        ]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        array_get_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(10)));
    }

    #[test]
    fn test_array_get_prim_reads_second_element() {
        // User index 2 maps to internal index 1.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![
            Cell::Int(10),
            Cell::Int(20),
            Cell::Int(30),
        ]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        array_get_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_array_get_prim_out_of_bounds() {
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::Int(1)]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(5)).unwrap();
        assert!(matches!(
            array_get_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: 5, size: 1 })
        ));
    }

    #[test]
    fn test_array_get_prim_zero_index_is_out_of_bounds() {
        // Index 0 is invalid in 1-based indexing.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::Int(1)]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            array_get_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: 0, .. })
        ));
    }

    #[test]
    fn test_array_get_prim_negative_index() {
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::Int(1)]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(-1)).unwrap();
        assert!(matches!(
            array_get_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: -1, .. })
        ));
    }

    // --- array_addr_prim ---

    #[test]
    fn test_array_addr_prim_pushes_array_addr() {
        // User index 1 maps to internal elem_idx 0.
        let mut vm = VM::new();
        vm.arrays
            .push(ArrayRef::new(vec![Cell::Int(0), Cell::Int(0)]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        array_addr_prim(&mut vm).unwrap();
        assert_eq!(
            vm.pop(),
            Ok(Cell::ArrayAddr {
                pool_idx: 0,
                elem_idx: 0
            })
        );
    }

    #[test]
    fn test_array_addr_prim_zero_index_is_out_of_bounds() {
        // Index 0 is invalid in 1-based indexing.
        let mut vm = VM::new();
        vm.arrays
            .push(ArrayRef::new(vec![Cell::Int(0), Cell::Int(0)]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            array_addr_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: 0, .. })
        ));
    }

    // --- store_prim / set_prim: Cell::Array to DictAddr must be rejected ---

    #[test]
    fn test_store_array_to_dict_addr_is_type_error() {
        // STORE must reject Cell::Array written to a DictAddr (scalar variable) slot.
        let mut vm = VM::new();
        vm.dictionary.push(Cell::None); // dict[0] = placeholder
        vm.push(Cell::Array(0)).unwrap(); // value
        vm.push(Cell::DictAddr(0)).unwrap(); // address
        assert!(
            matches!(
                store_prim(&mut vm),
                Err(TbxError::TypeError { got: "Array", .. })
            ),
            "expected TypeError(Array) from STORE to DictAddr"
        );
    }

    #[test]
    fn test_set_array_to_dict_addr_is_type_error() {
        // SET must reject Cell::Array written to a DictAddr (scalar variable) slot.
        let mut vm = VM::new();
        vm.dictionary.push(Cell::None); // dict[0] = placeholder
                                        // set_prim: stack is [..., addr, value]
        vm.push(Cell::DictAddr(0)).unwrap(); // address
        vm.push(Cell::Array(0)).unwrap(); // value
        assert!(
            matches!(
                set_prim(&mut vm),
                Err(TbxError::TypeError { got: "Array", .. })
            ),
            "expected TypeError(Array) from SET to DictAddr"
        );
    }

    // --- store/set to ArrayAddr ---

    #[test]
    fn test_store_to_array_addr() {
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None, Cell::None]));
        vm.push(Cell::Int(99)).unwrap();
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 1,
        })
        .unwrap();
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0].get_cloned(1), Some(Cell::Int(99)));
    }

    #[test]
    fn test_set_to_array_addr() {
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None, Cell::None]));
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::Int(42)).unwrap();
        set_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0].get_cloned(0), Some(Cell::Int(42)));
    }

    // --- array element write: Cell::Str (D-4: Rc<str> liberation, #591) ---
    //
    // Since #591 (D-4), `Cell::Str` is `Rc<str>`-backed and array element
    // writes accept `Cell::Str` for all array lifetimes (global, caller-owned,
    // frame-local).  No per-source-lifetime classification is needed because
    // the `Rc` handle keeps the string alive independently of any stack frame.

    #[test]
    fn test_set_str_to_array_element_is_allowed() {
        // Cell::Str(Rc<str>) written through SET must succeed (#591).
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None]));
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::string("hello")).unwrap();
        set_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0].get_cloned(0), Some(Cell::string("hello")));
    }

    #[test]
    fn test_store_str_to_array_element_is_allowed() {
        // Same as the SET path above, exercised through STORE (#591).
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None]));
        vm.push(Cell::string("world")).unwrap();
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0].get_cloned(0), Some(Cell::string("world")));
    }

    #[test]
    fn test_set_str_to_global_array_element_is_allowed() {
        // Storing a Str into a global array (global_array_pool_len covers it) must succeed.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None]));
        vm.global_array_pool_len = 1; // mark as global
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::string("global")).unwrap();
        set_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0].get_cloned(0), Some(Cell::string("global")));
    }

    #[test]
    fn test_set_str_to_frame_local_array_element_is_allowed() {
        // Storing a Str into a frame-local array must succeed (#591).
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None]));
        // global_array_pool_len = 0 (default) → array is frame-local
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::string("frame-local")).unwrap();
        set_prim(&mut vm).unwrap();
        assert_eq!(
            vm.arrays[0].get_cloned(0),
            Some(Cell::string("frame-local"))
        );
    }

    #[test]
    fn test_set_nested_array_to_array_element_is_invalid_array_element() {
        // Cell::Array must always be rejected as an array element.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None])); // pool_idx = 0: target array
        vm.arrays.push(ArrayRef::new(vec![Cell::None])); // pool_idx = 1: value to store
        vm.global_array_pool_len = 2; // both are global
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::Array(1)).unwrap();
        assert_eq!(
            set_prim(&mut vm),
            Err(TbxError::InvalidArrayElement { got: "Array" })
        );
    }

    // --- fetch_prim with ArrayAddr ---

    #[test]
    fn test_fetch_array_addr() {
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::Int(77)]));
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        fetch_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(77)));
    }

    // --- to_tuple_prim ---

    #[test]
    fn test_to_tuple_prim_basic() {
        // Stack: [1, 2, 3, Int(3)] → Cell::Tuple([Int(1), Int(2), Int(3)])
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        vm.push(Cell::Int(3)).unwrap(); // arity
        to_tuple_prim(&mut vm).unwrap();
        assert_eq!(
            vm.pop(),
            Ok(Cell::Tuple(vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]))
        );
    }

    #[test]
    fn test_to_tuple_prim_preserves_order() {
        // First argument becomes index 0 (lowest position in the tuple).
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        vm.push(Cell::Int(20)).unwrap();
        vm.push(Cell::Int(30)).unwrap();
        vm.push(Cell::Int(3)).unwrap(); // arity
        to_tuple_prim(&mut vm).unwrap();
        let result = vm.pop().unwrap();
        let Cell::Tuple(elems) = result else {
            panic!("expected Cell::Tuple");
        };
        assert_eq!(elems[0], Cell::Int(10));
        assert_eq!(elems[1], Cell::Int(20));
        assert_eq!(elems[2], Cell::Int(30));
    }

    #[test]
    fn test_to_tuple_prim_single_element() {
        // Stack: [Int(42), Int(1)] → Cell::Tuple([Int(42)])
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        vm.push(Cell::Int(1)).unwrap(); // arity
        to_tuple_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Tuple(vec![Cell::Int(42)])));
    }

    #[test]
    fn test_to_tuple_prim_mixed_types() {
        // Mix of Int, Float, Bool, and Str.
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Float(2.5)).unwrap();
        vm.push(Cell::Bool(true)).unwrap();
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::Int(4)).unwrap(); // arity
        to_tuple_prim(&mut vm).unwrap();
        assert_eq!(
            vm.pop(),
            Ok(Cell::Tuple(vec![
                Cell::Int(1),
                Cell::Float(2.5),
                Cell::Bool(true),
                Cell::string("hello"),
            ]))
        );
    }

    #[test]
    fn test_to_tuple_prim_rejects_array() {
        // Cell::Array is a forbidden tuple element type.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::Int(1)]));
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(1)).unwrap(); // arity
        assert!(matches!(
            to_tuple_prim(&mut vm),
            Err(TbxError::InvalidTupleElement { .. })
        ));
    }

    #[test]
    fn test_to_tuple_prim_negative_arity_returns_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap(); // negative arity
        assert!(matches!(
            to_tuple_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_to_tuple_prim_empty() {
        // Stack: [Int(0)] → Cell::Tuple([]) (empty tuple)
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap(); // arity = 0
        to_tuple_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Tuple(vec![])));
    }

    // --- int_prim ---

    #[test]
    fn test_int_prim_positive_float_truncates() {
        // INT(3.7) => 3 (truncation toward zero)
        let mut vm = VM::new();
        vm.push(Cell::Float(3.7)).unwrap();
        int_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(3)));
    }

    #[test]
    fn test_int_prim_negative_float_truncates_toward_zero() {
        // INT(-3.7) => -3 (truncation toward zero, not floor)
        let mut vm = VM::new();
        vm.push(Cell::Float(-3.7)).unwrap();
        int_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-3)));
    }

    #[test]
    fn test_int_prim_whole_float_returns_int() {
        // INT(3.0) => 3
        let mut vm = VM::new();
        vm.push(Cell::Float(3.0)).unwrap();
        int_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(3)));
    }

    #[test]
    fn test_int_prim_int_identity() {
        // INT(5) => 5 (identity for Cell::Int)
        let mut vm = VM::new();
        vm.push(Cell::Int(5)).unwrap();
        int_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(5)));
    }

    #[test]
    fn test_int_prim_type_error() {
        // INT on a non-numeric type must return TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            int_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Int or Float",
                ..
            })
        ));
    }

    // --- array_len_prim ---

    #[test]
    fn test_array_len_prim_basic() {
        // ARRAY_LEN on a 5-element array must return 5.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None; 5]));
        vm.push(Cell::Array(0)).unwrap();
        array_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(5)));
    }

    #[test]
    fn test_array_len_prim_one_element() {
        // ARRAY_LEN on a 1-element array must return 1.
        let mut vm = VM::new();
        vm.arrays.push(ArrayRef::new(vec![Cell::None]));
        vm.push(Cell::Array(0)).unwrap();
        array_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_array_len_prim_type_error() {
        // ARRAY_LEN on a non-Array cell must return TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        assert!(matches!(
            array_len_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Array",
                ..
            })
        ));
    }

    // --- rnd_prim ---

    #[test]
    fn test_rnd_prim_range() {
        // RND(n) must always return a value in [1, n].
        use rand::SeedableRng;
        let mut vm = VM::new();
        vm.rng = rand::rngs::SmallRng::seed_from_u64(42);
        for _ in 0..100 {
            vm.push(Cell::Int(6)).unwrap();
            rnd_prim(&mut vm).unwrap();
            let result = vm.pop_int().unwrap();
            assert!((1..=6).contains(&result), "RND(6) out of range: {result}");
        }
    }

    #[test]
    fn test_rnd_prim_one() {
        // RND(1) must always return 1.
        use rand::SeedableRng;
        let mut vm = VM::new();
        vm.rng = rand::rngs::SmallRng::seed_from_u64(0);
        for _ in 0..10 {
            vm.push(Cell::Int(1)).unwrap();
            rnd_prim(&mut vm).unwrap();
            assert_eq!(vm.pop(), Ok(Cell::Int(1)));
        }
    }

    #[test]
    fn test_rnd_prim_zero_error() {
        // RND(0) must return InvalidArgument.
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            rnd_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_rnd_prim_negative_error() {
        // RND(-1) must return InvalidArgument.
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap();
        assert!(matches!(
            rnd_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    // --- randomize_prim ---

    #[test]
    fn test_randomize_prim_no_error() {
        // RANDOMIZE must complete without error and leave the stack unchanged.
        let mut vm = VM::new();
        randomize_prim(&mut vm).unwrap();
        assert_eq!(vm.data_stack.len(), 0);
    }

    // --- unixtime_prim ---

    #[test]
    fn test_unixtime_returns_positive_float() {
        // UNIXTIME must push a positive Float (seconds since Unix epoch).
        let mut vm = VM::new();
        unixtime_prim(&mut vm).unwrap();
        match vm.pop().unwrap() {
            Cell::Float(f) => assert!(f > 0.0, "UNIXTIME must be positive, got {f}"),
            other => panic!("expected Float, got {other:?}"),
        }
    }

    // --- hour_prim ---

    // Unix timestamp 1_700_000_000 is 2023-11-14 22:13:20 UTC.
    // (1_700_000_000 / 3600) % 24 = 22
    #[test]
    fn test_hour_known_timestamp() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1_700_000_000.0)).unwrap();
        hour_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(22)));
    }

    #[test]
    fn test_hour_accepts_int() {
        // INT input must be promoted and yield the same result as Float.
        let mut vm = VM::new();
        vm.push(Cell::Int(1_700_000_000)).unwrap();
        hour_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(22)));
    }

    #[test]
    fn test_hour_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        assert!(matches!(
            hour_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- minute_prim ---

    // 1_700_000_000 = 28333333 minutes + 20 s  →  (28333333) % 60 = 13
    #[test]
    fn test_minute_known_timestamp() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1_700_000_000.0)).unwrap();
        minute_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(13)));
    }

    #[test]
    fn test_minute_accepts_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1_700_000_000)).unwrap();
        minute_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(13)));
    }

    #[test]
    fn test_minute_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(false)).unwrap();
        assert!(matches!(
            minute_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- second_prim ---

    // 1_700_000_000 % 60 = 20, fract = 0.0  →  20.0
    #[test]
    fn test_second_known_timestamp() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1_700_000_000.0)).unwrap();
        second_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(20.0)));
    }

    #[test]
    fn test_second_preserves_fractional_part() {
        // 1_700_000_000.75 → integer part 1_700_000_000 → seconds = 20, fract = 0.75
        let mut vm = VM::new();
        vm.push(Cell::Float(1_700_000_000.75)).unwrap();
        second_prim(&mut vm).unwrap();
        match vm.pop().unwrap() {
            Cell::Float(f) => {
                assert!((f - 20.75).abs() < 1e-9, "expected ≈20.75, got {f}");
            }
            other => panic!("expected Float, got {other:?}"),
        }
    }

    #[test]
    fn test_second_accepts_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(1_700_000_000)).unwrap();
        second_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Float(20.0)));
    }

    #[test]
    fn test_second_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::string("not a number")).unwrap();
        assert!(matches!(
            second_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_get_output_takes_and_clears_output_buffer() {
        let mut vm = VM::new();
        vm.write_output("hello");

        get_output_prim(&mut vm).unwrap();

        let cell = vm.pop().unwrap();
        assert_eq!(cell.as_str().map(|s| s.as_ref()), Some("hello"));
        // Buffer should be empty after GET_OUTPUT consumes it.
        assert_eq!(vm.take_output(), "");
    }

    #[test]
    fn test_get_output_empty_buffer_returns_empty_string() {
        let mut vm = VM::new();

        get_output_prim(&mut vm).unwrap();

        let cell = vm.pop().unwrap();
        assert_eq!(cell.as_str().map(|s| s.as_ref()), Some(""));
    }

    #[test]
    fn test_get_output_returns_str_cell() {
        let mut vm = VM::new();
        vm.write_output("test");

        get_output_prim(&mut vm).unwrap();

        let cell = vm.pop().unwrap();
        assert!(matches!(cell, Cell::Str(_)));
    }
}
