use crate::cell::Cell;
use crate::constants::MAX_DICTIONARY_CELLS;
use crate::dict::{EntryKind, WordEntry, FLAG_IMMEDIATE, FLAG_SYSTEM};
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::lexer::Token;
use crate::vm::{CompileState, VM};
use std::collections::HashMap;

/// DROP — discard the top element of the data stack.
pub fn drop_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.pop()?;
    Ok(())
}

/// LIT_MARKER — push a Cell::Marker sentinel onto the data stack.
pub fn lit_marker_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::Marker)?;
    Ok(())
}

/// DUP — duplicate the top element of the data stack.
pub fn dup_prim(vm: &mut VM) -> Result<(), TbxError> {
    let top = vm.pop()?;
    vm.push(top.clone())?;
    vm.push(top)?;
    Ok(())
}

/// SWAP — exchange the top two elements of the data stack.
pub fn swap_prim(vm: &mut VM) -> Result<(), TbxError> {
    let a = vm.pop()?;
    let b = vm.pop()?;
    vm.push(a)?;
    vm.push(b)?;
    Ok(())
}

/// FETCH — fetch a value from an address and push it onto the stack.
pub fn fetch_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            let value = vm.dict_read(a)?;
            vm.push(value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            let value = vm.local_read(a)?;
            vm.push(value)?;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// STORE — pop addr (top) then value (below), and store value at addr.
///
/// Stack convention: `[..., value, addr]` → STORE → `[...]`
pub fn store_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    let value = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            vm.local_write(a, value)?;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// LET — pop value (top) then addr (below), and store value at addr.
///
/// Designed for the `LET &var, value` statement pattern where `&var` is
/// pushed before `value` (left-to-right argument evaluation via comma).
/// Stack convention: `[..., addr, value]` → LET → `[...]`
pub fn let_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            vm.local_write(a, value)?;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

pub fn add_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_add(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x + y))?,
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 + y))?,
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x + y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

pub fn sub_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_sub(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x - y))?,
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 - y))?,
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x - y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

pub fn mul_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_mul(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x * y))?,
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 * y))?,
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x * y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

#[allow(clippy::redundant_guards)] // Float(0.0) pattern also matches -0.0; use guard for clarity
pub fn div_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_div(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(_), Cell::Float(y)) if y == 0.0 => return Err(TbxError::DivisionByZero),
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x / y))?,
        (Cell::Int(_), Cell::Float(y)) if y == 0.0 => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 / y))?,
        (Cell::Float(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x / y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

pub fn mod_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_int()?;
    let a = vm.pop_int()?;
    if b == 0 {
        return Err(TbxError::DivisionByZero);
    }
    let result = a.checked_rem(b).ok_or(TbxError::IntegerOverflow)?;
    vm.push(Cell::Int(result))?;
    Ok(())
}

/// EQ — equality comparison. Pushes Bool(true) if the two top values are equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
pub fn eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) == *y,
        (Cell::Float(x), Cell::Int(y)) => *x == (*y as f64),
        _ => a == b,
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// NEQ — inequality comparison. Pushes Bool(true) if the two top values are not equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
pub fn neq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) != *y,
        (Cell::Float(x), Cell::Int(y)) => *x != (*y as f64),
        _ => a != b,
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// LT — less than. Pushes Bool(true) if a < b (numeric only, with Int/Float promotion).
pub fn lt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x < y,
        (Cell::Float(x), Cell::Float(y)) => x < y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) < *y,
        (Cell::Float(x), Cell::Int(y)) => *x < (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// GT — greater than. Pushes Bool(true) if a > b (numeric only, with Int/Float promotion).
pub fn gt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x > y,
        (Cell::Float(x), Cell::Float(y)) => x > y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) > *y,
        (Cell::Float(x), Cell::Int(y)) => *x > (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// LE — less than or equal. Pushes Bool(true) if a <= b (numeric only, with Int/Float promotion).
pub fn le_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x <= y,
        (Cell::Float(x), Cell::Float(y)) => x <= y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) <= *y,
        (Cell::Float(x), Cell::Int(y)) => *x <= (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// GE — greater than or equal. Pushes Bool(true) if a >= b (numeric only, with Int/Float promotion).
pub fn ge_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x >= y,
        (Cell::Float(x), Cell::Float(y)) => x >= y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) >= *y,
        (Cell::Float(x), Cell::Int(y)) => *x >= (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// AND — logical AND. Evaluates both operands with is_truthy() and pushes the result as Bool.
pub fn and_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a.is_truthy() && b.is_truthy()))?;
    Ok(())
}

/// OR — logical OR. Evaluates both operands with is_truthy() and pushes the result as Bool.
pub fn or_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a.is_truthy() || b.is_truthy()))?;
    Ok(())
}

/// BAND — bitwise AND. Both operands must be Int.
pub fn band_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_int()?;
    let a = vm.pop_int()?;
    vm.push(Cell::Int(a & b))?;
    Ok(())
}

/// BOR — bitwise OR. Both operands must be Int.
pub fn bor_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_int()?;
    let a = vm.pop_int()?;
    vm.push(Cell::Int(a | b))?;
    Ok(())
}

/// PUTSTR — output the string referenced by a StringDesc on the stack (no newline).
/// Escape sequences (\n, \t, \\) in the stored string are output literally
/// as they were already expanded at compile time (during intern).
pub fn putstr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let idx = vm.pop_string_desc()?;
    let s = vm.resolve_string(idx)?;
    vm.write_output(&s);
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

/// PUTDEC — output the integer value on the stack as a signed decimal number (no newline).
pub fn putdec_prim(vm: &mut VM) -> Result<(), TbxError> {
    let n = vm.pop_int()?;
    vm.write_output(&n.to_string());
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

/// NEGATE — negate the numeric value on top of the data stack.
///
/// - `Cell::Int(n)` → `Cell::Int(-n)` (returns `IntegerOverflow` for `i64::MIN`)
/// - `Cell::Float(v)` → `Cell::Float(-v)`
/// - any other type → `TbxError::TypeError`
pub fn negate_prim(vm: &mut VM) -> Result<(), TbxError> {
    let val = vm.pop()?;
    match val {
        Cell::Int(n) => {
            let result = n.checked_neg().ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        Cell::Float(v) => {
            vm.push(Cell::Float(-v))?;
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
    let tok = vm.next_token()?;
    let name = match tok.token {
        Token::Ident(n) => n,
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "HEADER: expected identifier token",
            })
        }
    };
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
    let tok = vm.next_token()?;
    let name = match tok.token {
        Token::Ident(n) => n,
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "IMMEDIATE: expected identifier token",
            })
        }
    };
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
    let name_tok = vm.next_token()?;
    let name = match name_tok.token {
        crate::lexer::Token::Ident(n) => n,
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "expected word name after DEF",
            })
        }
    };

    // Parse optional parameter list: DEF WORD(X, Y, ...)
    //
    // DFA with 4 states:
    //   LParenOrEnd      — after word name: expect '(' or EOL
    //   FirstParamOrEnd  — right after '(': expect ident or ')'  (comma here = leading-comma error)
    //   CommaOrRParen    — after registering a param: expect ',' or ')'  (ident here = missing-comma error)
    //   NextParam        — after ',': next must be ident  (')' = trailing-comma error)
    enum DefParseState {
        LParenOrEnd,
        FirstParamOrEnd,
        CommaOrRParen,
        NextParam,
    }

    let mut local_table: HashMap<String, usize> = HashMap::new();
    let mut arity: usize = 0;
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
                    local_table.insert(param, arity);
                    arity += 1;
                    state = DefParseState::CommaOrRParen;
                }
                (DefParseState::FirstParamOrEnd, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected identifier or ')' after '('",
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
                    local_table.insert(param, arity);
                    arity += 1;
                    state = DefParseState::CommaOrRParen;
                }
                (DefParseState::NextParam, crate::lexer::Token::RParen) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "trailing comma before ')' is not allowed",
                    });
                }
                (DefParseState::NextParam, _) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "expected identifier after ',' in parameter list",
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
    ));

    Ok(())
}

/// END — finish compiling the current word definition.
pub fn end_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "END outside DEF",
        });
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
    // Update word header: confirm arity, local_count, unsmudge.
    let word_hdr_idx = state.word_hdr_idx();
    if word_hdr_idx < vm.headers.len() {
        vm.headers[word_hdr_idx].arity = state.arity;
        vm.headers[word_hdr_idx].local_count = state.local_count;
        vm.headers[word_hdr_idx].flags &= !crate::dict::FLAG_HIDDEN;
    }

    vm.seal_user();
    vm.is_compiling = false;

    Ok(())
}

/// VAR — declare a local variable (in compile mode) or global variable (in execute mode).
pub fn var_prim(vm: &mut VM) -> Result<(), TbxError> {
    let name_tok = vm.next_token()?;
    let name = match name_tok.token {
        crate::lexer::Token::Ident(n) => n,
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "expected variable name after VAR",
            })
        }
    };

    if vm.is_compiling {
        // Local variable: add to compile state's local table.
        let state = vm
            .compile_state
            .as_mut()
            .ok_or(TbxError::InvalidExpression {
                reason: "VAR in compile mode but no compile_state",
            })?;
        let idx = state.arity + state.local_count;
        state.local_table.insert(name, idx);
        state.local_count += 1;
    } else {
        // Global variable: allocate storage in dictionary.
        let storage_idx = vm.dp;
        vm.dict_write(Cell::None)?;
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
        vm.dict_write(Cell::Int(target as i64))?;
    } else {
        let patch_pos = vm.dp;
        vm.dict_write(Cell::Int(0))?;
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

/// Register all stack primitives into the VM's dictionary.
pub fn register_all(vm: &mut VM) {
    vm.register(WordEntry::new_primitive("DROP", drop_prim));
    vm.register(WordEntry::new_primitive("DUP", dup_prim));
    vm.register(WordEntry::new_primitive("SWAP", swap_prim));
    vm.register(WordEntry::new_primitive("FETCH", fetch_prim));
    vm.register(WordEntry::new_primitive("STORE", store_prim));
    vm.register(WordEntry::new_primitive("LET", let_prim));
    vm.register(WordEntry::new_primitive("ADD", add_prim));
    vm.register(WordEntry::new_primitive("SUB", sub_prim));
    vm.register(WordEntry::new_primitive("MUL", mul_prim));
    vm.register(WordEntry::new_primitive("DIV", div_prim));
    vm.register(WordEntry::new_primitive("MOD", mod_prim));
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
    vm.register(WordEntry::new_primitive("PUTSTR", putstr_prim));
    vm.register(WordEntry::new_primitive("PUTCHR", putchr_prim));
    vm.register(WordEntry::new_primitive("PUTDEC", putdec_prim));
    vm.register(WordEntry::new_primitive("PUTHEX", puthex_prim));
    vm.register(WordEntry::new_primitive("APPEND", append_prim));
    vm.register(WordEntry::new_primitive("ALLOT", allot_prim));
    vm.register(WordEntry::new_primitive("HERE", here_prim));
    vm.register(WordEntry::new_primitive("STATE", state_prim));
    vm.register(WordEntry::new_primitive("HALT", halt_prim));
    vm.register(WordEntry {
        name: "CALL".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Call,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    vm.register(WordEntry {
        name: "EXIT".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Exit,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    vm.register(WordEntry {
        name: "RETURN_VAL".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::ReturnVal,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    vm.register(WordEntry {
        name: "DROP_TO_MARKER".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::DropToMarker,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    vm.register(WordEntry {
        name: "GOTO".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Goto,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    vm.register(WordEntry {
        name: "BIF".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::BranchIfFalse,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    vm.register(WordEntry {
        name: "BIT".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::BranchIfTrue,
        arity: 0,
        local_count: 0,
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
        prev: None,
    });
    // LITERAL: system-internal compile-time primitive.
    // Not IMMEDIATE — it must not be caught by the interpreter's IMMEDIATE dispatch,
    // because it reads its argument from the data stack (not from the token stream).
    // FLAG_SYSTEM prevents it from being called as a user statement word.
    let mut literal_entry = WordEntry::new_primitive("LITERAL", literal_prim);
    literal_entry.flags = FLAG_SYSTEM;
    vm.register(literal_entry);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
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

    // --- PUTSTR tests ---

    #[test]
    fn test_putstr_basic() {
        let mut vm = VM::new();
        let idx = vm.intern_string("hello").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
        putstr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "hello");
    }

    #[test]
    fn test_putstr_empty() {
        let mut vm = VM::new();
        let idx = vm.intern_string("").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
        putstr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "");
    }

    #[test]
    fn test_putstr_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        assert!(matches!(
            putstr_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    #[test]
    fn test_putstr_underflow() {
        let mut vm = VM::new();
        assert!(matches!(
            putstr_prim(&mut vm),
            Err(TbxError::StackUnderflow)
        ));
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
    fn test_putdec_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Float(3.5)).unwrap();
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
        let lparen = crate::lexer::SpannedToken {
            token: crate::lexer::Token::LParen,
            pos: crate::lexer::Position { line: 1, col: 5 },
            source_offset: 4,
            source_len: 1,
        };
        let comma = crate::lexer::SpannedToken {
            token: crate::lexer::Token::Comma,
            pos: crate::lexer::Position { line: 1, col: 7 },
            source_offset: 6,
            source_len: 1,
        };
        let rparen = crate::lexer::SpannedToken {
            token: crate::lexer::Token::RParen,
            pos: crate::lexer::Position { line: 1, col: 9 },
            source_offset: 8,
            source_len: 1,
        };
        vm.token_stream = Some(VecDeque::from([
            make_ident_token("WORD"),
            lparen,
            make_ident_token("X"),
            comma,
            make_ident_token("Y"),
            rparen,
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

    // --- goto_prim normal case ---

    #[test]
    fn test_goto_prim_writes_dict() {
        // GOTO 10 inside DEF should write [Xt(goto_rt), Int(0)] to the dictionary
        // (forward reference: label not yet seen, so placeholder Int(0) is emitted
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
        // dict[dp_before] = Xt(goto runtime entry), dict[dp_before+1] = Int(0) placeholder.
        let goto_cell = vm.dict_read(dp_before).unwrap();
        let target_cell = vm.dict_read(dp_before + 1).unwrap();
        assert!(
            matches!(goto_cell, Cell::Xt(_)),
            "expected Xt for GOTO opcode, got {:?}",
            goto_cell
        );
        assert_eq!(
            target_cell,
            Cell::Int(0),
            "expected forward-ref placeholder Int(0)"
        );
        // patch_list should record the forward reference.
        let state = vm.compile_state.as_ref().unwrap();
        assert_eq!(state.patch_list, vec![(10, dp_before + 1)]);
    }

    // --- bif_prim normal case ---

    #[test]
    fn test_bif_prim_writes_dict() {
        // BIF 1, 20 inside DEF should compile condition (LIT, Int(1)),
        // then emit [Xt(bif_rt), Int(0)] as a forward reference placeholder.
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
        // Condition expression for literal 1: [Xt(LIT), Int(1)] then [Xt(bif_rt), Int(0)].
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
            Cell::Int(0),
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
        // BIT 1, 30 inside DEF should compile condition then emit [Xt(bit_rt), Int(0)].
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
        // [Xt(LIT), Int(1), Xt(bit_rt), Int(0)]
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
            Cell::Int(0),
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
}
