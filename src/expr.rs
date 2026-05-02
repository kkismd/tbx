//! Expression compiler using the Shunting-Yard Algorithm (SYA).
//!
//! Converts a slice of `SpannedToken`s representing an infix expression into a
//! flat RPN instruction sequence (`Vec<Cell>`) that can be appended to the VM
//! dictionary or executed directly.

use std::collections::HashMap;

use crate::cell::{Cell, Xt};
use crate::dict::{EntryKind, FLAG_IMMEDIATE};
use crate::error::TbxError;
use crate::lexer::{SpannedToken, Token};
use crate::vm::VM;

// ---------------------------------------------------------------------------
// Operator stack item
// ---------------------------------------------------------------------------

/// An item held on the operator stack during Shunting-Yard processing.
#[derive(Debug)]
enum OpItem {
    /// Binary operator with its primitive name and precedence (1=highest, 11=lowest).
    BinOp { prim: &'static str, prec: u8 },
    /// Unary prefix negation (`-`). Right-associative with precedence 1 (highest).
    UnaryNeg,
    /// Left-parenthesis sentinel. Carries optional call metadata.
    ///
    /// `call` is `Some(LParenCall::Function(xt, arity))` for a function call,
    /// `Some(LParenCall::Array(base, size))` for an array index expression,
    /// and `None` for a plain grouping parenthesis.
    LParen { call: Option<LParenCall> },
    /// Comma-as-binary-operator used outside of function calls (precedence 11).
    /// Both sides are evaluated in order; this marker produces no VM instruction.
    CommaSep,
}

/// Metadata attached to a `LParen` operator-stack item to distinguish call types.
#[derive(Debug)]
enum LParenCall {
    /// Function call: execution token and current argument arity count.
    Function(Xt, usize),
    /// Global array index access: dictionary base index and declared array size.
    Array(usize, usize),
    /// Local array index access: the Cell::Array handle is already on the output stack.
    /// On closing `)`, emit ARRAY_GET (index is on top, handle is below).
    LocalArray,
}

// ---------------------------------------------------------------------------
// ExprCompiler
// ---------------------------------------------------------------------------

/// Compiles an infix expression to a flat RPN instruction sequence.
///
/// The compiler borrows the `VM` mutably for string-pool interning and
/// dictionary look-ups; it never writes cells to the dictionary itself.
/// The caller receives the instruction sequence and decides how to use it.
pub struct ExprCompiler<'a> {
    vm: &'a mut VM,
    /// Optional reference to the local variable table for the word being compiled.
    /// Local variables shadow same-named globals: this table is checked first.
    /// Passed as a reference to avoid cloning the table on every statement.
    local_table: Option<&'a HashMap<String, usize>>,
    /// Name of the word currently being compiled, used to allow self-recursive lookups.
    /// Only this word's hidden entry (FLAG_HIDDEN) is visible to identifier resolution.
    self_word: Option<String>,
    /// Header index of the word currently being compiled (for self-recursive call detection).
    /// `None` outside of compile mode or when patching is not needed.
    self_hdr_idx: Option<usize>,
    /// Offsets within the compiled output `Vec<Cell>` where a `local_count` placeholder
    /// (`Cell::Int(0)`) was written for a self-recursive CALL instruction.
    /// The caller must translate these to absolute dictionary positions and add them to
    /// `CompileState::call_patch_list` after writing the cells to the dictionary.
    pub(crate) patch_offsets: Vec<usize>,
}

impl<'a> ExprCompiler<'a> {
    /// Create an `ExprCompiler` backed by the given VM (no local variable table).
    pub fn new(vm: &'a mut VM) -> Self {
        Self {
            vm,
            local_table: None,
            self_word: None,
            self_hdr_idx: None,
            patch_offsets: Vec::new(),
        }
    }

    /// Create an `ExprCompiler` with a reference to the local variable table and the
    /// name of the word currently being compiled (for self-recursive call resolution).
    ///
    /// `local_table` is borrowed rather than cloned to avoid per-statement HashMap copies.
    /// `self_hdr_idx` is the header index of the word being compiled; when
    /// a call to that same index is encountered in an expression, a `local_count`
    /// placeholder is emitted and its offset recorded in `patch_offsets`.
    pub fn with_context(
        vm: &'a mut VM,
        local_table: Option<&'a HashMap<String, usize>>,
        self_word: Option<String>,
        self_hdr_idx: Option<usize>,
    ) -> Self {
        Self {
            vm,
            local_table,
            self_word,
            self_hdr_idx,
            patch_offsets: Vec::new(),
        }
    }

    /// Parse `tokens` and return the corresponding RPN instruction sequence.
    ///
    /// No cells are written to the VM dictionary; the caller owns the returned
    /// `Vec<Cell>`.
    ///
    /// # Errors
    ///
    /// Returns `TbxError::UndefinedSymbol` if an identifier cannot be resolved.
    /// Returns `TbxError::TypeError` for operand/operator type mismatches.
    pub fn compile_expr(&mut self, tokens: &[SpannedToken]) -> Result<Vec<Cell>, TbxError> {
        let mut output: Vec<Cell> = Vec::new();
        let mut op_stack: Vec<OpItem> = Vec::new();
        // True when the previous significant token produced a value on the
        // conceptual stack (literal, closing paren, resolved identifier).
        let mut prev_was_operand = false;

        let mut i = 0;
        while i < tokens.len() {
            // Clone the token so we can reference `self.vm` without borrow conflicts.
            let token = tokens[i].token.clone();

            match token {
                // -------------------------------------------------------
                // Literals
                // -------------------------------------------------------
                Token::IntLit(n) => {
                    emit_lit(&mut output, Cell::Int(n), self.vm)?;
                    prev_was_operand = true;
                }

                Token::FloatLit(f) => {
                    emit_lit(&mut output, Cell::Float(f), self.vm)?;
                    prev_was_operand = true;
                }

                Token::StringLit(s) => {
                    let idx = self.vm.intern_string(&s)?;
                    emit_lit(&mut output, Cell::StringDesc(idx), self.vm)?;
                    prev_was_operand = true;
                }

                // -------------------------------------------------------
                // Identifiers (variables, constants, function calls)
                // -------------------------------------------------------
                Token::Ident(name) => {
                    let name = name.to_ascii_uppercase();
                    // Check local variable table first — locals shadow globals.
                    if let Some(local_idx) = self.local_table.and_then(|lt| lt.get(&name)).copied()
                    {
                        // Peek ahead: local variable followed by `(` means local array access.
                        let next_is_lparen = tokens
                            .get(i + 1)
                            .map(|st| matches!(st.token, Token::LParen))
                            .unwrap_or(false);

                        if next_is_lparen {
                            // Local array element read: A(I)
                            // 1. Load the Cell::Array handle from the local slot.
                            emit_local_read(&mut output, local_idx, self.vm)?;
                            // 2. Open an array-index paren frame marked as LocalArray
                            //    to distinguish it from a regular function call paren.
                            op_stack.push(OpItem::LParen {
                                call: Some(LParenCall::LocalArray),
                            });
                            i += 1; // consume '('
                            prev_was_operand = false;
                        } else {
                            emit_local_read(&mut output, local_idx, self.vm)?;
                            prev_was_operand = true;
                            i += 1;
                            continue;
                        }
                        i += 1;
                        continue;
                    }

                    let xt = self
                        .vm
                        .lookup_including_self(&name, self.self_word.as_deref())
                        .ok_or_else(|| TbxError::UndefinedSymbol { name: name.clone() })?;

                    // Reject IMMEDIATE words inside expressions.
                    // Per spec, IMMEDIATE words are only allowed at statement level.
                    if self.vm.headers[xt.index()].flags & FLAG_IMMEDIATE != 0 {
                        return Err(TbxError::InvalidExpression {
                            reason: "IMMEDIATE word cannot appear inside an expression",
                        });
                    }

                    // Peek ahead: is this a function call (`F(`)?
                    let next_is_lparen = tokens
                        .get(i + 1)
                        .map(|st| matches!(st.token, Token::LParen))
                        .unwrap_or(false);

                    if next_is_lparen {
                        // Is it a zero-argument call `F()`?
                        let next_is_rparen = tokens
                            .get(i + 2)
                            .map(|st| matches!(st.token, Token::RParen))
                            .unwrap_or(false);

                        if next_is_rparen {
                            // Arrays require exactly one index expression: NUMS(I).
                            if matches!(&self.vm.headers[xt.index()].kind, EntryKind::Array { .. })
                            {
                                return Err(TbxError::InvalidExpression {
                                    reason:
                                        "array index expression must not be empty: use NAME(index)",
                                });
                            }
                            // Zero-argument call: emit based on entry kind.
                            emit_call_by_kind(
                                &mut output,
                                xt,
                                0,
                                self.vm,
                                self.self_hdr_idx,
                                &mut self.patch_offsets,
                            )?;
                            i += 2; // skip '(' and ')'
                            prev_was_operand = true;
                        } else {
                            // Peek the entry kind to distinguish arrays from function calls.
                            // Use a reference to avoid cloning (no VM methods are called in
                            // either branch, only op_stack — a local Vec — is mutated).
                            if let EntryKind::Array { base, size } =
                                &self.vm.headers[xt.index()].kind
                            {
                                // Array index access: open an array-paren frame.
                                op_stack.push(OpItem::LParen {
                                    call: Some(LParenCall::Array(*base, *size)),
                                });
                            } else {
                                // Function call with arguments: open a function-call
                                // paren frame with initial arity = 1.
                                op_stack.push(OpItem::LParen {
                                    call: Some(LParenCall::Function(xt, 1)),
                                });
                            }
                            i += 1; // consume '('
                            prev_was_operand = false;
                        }
                    } else {
                        // Variable read or nullary call (no parentheses).
                        match &self.vm.headers[xt.index()].kind {
                            EntryKind::Variable(addr) => {
                                emit_var_read(&mut output, *addr, self.vm)?;
                            }
                            EntryKind::Array { .. } => {
                                // Arrays must always be accessed with index syntax NAME(index).
                                return Err(TbxError::InvalidExpression {
                                    reason: "array must be accessed with index syntax NAME(index)",
                                });
                            }
                            _ => {
                                // Treat as a nullary call: emit based on entry kind.
                                emit_call_by_kind(
                                    &mut output,
                                    xt,
                                    0,
                                    self.vm,
                                    self.self_hdr_idx,
                                    &mut self.patch_offsets,
                                )?;
                            }
                        }
                        prev_was_operand = true;
                    }
                }

                // -------------------------------------------------------
                // Unary / binary `&` (Ampersand)
                // -------------------------------------------------------
                Token::Ampersand => {
                    if prev_was_operand {
                        // Binary bitwise-AND (precedence 6, left-associative).
                        pop_ops_while(&mut op_stack, &mut output, self.vm, 6, true)?;
                        op_stack.push(OpItem::BinOp {
                            prim: "BAND",
                            prec: 6,
                        });
                        prev_was_operand = false;
                    } else {
                        // Unary address-of operator: eagerly consume the next identifier
                        // (and optional array index) without emitting a FETCH.
                        i += 1;
                        let next_tok = tokens.get(i).map(|st| st.token.clone());
                        match next_tok {
                            Some(Token::Ident(name)) => {
                                let name = name.to_ascii_uppercase();
                                // Check local table first — locals shadow globals.
                                if let Some(local_idx) =
                                    self.local_table.and_then(|lt| lt.get(&name)).copied()
                                {
                                    // Is this a local array element address: &A(I)?
                                    let lparen_pos = i + 1;
                                    if tokens
                                        .get(lparen_pos)
                                        .map(|st| matches!(st.token, Token::LParen))
                                        .unwrap_or(false)
                                    {
                                        // Local array address-of: &A(I)
                                        // Find the matching ')'.
                                        let idx_start = lparen_pos + 1;
                                        let close_pos = find_matching_rparen(tokens, idx_start)
                                            .ok_or(TbxError::InvalidExpression {
                                                reason:
                                                    "missing ')' in local array index expression",
                                            })?;
                                        let index_toks = &tokens[idx_start..close_pos];
                                        if index_toks.is_empty() {
                                            return Err(TbxError::InvalidExpression {
                                                reason: "array index expression must not be empty: use &NAME(index)",
                                            });
                                        }
                                        // 1. Load the Cell::Array handle.
                                        emit_local_read(&mut output, local_idx, self.vm)?;
                                        // 2. Compile the index expression.
                                        let index_cells = self.compile_expr(index_toks)?;
                                        output.extend(index_cells);
                                        // 3. Emit ARRAY_ADDR.
                                        let array_addr_xt = require_xt(self.vm, "ARRAY_ADDR")?;
                                        output.push(Cell::Xt(array_addr_xt));
                                        i = close_pos;
                                    } else {
                                        // Simple local variable address: &A (StackAddr).
                                        let xt_lit = require_xt(self.vm, "LIT")?;
                                        output.push(Cell::Xt(xt_lit));
                                        output.push(Cell::StackAddr(local_idx));
                                    }
                                } else {
                                    let xt = self
                                        .vm
                                        .lookup_including_self(&name, self.self_word.as_deref())
                                        .ok_or_else(|| TbxError::UndefinedSymbol {
                                            name: name.clone(),
                                        })?;
                                    match &self.vm.headers[xt.index()].kind {
                                        EntryKind::Variable(addr) => {
                                            // Emit address only — no FETCH.
                                            let xt_lit = require_xt(self.vm, "LIT")?;
                                            output.push(Cell::Xt(xt_lit));
                                            output.push(Cell::DictAddr(*addr));
                                        }
                                        EntryKind::Array { base, size } => {
                                            let (base, size) = (*base, *size);
                                            // Array address-of: &NUMS(I)
                                            // Consume '(' and compile the index expression,
                                            // then emit OFFSET without a trailing FETCH.
                                            let lparen_pos = i + 1;
                                            if !tokens
                                                .get(lparen_pos)
                                                .map(|st| matches!(st.token, Token::LParen))
                                                .unwrap_or(false)
                                            {
                                                return Err(TbxError::InvalidExpression {
                                                    reason:
                                                        "expected '(' after array name in unary &",
                                                });
                                            }
                                            // Find the matching ')' starting after '('.
                                            let idx_start = lparen_pos + 1;
                                            let close_pos = find_matching_rparen(tokens, idx_start)
                                                .ok_or(TbxError::InvalidExpression {
                                                    reason: "missing ')' in array index expression",
                                                })?;
                                            let index_toks = &tokens[idx_start..close_pos];
                                            if index_toks.is_empty() {
                                                return Err(TbxError::InvalidExpression {
                                                    reason: "array index expression must not be empty: use &NAME(index)",
                                                });
                                            }
                                            // Recursively compile the index expression.
                                            let index_cells = self.compile_expr(index_toks)?;
                                            output.extend(index_cells);
                                            // Emit OFFSET with inline base and size.
                                            let offset_xt = require_xt(self.vm, "OFFSET")?;
                                            output.push(Cell::Xt(offset_xt));
                                            output.push(Cell::Int(base as i64));
                                            output.push(Cell::Int(size as i64));
                                            // Advance i to the closing ')'.
                                            i = close_pos;
                                        }
                                        _ => {
                                            return Err(TbxError::TypeError {
                                                expected:
                                                    "variable or array identifier after unary &",
                                                got: "non-variable",
                                            });
                                        }
                                    }
                                }
                            }
                            _ => {
                                return Err(TbxError::TypeError {
                                    expected: "identifier after unary &",
                                    got: "non-identifier",
                                });
                            }
                        }
                        prev_was_operand = true;
                    }
                }

                // -------------------------------------------------------
                // Operator tokens (includes `-`, all binary operators)
                // -------------------------------------------------------
                Token::Op(s) => {
                    match s.as_str() {
                        "-" => {
                            if prev_was_operand {
                                // Binary subtraction (precedence 3, left-associative).
                                pop_ops_while(&mut op_stack, &mut output, self.vm, 3, true)?;
                                op_stack.push(OpItem::BinOp {
                                    prim: "SUB",
                                    prec: 3,
                                });
                                prev_was_operand = false;
                            } else {
                                // Unary negation (precedence 1, right-associative).
                                // No operators are popped (right-assoc, highest prec).
                                op_stack.push(OpItem::UnaryNeg);
                                prev_was_operand = false;
                            }
                        }
                        other => {
                            if let Some((prim, prec, left)) = binary_op_info(other) {
                                pop_ops_while(&mut op_stack, &mut output, self.vm, prec, left)?;
                                op_stack.push(OpItem::BinOp { prim, prec });
                                prev_was_operand = false;
                            } else {
                                return Err(TbxError::InvalidExpression {
                                    reason: "unknown operator in expression",
                                });
                            }
                        }
                    }
                }

                // -------------------------------------------------------
                // Parentheses
                // -------------------------------------------------------
                Token::LParen => {
                    // Plain grouping parenthesis (not a function call).
                    op_stack.push(OpItem::LParen { call: None });
                    prev_was_operand = false;
                }

                Token::RParen => {
                    // Pop operators to output until the nearest LParen.
                    loop {
                        match op_stack.last() {
                            Some(OpItem::LParen { .. }) => break,
                            None => {
                                // No matching '(' found — unmatched ')'.
                                return Err(TbxError::InvalidExpression {
                                    reason: "unmatched ')' in expression",
                                });
                            }
                            _ => {
                                let op = op_stack.pop().unwrap();
                                emit_op_item(&op, &mut output, self.vm)?;
                            }
                        }
                    }
                    // Pop the LParen and dispatch based on the call kind.
                    match op_stack.pop() {
                        Some(OpItem::LParen {
                            call: Some(LParenCall::Function(xt, arity)),
                        }) => {
                            emit_call_by_kind(
                                &mut output,
                                xt,
                                arity,
                                self.vm,
                                self.self_hdr_idx,
                                &mut self.patch_offsets,
                            )?;
                        }
                        Some(OpItem::LParen {
                            call: Some(LParenCall::Array(base, size)),
                        }) => {
                            // Emit OFFSET (inline base + size), then FETCH to load the value.
                            let offset_xt = require_xt(self.vm, "OFFSET")?;
                            let fetch_xt = require_xt(self.vm, "FETCH")?;
                            output.push(Cell::Xt(offset_xt));
                            output.push(Cell::Int(base as i64));
                            output.push(Cell::Int(size as i64));
                            output.push(Cell::Xt(fetch_xt));
                        }
                        Some(OpItem::LParen {
                            call: Some(LParenCall::LocalArray),
                        }) => {
                            // Emit ARRAY_GET: stack is [..., Cell::Array, index] → value.
                            let array_get_xt = require_xt(self.vm, "ARRAY_GET")?;
                            output.push(Cell::Xt(array_get_xt));
                        }
                        _ => {
                            // Plain grouping paren — nothing extra to emit.
                        }
                    }
                    prev_was_operand = true;
                }

                // -------------------------------------------------------
                // Comma (argument separator or low-priority binary op)
                // -------------------------------------------------------
                Token::Comma => {
                    // A comma is an argument separator when the nearest enclosing
                    // parenthesis belongs to a function call (not an array index).
                    let nearest_lparen = op_stack
                        .iter()
                        .rev()
                        .find(|op| matches!(op, OpItem::LParen { .. }));

                    // Array index expressions must be a single expression — commas are
                    // not allowed inside array subscripts (global or local).
                    if matches!(
                        nearest_lparen,
                        Some(OpItem::LParen {
                            call: Some(LParenCall::Array(..))
                        }) | Some(OpItem::LParen {
                            call: Some(LParenCall::LocalArray)
                        })
                    ) {
                        return Err(TbxError::InvalidExpression {
                            reason: "array index must be a single expression: use NAME(index)",
                        });
                    }

                    let in_func_call = matches!(
                        nearest_lparen,
                        Some(OpItem::LParen {
                            call: Some(LParenCall::Function(..))
                        })
                    );

                    if in_func_call {
                        // Flush operators accumulated for this argument.
                        loop {
                            match op_stack.last() {
                                Some(OpItem::LParen { .. }) | None => break,
                                _ => {
                                    let op = op_stack.pop().unwrap();
                                    emit_op_item(&op, &mut output, self.vm)?;
                                }
                            }
                        }
                        // Increment the arity counter in the enclosing LParen frame.
                        if let Some(OpItem::LParen {
                            call: Some(LParenCall::Function(_, arity)),
                        }) = op_stack.last_mut()
                        {
                            *arity += 1;
                        }
                    } else {
                        // Comma as lowest-priority binary operator (precedence 11).
                        // Both sides end up in the output in order; no instruction emitted.
                        pop_ops_while(&mut op_stack, &mut output, self.vm, 11, true)?;
                        op_stack.push(OpItem::CommaSep);
                    }
                    prev_was_operand = false;
                }

                // -------------------------------------------------------
                // Expression terminators
                // -------------------------------------------------------
                Token::Newline | Token::Eof | Token::Semicolon => break,

                // Propagate lexer errors rather than silently ignoring them.
                // Silently skipping Token::Error would compile incomplete
                // expressions and cause confusing runtime failures.
                Token::Error(_) => {
                    return Err(TbxError::InvalidExpression {
                        reason: "lexer error in expression (e.g. unterminated string literal)",
                    });
                }

                // Ignore all other tokens (LineNum, …).
                _ => {}
            }

            i += 1;
        }

        // Drain the operator stack, emitting remaining operators to output.
        while let Some(op) = op_stack.pop() {
            emit_op_item(&op, &mut output, self.vm)?;
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Look up a system word by name and return its `Xt`, or a `TypeError` if absent.
fn require_xt(vm: &VM, name: &'static str) -> Result<Xt, TbxError> {
    vm.lookup(name).ok_or(TbxError::TypeError {
        expected: "system word to be registered",
        got: "not found",
    })
}

/// Find the index of the matching closing `)` for an already-consumed `(`.
///
/// `tokens[start..]` is the content after the opening `(`.  Returns the index
/// in `tokens` of the first `)` at depth 0 (accounting for nested parentheses),
/// or `None` if no such `)` exists.
fn find_matching_rparen(tokens: &[SpannedToken], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, st) in tokens.iter().enumerate().skip(start) {
        match &st.token {
            Token::LParen => depth += 1,
            Token::RParen => {
                if depth == 0 {
                    return Some(j);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Emit `Xt(LIT)` followed by `value` onto `output`.
fn emit_lit(output: &mut Vec<Cell>, value: Cell, vm: &VM) -> Result<(), TbxError> {
    let xt = require_xt(vm, "LIT")?;
    output.push(Cell::Xt(xt));
    output.push(value);
    Ok(())
}

/// Emit the variable-read sequence: `Xt(LIT)`, `DictAddr(addr)`, `Xt(FETCH)`.
fn emit_var_read(output: &mut Vec<Cell>, addr: usize, vm: &VM) -> Result<(), TbxError> {
    let lit_xt = require_xt(vm, "LIT")?;
    let fetch_xt = require_xt(vm, "FETCH")?;
    output.push(Cell::Xt(lit_xt));
    output.push(Cell::DictAddr(addr));
    output.push(Cell::Xt(fetch_xt));
    Ok(())
}

/// Emit the local-variable-read sequence: `Xt(LIT)`, `StackAddr(idx)`, `Xt(FETCH)`.
fn emit_local_read(output: &mut Vec<Cell>, idx: usize, vm: &VM) -> Result<(), TbxError> {
    let lit_xt = require_xt(vm, "LIT")?;
    let fetch_xt = require_xt(vm, "FETCH")?;
    output.push(Cell::Xt(lit_xt));
    output.push(Cell::StackAddr(idx));
    output.push(Cell::Xt(fetch_xt));
    Ok(())
}

/// Emit the function-call sequence based on the `EntryKind` of `xt`.
///
/// - `EntryKind::Word`: emits `Xt(CALL)`, `Xt(xt)`, `Int(arity)`, `Int(local_count)`
///   - When `xt.index() == self_hdr_idx` (self-recursive call), emits `Int(0)` as a
///     placeholder and records the offset in `patch_offsets` for later back-patching.
/// - `EntryKind::Primitive` (variadic): emits `Xt(LIT)`, `Int(arity)`, `Xt(xt)`
/// - `EntryKind::Primitive` (fixed) / `Variable` / `Constant`: emits `Xt(xt)` directly
/// - Any internal kind (Lit, Call, Exit, ReturnVal, DropToMarker): returns `InvalidExpression`
fn emit_call_by_kind(
    output: &mut Vec<Cell>,
    xt: Xt,
    arity: usize,
    vm: &VM,
    self_hdr_idx: Option<usize>,
    patch_offsets: &mut Vec<usize>,
) -> Result<(), TbxError> {
    vm.headers[xt.index()].check_variadic_arity(arity)?;

    // Match by reference: `vm` is immutable here so no borrow conflict arises.
    match &vm.headers[xt.index()].kind {
        EntryKind::Word(_) => {
            let call_xt = require_xt(vm, "CALL")?;
            output.push(Cell::Xt(call_xt));
            output.push(Cell::Xt(xt));
            output.push(Cell::Int(arity as i64));
            // For a self-recursive call the local_count is not yet finalized — emit
            // a placeholder (0) and record the offset so the caller can back-patch it.
            if self_hdr_idx.is_some_and(|idx| idx == xt.index()) {
                patch_offsets.push(output.len());
                output.push(Cell::Int(0));
            } else {
                let local_count = vm.headers[xt.index()].local_count;
                output.push(Cell::Int(local_count as i64));
            }
        }
        EntryKind::Primitive(_) if vm.headers[xt.index()].is_variadic => {
            // Variadic primitive: push arity as Int before the Xt so the
            // primitive can pop it and know how many arguments to consume.
            let lit_xt = require_xt(vm, "LIT")?;
            output.push(Cell::Xt(lit_xt));
            output.push(Cell::Int(arity as i64));
            output.push(Cell::Xt(xt));
        }
        EntryKind::Primitive(_) | EntryKind::Variable(_) | EntryKind::Constant(_) => {
            output.push(Cell::Xt(xt));
        }
        _ => {
            return Err(TbxError::InvalidExpression {
                reason: "invalid entry kind in function call position",
            });
        }
    }
    Ok(())
}

/// Pop operators from `op_stack` to `output` while their priority is higher or
/// equal to the current incoming operator (accounting for associativity).
///
/// `cur_prec` and `cur_left` describe the **current** (incoming) operator.
/// Precedence uses the spec's convention: 1 = highest priority, 11 = lowest.
fn pop_ops_while(
    op_stack: &mut Vec<OpItem>,
    output: &mut Vec<Cell>,
    vm: &VM,
    cur_prec: u8,
    cur_left: bool,
) -> Result<(), TbxError> {
    loop {
        match op_stack.last() {
            // LParen is never popped by precedence rules.
            Some(OpItem::LParen { .. }) | None => break,
            Some(op) => {
                let top_prec = op_prec(op);
                // Standard SYA rule (our numbering: lower = higher priority):
                //   left-assoc current:  pop top when top_prec <= cur_prec
                //   right-assoc current: pop top when top_prec <  cur_prec
                let should_pop = if cur_left {
                    top_prec <= cur_prec
                } else {
                    top_prec < cur_prec
                };
                if should_pop {
                    let op = op_stack.pop().unwrap();
                    emit_op_item(&op, output, vm)?;
                } else {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Return the precedence of an operator stack item (1 = highest, 11 = lowest).
fn op_prec(op: &OpItem) -> u8 {
    match op {
        OpItem::BinOp { prec, .. } => *prec,
        OpItem::UnaryNeg => 1,
        OpItem::CommaSep => 11,
        OpItem::LParen { .. } => u8::MAX, // sentinel; never compared in practice
    }
}

/// Emit the VM instruction(s) for a single operator stack item.
fn emit_op_item(op: &OpItem, output: &mut Vec<Cell>, vm: &VM) -> Result<(), TbxError> {
    match op {
        OpItem::BinOp { prim, .. } => {
            let xt = require_xt(vm, prim)?;
            output.push(Cell::Xt(xt));
        }
        OpItem::UnaryNeg => {
            let xt = require_xt(vm, "NEGATE")?;
            output.push(Cell::Xt(xt));
        }
        OpItem::CommaSep => {
            // Binary comma separator: both sides are already in the output buffer;
            // no VM instruction is emitted.
        }
        OpItem::LParen { .. } => {
            // A stray LParen surviving to drain means an unmatched '(' — error.
            return Err(TbxError::InvalidExpression {
                reason: "unmatched '(' in expression",
            });
        }
    }
    Ok(())
}

/// Map a binary operator string to `(primitive_name, precedence, left_assoc)`.
///
/// Returns `None` for unrecognised operator strings.
fn binary_op_info(op: &str) -> Option<(&'static str, u8, bool)> {
    match op {
        "+" => Some(("ADD", 3, true)),
        "*" => Some(("MUL", 2, true)),
        "/" => Some(("DIV", 2, true)),
        "%" => Some(("MOD", 2, true)),
        "<" => Some(("LT", 4, true)),
        ">" => Some(("GT", 4, true)),
        "<=" => Some(("LE", 4, true)),
        ">=" => Some(("GE", 4, true)),
        "=" => Some(("EQ", 5, true)),
        "<>" => Some(("NEQ", 5, true)),
        "|" => Some(("BOR", 7, true)),
        "&&" => Some(("AND", 8, true)),
        "||" => Some(("OR", 9, true)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::WordEntry;
    use crate::lexer::Lexer;

    /// Tokenise `src`, stopping before `Newline` / `Eof`.
    ///
    /// A dummy identifier prefix is prepended so that the lexer's
    /// `at_line_start` flag is cleared before the first real token.
    /// This prevents leading integer literals from being classified as
    /// `LineNum` tokens.
    fn lex(src: &str) -> Vec<SpannedToken> {
        // The `_S ` prefix forces `at_line_start` to false before any token in
        // `src` is scanned. The leading identifier token is discarded.
        let prefixed = format!("_S {src}");
        let mut lx = Lexer::new(&prefixed);
        lx.next_token(); // discard the dummy "_S" identifier
        let mut out = Vec::new();
        loop {
            let st = lx.next_token();
            match &st.token {
                Token::Eof | Token::Newline => break,
                _ => out.push(st),
            }
        }
        out
    }

    /// Build a VM with all system primitives registered.
    fn make_vm() -> VM {
        crate::init_vm()
    }

    /// Helper: extract the name of each Cell for readable assertions.
    fn cell_names(cells: &[Cell]) -> Vec<String> {
        cells.iter().map(|c| format!("{c}")).collect()
    }

    // ------------------------------------------------------------------
    // Test 1: integer literal
    // ------------------------------------------------------------------

    #[test]
    fn test_int_literal() {
        let mut vm = make_vm();
        let tokens = lex("42");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();
        assert_eq!(result.len(), 2);
        let lit_xt = vm.lookup("LIT").unwrap();
        assert_eq!(result[0], Cell::Xt(lit_xt));
        assert_eq!(result[1], Cell::Int(42));
    }

    // ------------------------------------------------------------------
    // Test 2: arithmetic with precedence (1 + 2 * 3)
    // ------------------------------------------------------------------

    #[test]
    fn test_precedence_mul_before_add() {
        let mut vm = make_vm();
        let tokens = lex("1 + 2 * 3");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let mul_xt = vm.lookup("MUL").unwrap();
        let add_xt = vm.lookup("ADD").unwrap();

        // Expected RPN: LIT 1 LIT 2 LIT 3 MUL ADD
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(lit_xt),
                Cell::Int(2),
                Cell::Xt(lit_xt),
                Cell::Int(3),
                Cell::Xt(mul_xt),
                Cell::Xt(add_xt),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Test 3: unary minus
    // ------------------------------------------------------------------

    #[test]
    fn test_unary_minus() {
        let mut vm = make_vm();
        let tokens = lex("-1");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let neg_xt = vm.lookup("NEGATE").unwrap();

        assert_eq!(
            result,
            vec![Cell::Xt(lit_xt), Cell::Int(1), Cell::Xt(neg_xt)]
        );
    }

    // ------------------------------------------------------------------
    // Test 4: global variable read
    // ------------------------------------------------------------------

    #[test]
    fn test_global_variable_read() {
        let mut vm = make_vm();
        // Register a global variable "A" backed by dictionary slot 0.
        vm.dict_write(Cell::Int(0)).unwrap(); // allocate slot 0
        vm.register(WordEntry::new_variable("A", 0));

        let tokens = lex("A");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let fetch_xt = vm.lookup("FETCH").unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], Cell::Xt(lit_xt));
        assert_eq!(result[1], Cell::DictAddr(0));
        assert_eq!(result[2], Cell::Xt(fetch_xt));
    }

    // ------------------------------------------------------------------
    // Test 5: address-of operator &A
    // ------------------------------------------------------------------

    #[test]
    fn test_address_of_variable() {
        let mut vm = make_vm();
        vm.dict_write(Cell::Int(0)).unwrap();
        vm.register(WordEntry::new_variable("A", 0));

        let tokens = lex("&A");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();

        // Should emit LIT DictAddr(0) — no FETCH
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Cell::Xt(lit_xt));
        assert_eq!(result[1], Cell::DictAddr(0));
    }

    // ------------------------------------------------------------------
    // Test 6: string literal
    // ------------------------------------------------------------------

    #[test]
    fn test_string_literal() {
        let mut vm = make_vm();
        let tokens = lex(r#""hello""#);
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Cell::Xt(lit_xt));
        // The string "hello" should be interned at index 0 in an empty pool.
        assert_eq!(result[1], Cell::StringDesc(0));
    }

    // ------------------------------------------------------------------
    // Test 7: function call F(1, 2)
    // ------------------------------------------------------------------

    #[test]
    fn test_function_call_two_args() {
        let mut vm = make_vm();

        // Register a dummy compiled word "F" pointing to dictionary offset 0.
        // (The content does not matter for compilation; we only need the Xt.)
        let f_xt = vm.register(WordEntry::new_word("F", 0));

        let tokens = lex("F(1, 2)");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let call_xt = vm.lookup("CALL").unwrap();

        // Expected: LIT 1, LIT 2, CALL F 2 0
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(lit_xt),
                Cell::Int(2),
                Cell::Xt(call_xt),
                Cell::Xt(f_xt),
                Cell::Int(2),
                Cell::Int(0),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Test 8: comma as statement-level separator (A, B)
    // ------------------------------------------------------------------

    #[test]
    fn test_comma_separator() {
        let mut vm = make_vm();
        // Register two global variables.
        vm.dict_write(Cell::Int(0)).unwrap();
        vm.dict_write(Cell::Int(0)).unwrap();
        vm.register(WordEntry::new_variable("A", 0));
        vm.register(WordEntry::new_variable("B", 1));

        let tokens = lex("A, B");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let fetch_xt = vm.lookup("FETCH").unwrap();

        // A read + B read, no comma instruction
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::DictAddr(0),
                Cell::Xt(fetch_xt),
                Cell::Xt(lit_xt),
                Cell::DictAddr(1),
                Cell::Xt(fetch_xt),
            ]
        );
        let _ = cell_names(&result); // smoke-test Display
    }

    // ------------------------------------------------------------------
    // Additional: float literal
    // ------------------------------------------------------------------

    #[test]
    fn test_float_literal() {
        let mut vm = make_vm();
        let tokens = lex("2.5");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Cell::Xt(lit_xt));
        assert_eq!(result[1], Cell::Float(2.5));
    }

    // ------------------------------------------------------------------
    // Additional: zero-argument function call F()
    // ------------------------------------------------------------------

    #[test]
    fn test_zero_arg_call() {
        let mut vm = make_vm();
        let f_xt = vm.register(WordEntry::new_word("F", 0));

        let tokens = lex("F()");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let call_xt = vm.lookup("CALL").unwrap();
        assert_eq!(
            result,
            vec![
                Cell::Xt(call_xt),
                Cell::Xt(f_xt),
                Cell::Int(0),
                Cell::Int(0),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Additional: double unary minus --1
    // ------------------------------------------------------------------

    #[test]
    fn test_double_unary_minus() {
        let mut vm = make_vm();
        let tokens = lex("--1");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let neg_xt = vm.lookup("NEGATE").unwrap();

        // RPN: LIT 1 NEGATE NEGATE
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(neg_xt),
                Cell::Xt(neg_xt),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Additional: comparison operator
    // ------------------------------------------------------------------

    #[test]
    fn test_comparison_operator() {
        let mut vm = make_vm();
        let tokens = lex("1 < 2");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let lt_xt = vm.lookup("LT").unwrap();

        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(lit_xt),
                Cell::Int(2),
                Cell::Xt(lt_xt),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Additional: UndefinedSymbol error
    // ------------------------------------------------------------------

    #[test]
    fn test_undefined_symbol_error() {
        let mut vm = make_vm();
        let tokens = lex("NOEXIST");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(matches!(err, TbxError::UndefinedSymbol { name } if name == "NOEXIST"));
    }

    // ------------------------------------------------------------------
    // Error: unmatched '(' (missing closing paren)
    // ------------------------------------------------------------------

    #[test]
    fn test_unmatched_lparen_error() {
        let mut vm = make_vm();
        let tokens = lex("(1 + 2");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("unmatched '('")),
            "expected InvalidExpression for unmatched '(', got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Error: unmatched ')' (no opening paren)
    // ------------------------------------------------------------------

    #[test]
    fn test_unmatched_rparen_error() {
        let mut vm = make_vm();
        let tokens = lex("1 + 2)");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { reason } if reason.contains("unmatched ')'")),
            "expected InvalidExpression for unmatched ')', got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Error: unknown operator
    // ------------------------------------------------------------------

    #[test]
    fn test_unknown_operator_error() {
        // "!" is lexed as Op("!") but is not in binary_op_info.
        let mut vm = make_vm();
        let tokens = lex("1 ! 2");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for unknown operator, got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Binary & / | compile to BAND / BOR
    // ------------------------------------------------------------------

    #[test]
    fn test_binary_band_compiles() {
        // `1 & 3` should compile to: LIT 1 LIT 3 BAND
        let mut vm = make_vm();
        let tokens = lex("1 & 3");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let band_xt = vm.lookup("BAND").unwrap();

        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(lit_xt),
                Cell::Int(3),
                Cell::Xt(band_xt),
            ]
        );
    }

    #[test]
    fn test_binary_bor_compiles() {
        // `1 | 2` should compile to: LIT 1 LIT 2 BOR
        let mut vm = make_vm();
        let tokens = lex("1 | 2");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let bor_xt = vm.lookup("BOR").unwrap();

        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(lit_xt),
                Cell::Int(2),
                Cell::Xt(bor_xt),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Error: & applied to a function name (non-variable) → TypeError
    // ------------------------------------------------------------------

    #[test]
    fn test_address_of_function_error() {
        let mut vm = make_vm();
        // Register "F" as a compiled word (not a variable).
        vm.register(WordEntry::new_word("F", 0));

        let tokens = lex("&F");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(
                err,
                TbxError::TypeError { expected, .. } if expected.contains("variable") && expected.contains("identifier after unary &")
            ),
            "expected TypeError for & applied to function, got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Error: & applied to a non-identifier token (e.g. integer literal) → TypeError
    // ------------------------------------------------------------------

    #[test]
    fn test_address_of_non_ident_error() {
        let mut vm = make_vm();
        // "&123" — the token after & is an integer literal, not an identifier.
        let tokens = lex("&123");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(
                err,
                TbxError::TypeError { expected, .. } if expected == "identifier after unary &"
            ),
            "expected TypeError for & applied to non-identifier, got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Error: & applied to an undefined symbol → UndefinedSymbol
    // ------------------------------------------------------------------

    #[test]
    fn test_address_of_undefined_error() {
        let mut vm = make_vm();
        let tokens = lex("&NOEXIST");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::UndefinedSymbol { ref name } if name == "NOEXIST"),
            "expected UndefinedSymbol for &NOEXIST, got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Primitive: zero-argument call P() → Xt(p) only (no CALL)
    // ------------------------------------------------------------------

    #[test]
    fn test_primitive_zero_arg_call() {
        let mut vm = make_vm();
        fn dummy(_vm: &mut VM) -> Result<(), TbxError> {
            Ok(())
        }
        let p_xt = vm.register(WordEntry::new_primitive("P", dummy));

        let tokens = lex("P()");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        // A Primitive call must emit only Xt(p) — no CALL prefix.
        assert_eq!(result, vec![Cell::Xt(p_xt)]);
    }

    // ------------------------------------------------------------------
    // Primitive: call with arguments P(1, 2) → LIT 1, LIT 2, Xt(p)
    // ------------------------------------------------------------------

    #[test]
    fn test_primitive_call_with_args() {
        let mut vm = make_vm();
        fn dummy(_vm: &mut VM) -> Result<(), TbxError> {
            Ok(())
        }
        let p_xt = vm.register(WordEntry::new_primitive("P", dummy));
        let lit_xt = vm.lookup("LIT").unwrap();

        let tokens = lex("P(1, 2)");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        // Arguments are pushed normally; the callee is emitted as a bare Xt.
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::Int(1),
                Cell::Xt(lit_xt),
                Cell::Int(2),
                Cell::Xt(p_xt),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Primitive: no-paren reference P → Xt(p) only (no CALL)
    // ------------------------------------------------------------------

    #[test]
    fn test_primitive_no_paren() {
        let mut vm = make_vm();
        fn dummy(_vm: &mut VM) -> Result<(), TbxError> {
            Ok(())
        }
        let p_xt = vm.register(WordEntry::new_primitive("P", dummy));

        let tokens = lex("P");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        // Without parentheses a Primitive is emitted as a bare Xt, not CALL.
        assert_eq!(result, vec![Cell::Xt(p_xt)]);
    }

    // ------------------------------------------------------------------
    // Word: no-paren reference F → CALL Xt(F) Int(0) Int(0)
    // ------------------------------------------------------------------

    #[test]
    fn test_word_no_paren_emits_call() {
        let mut vm = make_vm();
        let f_xt = vm.register(WordEntry::new_word("F", 0));

        let tokens = lex("F");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        // A bare Word identifier (no parentheses) is treated as a nullary call
        // and must emit the CALL 4-cell form, same as F().
        let call_xt = vm.lookup("CALL").unwrap();
        assert_eq!(
            result,
            vec![
                Cell::Xt(call_xt),
                Cell::Xt(f_xt),
                Cell::Int(0),
                Cell::Int(0),
            ]
        );
    }

    // ------------------------------------------------------------------
    // Variable: V() emits Xt(v) directly (not a value read)
    // V without parens → FETCH (value read); V() → Xt(v) (bare push)
    // ------------------------------------------------------------------

    #[test]
    fn test_variable_paren_emits_xt_not_fetch() {
        let mut vm = make_vm();
        let v_xt = vm.register(WordEntry::new_variable("V", 0));

        // V() should emit only Xt(v) — not the LIT+DictAddr+FETCH sequence
        // that a bare `V` (no parentheses) would produce.
        let tokens = lex("V()");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();
        assert_eq!(result, vec![Cell::Xt(v_xt)]);
    }

    #[test]
    fn test_variable_no_paren_emits_fetch() {
        let mut vm = make_vm();
        let v_xt = vm.register(WordEntry::new_variable("V", 0));

        // A bare `V` (no parens) reads the variable value:
        // LIT DictAddr(addr) FETCH
        let lit_xt = vm.lookup("LIT").unwrap();
        let fetch_xt = vm.lookup("FETCH").unwrap();

        // Retrieve the dict address stored in the Variable entry.
        let addr = match vm.headers[v_xt.index()].kind.clone() {
            EntryKind::Variable(a) => a,
            _ => panic!("expected Variable kind"),
        };

        let tokens = lex("V");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();
        assert_eq!(
            result,
            vec![Cell::Xt(lit_xt), Cell::DictAddr(addr), Cell::Xt(fetch_xt),]
        );
    }

    // ------------------------------------------------------------------
    // Internal kind call rejected → InvalidExpression
    // ------------------------------------------------------------------

    #[test]
    fn test_internal_kind_call_rejected() {
        use crate::dict::EntryKind;
        let mut vm = make_vm();
        // Register an entry whose kind is an internal-only variant (Lit).
        vm.register(WordEntry {
            name: "INTERNAL".to_string(),
            flags: 0,
            kind: EntryKind::Lit,
            arity: 0,
            local_count: 0,
            is_variadic: false,
            prev: None,
        });

        // Calling it with () must be rejected.
        let tokens = lex("INTERNAL()");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "expected InvalidExpression for internal kind call, got: {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Array access: NUMS(I) compiles to index + OFFSET + base + size + FETCH
    // ------------------------------------------------------------------

    #[test]
    fn test_array_read_compiles_offset_fetch() {
        let mut vm = make_vm();

        // Register a global variable I at dict[0].
        vm.dict_write(Cell::None).unwrap(); // storage for I
        vm.register(WordEntry::new_variable("I", 0));

        // Register a global array NUMS(3) starting at dict[1].
        let array_base = vm.dp;
        for _ in 0..3 {
            vm.dict_write(Cell::None).unwrap();
        }
        vm.register(WordEntry::new_array("NUMS", array_base, 3));

        // Compile "NUMS(I)".
        let tokens = lex("NUMS(I)");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let fetch_xt = vm.lookup("FETCH").unwrap();
        let offset_xt = vm.lookup("OFFSET").unwrap();

        // Expected: read I (LIT DictAddr(0) FETCH), then OFFSET base size, then FETCH
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::DictAddr(0),   // address of I
                Cell::Xt(fetch_xt),  // load I
                Cell::Xt(offset_xt), // OFFSET instruction
                Cell::Int(array_base as i64),
                Cell::Int(3),
                Cell::Xt(fetch_xt), // load array[I]
            ],
            "NUMS(I) should compile to index-read + OFFSET + base + size + FETCH"
        );
    }

    // ------------------------------------------------------------------
    // Array address-of: &NUMS(I) compiles to index + OFFSET + base + size (no FETCH)
    // ------------------------------------------------------------------

    #[test]
    fn test_array_address_of_compiles_offset_no_fetch() {
        let mut vm = make_vm();

        // Register variable I at dict[0].
        vm.dict_write(Cell::None).unwrap();
        vm.register(WordEntry::new_variable("I", 0));

        // Register array NUMS(5) starting at dict[1].
        let array_base = vm.dp;
        for _ in 0..5 {
            vm.dict_write(Cell::None).unwrap();
        }
        vm.register(WordEntry::new_array("NUMS", array_base, 5));

        // Compile "&NUMS(I)".
        let tokens = lex("&NUMS(I)");
        let result = ExprCompiler::new(&mut vm).compile_expr(&tokens).unwrap();

        let lit_xt = vm.lookup("LIT").unwrap();
        let fetch_xt = vm.lookup("FETCH").unwrap();
        let offset_xt = vm.lookup("OFFSET").unwrap();

        // Expected: read I, then OFFSET base size — no trailing FETCH.
        assert_eq!(
            result,
            vec![
                Cell::Xt(lit_xt),
                Cell::DictAddr(0),
                Cell::Xt(fetch_xt),
                Cell::Xt(offset_xt),
                Cell::Int(array_base as i64),
                Cell::Int(5),
            ],
            "&NUMS(I) should compile to index-read + OFFSET + base + size (no FETCH)"
        );
    }

    // ------------------------------------------------------------------
    // Array without index syntax should produce an error.
    // ------------------------------------------------------------------

    #[test]
    fn test_bare_array_name_is_error() {
        let mut vm = make_vm();
        let array_base = vm.dp;
        for _ in 0..3 {
            vm.dict_write(Cell::None).unwrap();
        }
        vm.register(WordEntry::new_array("ARR", array_base, 3));

        let tokens = lex("ARR");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "bare array name without () should produce InvalidExpression, got {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // NUMS() with no index should produce a clear error.
    // ------------------------------------------------------------------

    #[test]
    fn test_array_empty_index_is_error() {
        let mut vm = make_vm();
        let array_base = vm.dp;
        for _ in 0..5 {
            vm.dict_write(Cell::None).unwrap();
        }
        vm.register(WordEntry::new_array("NUMS", array_base, 5));

        let tokens = lex("NUMS()");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "NUMS() with no index should produce InvalidExpression, got {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // NUMS(I, J) with multiple indices should produce a clear error.
    // ------------------------------------------------------------------

    #[test]
    fn test_array_multi_index_is_error() {
        let mut vm = make_vm();
        let array_base = vm.dp;
        for _ in 0..5 {
            vm.dict_write(Cell::None).unwrap();
        }
        vm.register(WordEntry::new_array("NUMS", array_base, 5));
        // Also register a variable I so the expression is otherwise valid.
        let i_addr = vm.dp;
        vm.dict_write(Cell::Int(0)).unwrap();
        vm.register(WordEntry::new_variable("I", i_addr));

        let tokens = lex("NUMS(I, I)");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "NUMS(I, I) with multiple indices should produce InvalidExpression, got {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // &NUMS() with no index should produce a clear error.
    // ------------------------------------------------------------------

    #[test]
    fn test_array_address_of_empty_index_is_error() {
        let mut vm = make_vm();
        let array_base = vm.dp;
        for _ in 0..5 {
            vm.dict_write(Cell::None).unwrap();
        }
        vm.register(WordEntry::new_array("NUMS", array_base, 5));

        let tokens = lex("&NUMS()");
        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "&NUMS() with no index should produce InvalidExpression, got {err:?}"
        );
    }

    // ------------------------------------------------------------------
    // Test: Token::Error propagates as InvalidExpression
    // ------------------------------------------------------------------

    /// An unterminated string literal produces Token::Error from the lexer.
    /// compile_expr must propagate that as TbxError::InvalidExpression rather
    /// than silently ignoring it (which would leave the argument list empty and
    /// cause confusing runtime type errors).
    #[test]
    fn test_token_error_propagates_as_invalid_expression() {
        use crate::lexer::Position;
        let mut vm = make_vm();

        // Build a token slice that contains Token::Error directly, mimicking
        // what the lexer emits for an unterminated string literal.
        let error_token = SpannedToken {
            token: Token::Error("unterminated string literal".to_string()),
            pos: Position { line: 1, col: 1 },
            source_offset: 0,
            source_len: 1,
        };
        let tokens = vec![error_token];

        let err = ExprCompiler::new(&mut vm)
            .compile_expr(&tokens)
            .unwrap_err();
        assert!(
            matches!(err, TbxError::InvalidExpression { .. }),
            "Token::Error should produce InvalidExpression, got {err:?}"
        );
    }
}
