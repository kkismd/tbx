use crate::cell::{Cell, CompileEntry};
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

/// SET — pop value (top) then addr (below), and store value at addr.
///
/// Designed for the `SET &var, value` statement pattern where `&var` is
/// pushed before `value` (left-to-right argument evaluation via comma).
/// Stack convention: `[..., addr, value]` → SET → `[...]`
pub fn set_prim(vm: &mut VM) -> Result<(), TbxError> {
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

/// ASSERT_FAIL — raise an AssertionFailed error unconditionally.
pub fn assert_fail_prim(_vm: &mut VM) -> Result<(), TbxError> {
    Err(TbxError::AssertionFailed)
}

/// ASSERT_FAIL_MSG — pop a string message from the stack and raise AssertionFailedWithMessage.
pub fn assert_fail_msg_prim(vm: &mut VM) -> Result<(), TbxError> {
    let idx = vm.pop_string_desc()?;
    let message = vm.resolve_string(idx)?;
    Err(TbxError::AssertionFailedWithMessage { message })
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
        Token::Ident(n) => n.to_ascii_uppercase(),
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
        Token::Ident(n) => n.to_ascii_uppercase(),
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
        crate::lexer::Token::Ident(n) => n.to_ascii_uppercase(),
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
                    local_table.insert(param.to_ascii_uppercase(), arity);
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
        crate::lexer::Token::Ident(n) => n.to_ascii_uppercase(),
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

/// DIM — declare a global array. Syntax: `DIM NAME(SIZE)`.
///
/// Allocates `SIZE` cells of `Cell::None` in the dictionary and registers the
/// array under `NAME` as an `EntryKind::Array` entry.
///
/// This word is IMMEDIATE and executes at read time (like VAR).
/// Using DIM inside a DEF..END block is not allowed.
pub fn dim_prim(vm: &mut VM) -> Result<(), TbxError> {
    if vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "DIM is not allowed inside DEF",
        });
    }

    // Read array name.
    let tok = vm.next_token()?;
    let name = match tok.token {
        Token::Ident(n) => n.to_ascii_uppercase(),
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "expected array name after DIM",
            })
        }
    };

    // Read '('.
    let tok = vm.next_token()?;
    if !matches!(tok.token, Token::LParen) {
        return Err(TbxError::InvalidExpression {
            reason: "expected '(' after DIM NAME",
        });
    }

    // Read size (must be a positive integer literal).
    let tok = vm.next_token()?;
    let size = match tok.token {
        Token::IntLit(n) if n <= 0 => return Err(TbxError::InvalidAllotCount),
        Token::IntLit(n) => n as usize,
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "expected positive integer size in DIM NAME(SIZE)",
            })
        }
    };

    // Read ')'.
    let tok = vm.next_token()?;
    if !matches!(tok.token, Token::RParen) {
        return Err(TbxError::InvalidExpression {
            reason: "expected ')' after DIM NAME(SIZE)",
        });
    }

    // Check that the allocation fits within the dictionary limit.
    // Use saturating_add to guard against usize overflow when dp + size > usize::MAX.
    // In practice this cannot occur given MAX_DICTIONARY_CELLS = 1_048_576, but the
    // guard makes the overflow behaviour explicit rather than relying on wrapping.
    let new_dp = vm.dp.saturating_add(size);
    if new_dp > MAX_DICTIONARY_CELLS {
        return Err(TbxError::DictionaryOverflow {
            requested: new_dp,
            limit: MAX_DICTIONARY_CELLS,
        });
    }

    // Allocate storage and register the array entry.
    let base = vm.dp;
    for _ in 0..size {
        vm.dict_write(Cell::None)?;
    }
    let entry = crate::dict::WordEntry::new_array(&name, base, size);
    vm.register(entry);
    vm.seal_user();
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

/// CS_OPEN_TAG — pop a StringDesc from the data stack and push a `CompileEntry::Tag`
/// onto the compile stack.
///
/// Used by IMMEDIATE words (e.g. WHILE, IF) to mark the start of a control-structure
/// scope.  The string (e.g. `"WHILE"` or `"IF"`) is matched by a later CS_CLOSE_TAG
/// call to validate correct nesting.
/// Must be called in compile mode.
fn cs_open_tag_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_OPEN_TAG outside compile mode",
        });
    }
    let idx = vm.pop_string_desc()?;
    let tag = vm.resolve_string(idx)?;
    vm.compile_stack.push(CompileEntry::Tag(tag));
    Ok(())
}

/// CS_CLOSE_TAG — pop a StringDesc from the data stack, then validate and remove the
/// matching `CompileEntry::Tag` from the top of the compile stack.
///
/// Returns `NoOpenTag` if the compile stack is empty or its top entry is a `Cell`
/// (not a `Tag`).  Returns `MismatchedTag` if the top is a `Tag` but does not match
/// the expected string.
/// Must be called in compile mode.
fn cs_close_tag_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "CS_CLOSE_TAG outside compile mode",
        });
    }
    let idx = vm.pop_string_desc()?;
    let expected = vm.resolve_string(idx)?;
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

    let tok = vm.next_token()?;
    match tok.token {
        Token::Comma => Ok(()),
        _ => Err(TbxError::InvalidExpression {
            reason: "SKIP_COMMA: expected ','",
        }),
    }
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
    let amp_tok = vm.next_token()?;
    if !matches!(amp_tok.token, Token::Ampersand) {
        return Err(TbxError::InvalidExpression {
            reason: "COMPILE_LVALUE_SAVE: expected '&' before variable name",
        });
    }

    let tok = vm.next_token()?;
    let name = match tok.token {
        Token::Ident(n) => n.to_ascii_uppercase(),
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "COMPILE_LVALUE_SAVE: expected variable name",
            })
        }
    };

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

/// COMPILE_LVALUE — read a variable name from the token stream and emit `LIT addr` to the
/// dictionary, where `addr` is the variable's stack or dictionary address.
///
/// This is the compile-time counterpart to the `&var` address-of operator in expressions.
/// Locals (from `compile_state.local_table`) resolve to `StackAddr`; global variables
/// (`EntryKind::Variable`) resolve to `DictAddr`.
///
/// Must be called in compile mode (inside an IMMEDIATE word invocation that runs during a
/// DEF body compilation). Requires `token_stream` to be set.
fn compile_lvalue_prim(vm: &mut VM) -> Result<(), TbxError> {
    if !vm.is_compiling {
        return Err(TbxError::InvalidExpression {
            reason: "COMPILE_LVALUE outside compile mode",
        });
    }

    let tok = vm.next_token()?;
    let name = match tok.token {
        Token::Ident(n) => n.to_ascii_uppercase(),
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "COMPILE_LVALUE: expected variable name",
            })
        }
    };

    // Resolve address: local table first, then global dictionary.
    // Follow the same take→use→restore→apply-? pattern as compile_expr_prim so that
    // local_table is always restored before any early return propagates.
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
    let lit_xt =
        vm.find_by_kind(|k| matches!(k, EntryKind::Lit))
            .ok_or(TbxError::UndefinedSymbol {
                name: "LIT".to_string(),
            })?;
    vm.dict_write(Cell::Xt(lit_xt))?;
    vm.dict_write(addr_cell)?;
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

    let tok = vm.next_token()?;
    match tok.token {
        Token::Op(ref s) if s == "=" => Ok(()),
        _ => Err(TbxError::InvalidExpression {
            reason: "SKIP_EQ: expected '='",
        }),
    }
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

    let tok = vm.next_token()?;
    let path = match tok.token {
        crate::lexer::Token::StringLit(p) => p,
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "USE expects a string literal as its argument",
            })
        }
    };

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

/// ACCEPT — read one line from the VM's input source and store it in the input buffer.
///
/// Reads until a newline (or EOF) and strips the trailing newline characters.
/// Each call overwrites any previously buffered input.
/// Stack signature: `( -- )`
pub fn accept_prim(vm: &mut VM) -> Result<(), TbxError> {
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
    vm.input_buffer = Some(trimmed);
    Ok(())
}

/// GETDEC — consume the input buffer and push its integer value onto the data stack.
///
/// Parses the string stored by the most recent ACCEPT call as a signed decimal integer
/// (leading/trailing whitespace is ignored) and pushes the result as `Cell::Int`.
///
/// Returns `TbxError::InputBufferEmpty` if ACCEPT has not been called since the last
/// GETDEC (or since VM creation), and `TbxError::ParseIntError` if the string cannot
/// be parsed as a signed decimal integer.
///
/// Stack signature: `( -- n )`
pub fn getdec_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s = vm.input_buffer.take().ok_or(TbxError::InputBufferEmpty)?;
    let n = s
        .trim()
        .parse::<i64>()
        .map_err(|_| TbxError::ParseIntError { input: s })?;
    vm.push(Cell::Int(n))
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
    vm.register(WordEntry::new_primitive("ACCEPT", accept_prim));
    vm.register(WordEntry::new_primitive("GETDEC", getdec_prim));
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
    // OFFSET: system-internal instruction handled by the inner interpreter.
    // Pops an index from the data stack, bounds-checks it against inline base/size
    // operands, and pushes the element address.
    vm.register(WordEntry {
        name: "OFFSET".to_string(),
        flags: FLAG_SYSTEM,
        kind: EntryKind::Offset,
        arity: 0,
        local_count: 0,
        prev: None,
    });
    // DIM: IMMEDIATE so the outer interpreter feeds the token stream before calling it.
    // FLAG_SYSTEM marks it as a system word consistent with other compile-time declarations.
    let mut dim_entry = WordEntry::new_primitive("DIM", dim_prim);
    dim_entry.flags = FLAG_IMMEDIATE | FLAG_SYSTEM;
    vm.register(dim_entry);

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

    // ASSIGN_XT: constant holding the Xt of the SET primitive.
    // Allows TBX compile words to emit a SET instruction via `APPEND ASSIGN_XT`,
    // analogous to JUMP_FALSE/JUMP_TRUE/JUMP_ALWAYS for branch instructions.
    let set_xt = vm
        .lookup("SET")
        .expect("SET primitive must be registered before ASSIGN_XT");
    vm.register(WordEntry::new_constant("ASSIGN_XT", Cell::Xt(set_xt)));

    // FOR/NEXT compile-helper primitives.
    // These are used inside IMMEDIATE word bodies (FOR, NEXT) defined in basic.tbx.
    vm.register(WordEntry::new_primitive("SKIP_COMMA", skip_comma_prim));
    vm.register(WordEntry::new_primitive(
        "COMPILE_LVALUE_SAVE",
        compile_lvalue_save_prim,
    ));

    // Runtime Xt constants for arithmetic/comparison instructions used by FOR/NEXT.
    // Analogous to ASSIGN_XT (SET) and JUMP_FALSE/JUMP_ALWAYS for control flow.
    let fetch_xt = vm
        .lookup("FETCH")
        .expect("FETCH primitive must be registered before FETCH_XT");
    vm.register(WordEntry::new_constant("FETCH_XT", Cell::Xt(fetch_xt)));

    let add_xt = vm
        .lookup("ADD")
        .expect("ADD primitive must be registered before ADD_XT");
    vm.register(WordEntry::new_constant("ADD_XT", Cell::Xt(add_xt)));

    let le_xt = vm
        .lookup("LE")
        .expect("LE primitive must be registered before LE_XT");
    vm.register(WordEntry::new_constant("LE_XT", Cell::Xt(le_xt)));
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
        let idx = vm.intern_string("SIGN(7) should be 1").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        let idx = vm.intern_string("msg").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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

    // ------------------------------------------------------------------
    // DIM primitive tests
    // ------------------------------------------------------------------

    /// Helper: build a minimal execution-mode VM with all primitives registered.
    fn make_exec_vm() -> VM {
        let mut vm = VM::new();
        register_all(&mut vm);
        vm
    }

    #[test]
    fn test_dim_allocates_storage_and_registers_array() {
        use std::collections::VecDeque;
        let mut vm = make_exec_vm();

        // DIM NUMS(3) — should allocate 3 cells and register an Array entry.
        vm.token_stream = Some(VecDeque::from([
            crate::lexer::SpannedToken {
                token: Token::Ident("NUMS".to_string()),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 4,
            },
            crate::lexer::SpannedToken {
                token: Token::LParen,
                pos: crate::lexer::Position { line: 1, col: 5 },
                source_offset: 4,
                source_len: 1,
            },
            crate::lexer::SpannedToken {
                token: Token::IntLit(3),
                pos: crate::lexer::Position { line: 1, col: 6 },
                source_offset: 5,
                source_len: 1,
            },
            crate::lexer::SpannedToken {
                token: Token::RParen,
                pos: crate::lexer::Position { line: 1, col: 7 },
                source_offset: 6,
                source_len: 1,
            },
        ]));

        let dp_before = vm.dp;
        dim_prim(&mut vm).unwrap();

        // dp should have advanced by 3.
        assert_eq!(vm.dp, dp_before + 3, "dp should advance by 3");

        // Each allocated cell must be Cell::None.
        for offset in 0..3 {
            assert_eq!(
                vm.dict_read(dp_before + offset).unwrap(),
                Cell::None,
                "allocated cell at offset {offset} should be None"
            );
        }

        // The word "NUMS" must be visible as an Array entry.
        let xt = vm.lookup("NUMS").expect("NUMS should be registered");
        assert!(
            matches!(
                vm.headers[xt.index()].kind,
                crate::dict::EntryKind::Array { base, size } if base == dp_before && size == 3
            ),
            "expected Array {{ base: {dp_before}, size: 3 }}, got {:?}",
            vm.headers[xt.index()].kind
        );
    }

    #[test]
    fn test_dim_zero_size_returns_error() {
        use std::collections::VecDeque;
        let mut vm = make_exec_vm();

        vm.token_stream = Some(VecDeque::from([
            crate::lexer::SpannedToken {
                token: Token::Ident("BUF".to_string()),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 3,
            },
            crate::lexer::SpannedToken {
                token: Token::LParen,
                pos: crate::lexer::Position { line: 1, col: 4 },
                source_offset: 3,
                source_len: 1,
            },
            crate::lexer::SpannedToken {
                token: Token::IntLit(0),
                pos: crate::lexer::Position { line: 1, col: 5 },
                source_offset: 4,
                source_len: 1,
            },
            crate::lexer::SpannedToken {
                token: Token::RParen,
                pos: crate::lexer::Position { line: 1, col: 6 },
                source_offset: 5,
                source_len: 1,
            },
        ]));

        let err = dim_prim(&mut vm).unwrap_err();
        assert_eq!(
            err,
            TbxError::InvalidAllotCount,
            "DIM with size 0 should return InvalidAllotCount"
        );
    }

    #[test]
    fn test_dim_inside_def_returns_error() {
        let mut vm = make_compiling_vm("MYWORD");
        // token_stream doesn't matter — DIM should refuse inside DEF.
        use std::collections::VecDeque;
        vm.token_stream = Some(VecDeque::new());
        let err = dim_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("DIM")),
            "expected InvalidExpression mentioning DIM, got {err:?}"
        );
    }

    #[test]
    fn test_dim_dictionary_overflow() {
        use std::collections::VecDeque;
        let mut vm = make_exec_vm();

        // Fill the dictionary so that only 2 cells remain.
        while vm.dp + 2 < MAX_DICTIONARY_CELLS {
            vm.dict_write(Cell::None).unwrap();
        }

        // DIM BUF(3) — requires 3 cells but only 2 are available.
        vm.token_stream = Some(VecDeque::from([
            crate::lexer::SpannedToken {
                token: Token::Ident("BUF".to_string()),
                pos: crate::lexer::Position { line: 1, col: 1 },
                source_offset: 0,
                source_len: 3,
            },
            crate::lexer::SpannedToken {
                token: Token::LParen,
                pos: crate::lexer::Position { line: 1, col: 4 },
                source_offset: 3,
                source_len: 1,
            },
            crate::lexer::SpannedToken {
                token: Token::IntLit(3),
                pos: crate::lexer::Position { line: 1, col: 5 },
                source_offset: 4,
                source_len: 1,
            },
            crate::lexer::SpannedToken {
                token: Token::RParen,
                pos: crate::lexer::Position { line: 1, col: 6 },
                source_offset: 5,
                source_len: 1,
            },
        ]));

        let err = dim_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::DictionaryOverflow { .. }),
            "DIM with insufficient space should return DictionaryOverflow, got {err:?}"
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
        let idx = vm.intern_string("IF").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
        let err = cs_open_tag_prim(&mut vm).unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn test_cs_open_tag_pushes_tag_to_compile_stack() {
        // CS_OPEN_TAG must pop a StringDesc, resolve it and push Tag to compile_stack.
        let mut vm = make_compiling_vm("TESTWORD");
        let idx = vm.intern_string("WHILE").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        // CS_OPEN_TAG with a non-StringDesc on the data stack must return TypeError.
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
        let idx = vm.intern_string("IF").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        let idx = vm.intern_string("WHILE").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        let idx = vm.intern_string("WHILE").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        let idx = vm.intern_string("WHILE").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        let idx = vm.intern_string("IF").unwrap();
        vm.push(Cell::StringDesc(idx)).unwrap();
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
        let idx_if = vm.intern_string("IF").unwrap();
        let idx_while = vm.intern_string("WHILE").unwrap();

        vm.push(Cell::StringDesc(idx_if)).unwrap();
        cs_open_tag_prim(&mut vm).unwrap(); // push Tag("IF")

        vm.push(Cell::StringDesc(idx_while)).unwrap();
        cs_open_tag_prim(&mut vm).unwrap(); // push Tag("WHILE")

        // Close WHILE
        let idx_while2 = vm.intern_string("WHILE").unwrap();
        vm.push(Cell::StringDesc(idx_while2)).unwrap();
        cs_close_tag_prim(&mut vm).unwrap(); // pop Tag("WHILE")

        // Close IF
        let idx_if2 = vm.intern_string("IF").unwrap();
        vm.push(Cell::StringDesc(idx_if2)).unwrap();
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
    fn test_accept_stores_line_in_input_buffer() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("hello\n"));
        accept_prim(&mut vm).unwrap();
        assert_eq!(vm.input_buffer, Some("hello".to_string()));
    }

    #[test]
    fn test_accept_strips_trailing_newline() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("world\r\n"));
        accept_prim(&mut vm).unwrap();
        assert_eq!(vm.input_buffer, Some("world".to_string()));
    }

    #[test]
    fn test_accept_overwrites_previous_buffer() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_buffer = Some("old value".to_string());
        vm.input_reader = Box::new(Cursor::new("new value\n"));
        accept_prim(&mut vm).unwrap();
        assert_eq!(vm.input_buffer, Some("new value".to_string()));
    }

    #[test]
    fn test_accept_does_not_push_to_stack() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("42\n"));
        accept_prim(&mut vm).unwrap();
        // Stack must remain empty — ACCEPT is ( -- ).
        assert_eq!(vm.data_stack.len(), 0);
    }

    #[test]
    fn test_accept_empty_line() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("\n"));
        accept_prim(&mut vm).unwrap();
        assert_eq!(vm.input_buffer, Some("".to_string()));
    }

    // --- getdec_prim ---

    #[test]
    fn test_getdec_pushes_integer() {
        let mut vm = VM::new();
        vm.input_buffer = Some("42".to_string());
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(42)));
    }

    #[test]
    fn test_getdec_negative_integer() {
        let mut vm = VM::new();
        vm.input_buffer = Some("-7".to_string());
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(-7)));
    }

    #[test]
    fn test_getdec_trims_whitespace() {
        let mut vm = VM::new();
        vm.input_buffer = Some("  100  ".to_string());
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(100)));
    }

    #[test]
    fn test_getdec_consumes_buffer() {
        let mut vm = VM::new();
        vm.input_buffer = Some("10".to_string());
        getdec_prim(&mut vm).unwrap();
        // input_buffer must be None after take().
        assert_eq!(vm.input_buffer, None);
    }

    #[test]
    fn test_getdec_empty_buffer_returns_error() {
        let mut vm = VM::new();
        // input_buffer is None by default.
        let result = getdec_prim(&mut vm);
        assert_eq!(result, Err(TbxError::InputBufferEmpty));
    }

    #[test]
    fn test_getdec_non_integer_returns_error() {
        let mut vm = VM::new();
        vm.input_buffer = Some("abc".to_string());
        let result = getdec_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::ParseIntError { .. })),
            "expected ParseIntError, got: {:?}",
            result
        );
    }

    #[test]
    fn test_accept_then_getdec_sequence() {
        use std::io::Cursor;
        let mut vm = VM::new();
        vm.input_reader = Box::new(Cursor::new("123\n"));
        accept_prim(&mut vm).unwrap();
        getdec_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(123)));
    }
}
