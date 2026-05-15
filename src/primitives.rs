use crate::cell::{Cell, CompileEntry, ReturnFrame};
use crate::constants::MAX_DICTIONARY_CELLS;
use crate::dict::{EntryKind, WordEntry, FLAG_IMMEDIATE, FLAG_SYSTEM};
use crate::error::TbxError;
use crate::expr::ExprCompiler;
use crate::lexer::Token;
use crate::vm::{CompileState, VM};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Low-dependency primitives split out into category modules.
// `primitives.rs` remains the façade and registration entry point; the
// `pub use` re-exports keep `crate::primitives::<name>` paths working for
// existing callers and tests.
mod logic;
mod numeric;
mod stack;

pub use logic::*;
pub use numeric::*;
pub use stack::*;

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
        Cell::ArrayAddr { pool_idx, elem_idx } => {
            let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
                index: pool_idx,
                size: vm.arrays.len(),
            })?;
            let size = arr.len();
            if elem_idx >= size {
                return Err(TbxError::ArrayIndexOutOfBounds {
                    index: elem_idx as i64,
                    size,
                });
            }
            let value = arr[elem_idx].clone();
            vm.push(value)?;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// Reference to a pool-managed value.  After #590 D3b the only pool with VM-
/// managed lifetime is the array pool (`Cell::Str` is `Rc<str>`-backed), so a
/// `PoolRef` is simply an array pool index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PoolRef(usize);

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PoolRefLifetime {
    Global,
    CallerOwned,
    FrameLocal,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PoolBounds {
    array_len: usize,
}

fn pool_ref_from_cell(cell: &Cell) -> Option<PoolRef> {
    match cell {
        Cell::Array(idx) => Some(PoolRef(*idx)),
        _ => None,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn current_call_pool_bounds(vm: &VM) -> Option<PoolBounds> {
    vm.return_stack.iter().rev().find_map(|frame| match frame {
        ReturnFrame::Call {
            saved_array_pool_len,
            ..
        } => Some(PoolBounds {
            array_len: *saved_array_pool_len,
        }),
        ReturnFrame::TopLevel => None,
    })
}

impl PoolRef {
    fn index(self) -> usize {
        self.0
    }
}

fn is_global_pool_ref(vm: &VM, pool_ref: PoolRef) -> bool {
    pool_ref.index() < vm.global_array_pool_len
}

fn promote_pool_ref_to_global(vm: &mut VM, pool_ref: PoolRef) {
    vm.global_array_pool_len = vm.global_array_pool_len.max(pool_ref.index() + 1);
}

#[cfg_attr(not(test), allow(dead_code))]
fn classify_pool_ref(vm: &VM, pool_ref: PoolRef) -> PoolRefLifetime {
    if is_global_pool_ref(vm, pool_ref) {
        return PoolRefLifetime::Global;
    }

    let Some(bounds) = current_call_pool_bounds(vm) else {
        // A non-global top-level handle is not yet promoted, so classify it as
        // frame-local/top-level-local until dict-write promotion or top-level exit.
        return PoolRefLifetime::FrameLocal;
    };

    if pool_ref.index() >= bounds.array_len {
        PoolRefLifetime::FrameLocal
    } else {
        PoolRefLifetime::CallerOwned
    }
}

fn check_dict_reference_write(vm: &mut VM, value: &Cell) -> Result<(), TbxError> {
    let Some(pool_ref) = pool_ref_from_cell(value) else {
        return Ok(());
    };

    if is_global_pool_ref(vm, pool_ref) {
        return Ok(());
    }

    if vm.is_executing_top_level() {
        promote_pool_ref_to_global(vm, pool_ref);
        return Ok(());
    }

    Err(TbxError::ArrayFrameEscape)
}

/// STORE — pop addr (top) then value (below), and store value at addr.
///
/// Stack convention: `[..., value, addr]` → STORE → `[...]`
pub fn store_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    let value = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            check_dict_reference_write(vm, &value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            vm.local_write(a, value)?;
            Ok(())
        }
        Cell::ArrayAddr { pool_idx, elem_idx } => {
            write_array_element(vm, pool_idx, elem_idx, value)
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
            check_dict_reference_write(vm, &value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            vm.local_write(a, value)?;
            Ok(())
        }
        Cell::ArrayAddr { pool_idx, elem_idx } => {
            write_array_element(vm, pool_idx, elem_idx, value)
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// Check that `value` is a permitted array element type (static shape check,
/// no VM context required).
///
/// `Cell::Array` is always rejected (nested arrays are not supported).
/// `Cell::Str` is also rejected here: storing strings inside arrays is
/// liberation-tracked separately by #591.  Per-string lifetime classification
/// is no longer expressible at this layer because `Cell::Str` is now
/// `Rc<str>`-backed (it has no pool index), so `check_array_element_write`
/// likewise rejects all `Cell::Str` values.
fn check_array_element_type(value: &Cell) -> Result<(), TbxError> {
    match value {
        Cell::Array(_) => Err(TbxError::InvalidArrayElement { got: "Array" }),
        Cell::Str(_) => Err(TbxError::InvalidArrayElement { got: "Str" }),
        _ => Ok(()),
    }
}

/// Check that `value` may be written to the array at `array_pool_idx`.
///
/// This is the validation counterpart to `check_array_element_type` and is
/// used by `write_array_element` (the `SET`/`STORE` path).
///
/// * `Cell::Array` is always rejected (nested arrays are not supported).
/// * `Cell::Str` is blanket-rejected with `StringFrameEscape` during the
///   transition to `Rc<str>`-backed strings.  Now that `Cell::Str` no longer
///   carries a pool index, the per-string lifetime classification used by
///   the previous Phase 5A matrix is impossible to express here.  Allowing
///   strings into arrays will be done in #591 (which can use Rc-based
///   ownership without lifetime tracking); until then this remains a
///   conservative reject to preserve the pre-Phase-5B "no strings in arrays"
///   contract.
/// * All non-reference scalars are accepted unconditionally.
fn check_array_element_write(
    _vm: &VM,
    _array_pool_idx: usize,
    value: &Cell,
) -> Result<(), TbxError> {
    match value {
        // Nested arrays are unconditionally rejected.
        Cell::Array(_) => Err(TbxError::InvalidArrayElement { got: "Array" }),
        // Blanket reject Cell::Str: lifetime classification is no longer
        // possible with Rc<str>-backed strings, and array-write liberation
        // is tracked separately in #591.
        Cell::Str(_) => Err(TbxError::StringFrameEscape),
        // All other scalar types are accepted.
        _ => Ok(()),
    }
}

/// Validate and (in future phases) transform `value` before it is stored in
/// the array at `array_pool_idx`.
///
/// This is the Phase 5B entry-point for array element writes.
///
/// Current policy (#588 D-1):
///
/// * Nested `Cell::Array` is rejected with `InvalidArrayElement`.
/// * `Cell::Str` is rejected with `StringFrameEscape`.  Now that `Cell::Str`
///   is `Rc<str>`-backed it no longer carries a pool index, so the previous
///   Phase 5A lifetime matrix (FrameLocal/Global/CallerOwned) is no longer
///   expressible at this layer.  Liberation of the array write path for
///   strings is tracked separately by #591, which will allow Rc<str> elements
///   without further lifetime tracking.
/// * All other types are accepted.
///
/// Future phases may intercept specific cases here and return a modified
/// `Cell` (e.g. a wrapped or specialised representation), but the D-1 phase
/// performs validation only — no clone/promote.
///
/// # Errors
///
/// Returns an error when the combination is unsafe or unsupported.
fn stabilize_array_element_write(
    vm: &mut VM,
    array_pool_idx: usize,
    value: Cell,
) -> Result<Cell, TbxError> {
    // Phase 5B-1: validate only; clone/promote will be added in later phases.
    // The immutable borrow of `vm` ends before any future mutable operations.
    check_array_element_write(vm, array_pool_idx, &value)?;
    Ok(value)
}

/// Write `value` to element `elem_idx` of the array at `pool_idx`.
fn write_array_element(
    vm: &mut VM,
    pool_idx: usize,
    elem_idx: usize,
    value: Cell,
) -> Result<(), TbxError> {
    // Validate (and in later phases, transform) the value before storing it.
    // Must be called before get_mut() to avoid borrow conflicts.
    let value = stabilize_array_element_write(vm, pool_idx, value)?;
    let pool_size = vm.arrays.len();
    let arr = vm
        .arrays
        .get_mut(pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: pool_idx,
            size: pool_size,
        })?;
    let size = arr.len();
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx as i64,
            size,
        });
    }
    arr[elem_idx] = value;
    Ok(())
}

/// EQ — equality comparison. Pushes Bool(true) if the two top values are equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
/// Two `Cell::Str` values are compared by string content.
pub fn eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) == *y,
        (Cell::Float(x), Cell::Int(y)) => *x == (*y as f64),
        // Content equality on `Rc<str>` is delegated to its `PartialEq`
        // (which dereferences to `str`); same as `a == b` for the Str pair.
        (Cell::Str(_), Cell::Str(_)) => resolve_str_cell(&a)? == resolve_str_cell(&b)?,
        _ => a == b,
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// NEQ — inequality comparison. Pushes Bool(true) if the two top values are not equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
/// Two `Cell::Str` values are compared by string content.
pub fn neq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) != *y,
        (Cell::Float(x), Cell::Int(y)) => *x != (*y as f64),
        (Cell::Str(_), Cell::Str(_)) => resolve_str_cell(&a)? != resolve_str_cell(&b)?,
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

/// PUTSTR — output the string referenced by a `Cell::Str` on the stack
/// (no newline).
/// Escape sequences (\n, \t, \\) in the stored string are output literally
/// as they were already expanded at compile time.
pub fn putstr_prim(vm: &mut VM) -> Result<(), TbxError> {
    // `pop_string_value` returns the inner `Rc<str>` directly; we only need
    // a `&str` view to write into the output buffer.
    let s = vm.pop_string_value()?;
    vm.write_output(s.as_ref());
    Ok(())
}

/// STR — convert a value to its string representation and push a `Cell::Str` handle.
///
/// Accepts any `Cell` value.
pub fn str_prim(vm: &mut VM) -> Result<(), TbxError> {
    let cell = vm.pop()?;
    let s: std::rc::Rc<str> = match &cell {
        // For Str, reuse the underlying Rc (identity-like conversion).
        Cell::Str(rc) => rc.clone(),
        // For everything else, use Display.
        other => other.to_string().into(),
    };
    vm.push(Cell::Str(s))?;
    Ok(())
}

/// STR_CONCAT — concatenate two strings and push a new `Cell::Str` handle.
///
/// Stack: `[..., a: Str, b: Str]` → `Cell::Str(new)`
pub fn str_concat_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b_cell = vm.pop()?;
    let a_cell = vm.pop()?;
    let b = resolve_str_cell(&b_cell)?;
    let a = resolve_str_cell(&a_cell)?;
    // Concatenation always produces a fresh string; allocate a `String`
    // first to amortise the join, then convert into a new `Rc<str>` for
    // the resulting `Cell::Str`.
    let mut result = String::with_capacity(a.len() + b.len());
    result.push_str(a.as_ref());
    result.push_str(b.as_ref());
    vm.push(Cell::string(result))?;
    Ok(())
}

/// STR_LEN — return the character count of a string.
///
/// Stack: `[..., s: Str]` → `Cell::Int(len)`
///
/// The length is the number of Unicode scalar values (chars), not UTF-8 bytes.
pub fn str_len_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s_cell = vm.pop()?;
    let s = resolve_str_cell(&s_cell)?;
    let len = s.chars().count() as i64;
    vm.push(Cell::Int(len))?;
    Ok(())
}

/// STR_EQ — compare two strings by content and push a `Cell::Bool`.
///
/// Stack: `[..., a: Str, b: Str]` → `Cell::Bool`
///
/// With `Cell::Str` now `Rc<str>`-backed, the `PartialEq` impl on `Cell`
/// already compares string content, so this primitive is effectively
/// equivalent to `EQ` for two `Cell::Str` operands.  It is retained because
/// the language exposes `STR_EQ` as part of the string-manipulation surface.
pub fn str_eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b_cell = vm.pop()?;
    let a_cell = vm.pop()?;
    let b = resolve_str_cell(&b_cell)?;
    let a = resolve_str_cell(&a_cell)?;
    // `Rc<str>` derefs to `str`, so `==` compares string content directly.
    vm.push(Cell::Bool(a == b))?;
    Ok(())
}

/// STR_INDEXOF — find the first occurrence of a substring and return a 1-based position.
///
/// Stack: `[..., haystack: Str, needle: Str]` → `Cell::Int(pos)`
///
/// Returns 0 when the substring is not found. Positions are counted in Unicode
/// scalar values (chars), matching `STR_LEN`.
pub fn str_indexof_prim(vm: &mut VM) -> Result<(), TbxError> {
    let needle_cell = vm.pop()?;
    let haystack_cell = vm.pop()?;
    let needle = resolve_str_cell(&needle_cell)?;
    let haystack = resolve_str_cell(&haystack_cell)?;
    // `Rc<str>` derefs to `&str`, so `find` / slicing work directly without
    // an intermediate `String` allocation.
    let pos = haystack
        .find(needle.as_ref())
        .map(|byte_idx| haystack[..byte_idx].chars().count() as i64 + 1)
        .unwrap_or(0);
    vm.push(Cell::Int(pos))?;
    Ok(())
}

/// STR_SLICE — extract a substring by 1-based start position and length.
///
/// Stack: `[..., s: Str, start: Int, len: Int]` → `Cell::Str(new)`
///
/// Positive `start` counts from the beginning (`1` is the first character).
/// Negative `start` counts from the end (`-1` is the last character). A `start`
/// of `0` is invalid. The result is clipped to the string boundaries.
pub fn str_slice_prim(vm: &mut VM) -> Result<(), TbxError> {
    let len = vm.pop_int()?;
    let start = vm.pop_int()?;
    let s_cell = vm.pop()?;
    if start == 0 {
        return Err(TbxError::InvalidArgument {
            message: "STR_SLICE start must not be 0".to_string(),
        });
    }
    if len < 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("STR_SLICE length must be non-negative, got {len}"),
        });
    }

    let s = resolve_str_cell(&s_cell)?;
    let chars: Vec<char> = s.chars().collect();
    let char_len = chars.len() as i64;
    let raw_start = if start > 0 {
        start - 1
    } else {
        char_len + start
    };
    let start_idx = raw_start.clamp(0, char_len) as usize;
    let end_idx = start_idx.saturating_add(len as usize).min(chars.len());
    let result: String = chars[start_idx..end_idx].iter().collect();

    vm.push(Cell::string(result))?;
    Ok(())
}

/// STR_TRIM — remove leading and trailing Unicode whitespace from a string.
///
/// Stack: `[..., s: Str]` → `Cell::Str(new)`
pub fn str_trim_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s_cell = vm.pop()?;
    let s = resolve_str_cell(&s_cell)?;
    // `s.trim_matches(...)` borrows from the `Rc<str>` and yields `&str`;
    // `Cell::string` allocates a fresh owned string.
    let trimmed = s.trim_matches(char::is_whitespace).to_string();
    vm.push(Cell::string(trimmed))?;
    Ok(())
}

/// STR_UPPER — convert a string to locale-independent Unicode uppercase.
///
/// Stack: `[..., s: Str]` → `Cell::Str(new)`
pub fn str_upper_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s_cell = vm.pop()?;
    let s = resolve_str_cell(&s_cell)?;
    let upper = s.to_uppercase();
    vm.push(Cell::string(upper))?;
    Ok(())
}

/// STR_LOWER — convert a string to locale-independent Unicode lowercase.
///
/// Stack: `[..., s: Str]` → `Cell::Str(new)`
pub fn str_lower_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s_cell = vm.pop()?;
    let s = resolve_str_cell(&s_cell)?;
    let lower = s.to_lowercase();
    vm.push(Cell::string(lower))?;
    Ok(())
}

/// STR_REPLACE_FIRST — replace the first occurrence of a substring.
///
/// Stack: `[..., s: Str, needle: Str, replacement: Str]` → `Cell::Str(new)`
pub fn str_replace_first_prim(vm: &mut VM) -> Result<(), TbxError> {
    let replacement_cell = vm.pop()?;
    let needle_cell = vm.pop()?;
    let s_cell = vm.pop()?;
    let replacement = resolve_str_cell(&replacement_cell)?;
    let needle = resolve_str_cell(&needle_cell)?;
    let s = resolve_str_cell(&s_cell)?;

    if needle.is_empty() {
        return Err(TbxError::InvalidArgument {
            message: "STR_REPLACE_FIRST needle must not be empty".to_string(),
        });
    }

    // When no occurrence is found we can return the same `Rc<str>` by
    // cloning it cheaply (Rc reference-count bump) without allocating a
    // new buffer.
    if let Some(idx) = s.find(needle.as_ref()) {
        let (prefix, rest) = s.split_at(idx);
        let suffix = &rest[needle.len()..];
        let mut result = String::with_capacity(prefix.len() + replacement.len() + suffix.len());
        result.push_str(prefix);
        result.push_str(replacement.as_ref());
        result.push_str(suffix);
        vm.push(Cell::string(result))?;
    } else {
        vm.push(Cell::Str(s))?;
    }
    Ok(())
}

/// STR_REPLACE_ALL — replace all non-overlapping occurrences of a substring.
///
/// Stack: `[..., s: Str, needle: Str, replacement: Str]` → `Cell::Str(new)`
pub fn str_replace_all_prim(vm: &mut VM) -> Result<(), TbxError> {
    let replacement_cell = vm.pop()?;
    let needle_cell = vm.pop()?;
    let s_cell = vm.pop()?;
    let replacement = resolve_str_cell(&replacement_cell)?;
    let needle = resolve_str_cell(&needle_cell)?;
    let s = resolve_str_cell(&s_cell)?;

    if needle.is_empty() {
        return Err(TbxError::InvalidArgument {
            message: "STR_REPLACE_ALL needle must not be empty".to_string(),
        });
    }

    // `str::replace` always allocates a fresh `String`, even when the needle
    // is absent.  We keep that simple behaviour here.
    let result = s.replace(needle.as_ref(), replacement.as_ref());
    vm.push(Cell::string(result))?;
    Ok(())
}

/// Helper: resolve a `Cell::Str` to its inner `Rc<str>` handle.
///
/// `Cell::Str` is `Rc<str>`-backed (#588), so this is a cheap `Rc::clone`
/// rather than a content copy.  Callers that need to mutate or own a
/// `String` should call `.to_string()` on the result.
fn resolve_str_cell(cell: &Cell) -> Result<std::rc::Rc<str>, TbxError> {
    match cell {
        Cell::Str(rc) => Ok(rc.clone()),
        other => Err(TbxError::TypeError {
            expected: "Str",
            got: other.type_name(),
        }),
    }
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
pub fn var_prim(vm: &mut VM) -> Result<(), TbxError> {
    loop {
        // Read the next identifier.
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
            // For variadic words, use the VARIADIC_LOCAL_BASE offset so that local-variable
            // StackAddr indices are in a disjoint range from argument indices.
            // This allows ARG_ADDR to return raw argument indices without ambiguity.
            let idx = if state.is_variadic {
                crate::constants::VARIADIC_LOCAL_BASE + state.local_count
            } else {
                state.arity + state.local_count
            };
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

        // Peek at the next token to decide whether to continue.
        // If it is a comma, consume it and read another identifier.
        // Otherwise push it back and stop.
        match vm.next_token() {
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

/// TO_ARRAY — collect n values from the stack into an array.
///
/// The compiler emits `LIT Int(n)` before the Xt for variadic primitives, so the
/// arity is on top of the stack when this function runs.
///
/// Stack before call: `[arg0, arg1, ..., arg(n-1), Int(n)]`
/// Stack after call:  `[Cell::Array(pool_idx)]`
///
/// The returned `Cell::Array` is bound to the current frame and must not escape.
/// TO_ARRAY with zero arguments (`TO_ARRAY()`) produces an empty array.
pub fn to_array_prim(vm: &mut VM) -> Result<(), TbxError> {
    // Pop the arity pushed by the compiler.
    let n = vm.pop_int()?;
    if n < 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("TO_ARRAY arity must be non-negative, got {n}"),
        });
    }
    let count = n as usize;
    // Pop `count` values in reverse order, then reverse to restore original order.
    let mut elems: Vec<Cell> = Vec::with_capacity(count);
    for _ in 0..count {
        let elem = vm.pop()?;
        // Reject reference types that could dangle when the owning frame is freed.
        check_array_element_type(&elem)?;
        elems.push(elem);
    }
    elems.reverse();
    let pool_idx = vm.arrays.len();
    vm.arrays.push(elems);
    vm.push(Cell::Array(pool_idx))?;
    Ok(())
}

/// FROM_ARRAY — expand an array onto the stack.
///
/// Pops `Cell::Array(pool_idx)` from the stack and pushes every element of the
/// array onto the stack in order (index 0 first).
///
/// Stack before call: `[Cell::Array(pool_idx)]`
/// Stack after call:  `[elem0, elem1, ..., elem(n-1)]`
pub fn from_array_prim(vm: &mut VM) -> Result<(), TbxError> {
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let elems = vm
        .arrays
        .get(pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: pool_idx,
            size: vm.arrays.len(),
        })?
        .clone();
    for elem in elems {
        vm.push(elem)?;
    }
    Ok(())
}

/// ARRAY — create an array of N elements and push its handle onto the stack.
///
/// Pops `Cell::Int(n)` from the stack (n > 0), pushes `n` `Cell::None` elements
/// into `vm.arrays`, and pushes `Cell::Array(pool_idx)` as the handle.
///
/// Arrays created inside a word are bound to that stack frame and freed automatically
/// when the owning word returns (EXIT/RETURN_VAL truncates the pool).
pub fn array_prim(vm: &mut VM) -> Result<(), TbxError> {
    let n = vm.pop_int()?;
    if n <= 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("ARRAY size must be positive, got {n}"),
        });
    }
    let size = n as usize;
    let idx = vm.arrays.len();
    vm.arrays.push(vec![Cell::None; size]);
    vm.push(Cell::Array(idx))?;
    Ok(())
}

/// ARRAY_GET — read an element from an array.
///
/// Stack: `[..., Cell::Array(pool_idx), Cell::Int(elem_idx)]` → `value`
///
/// Array indices are 1-based from the user's perspective: valid range is `1..=N`.
/// The index is translated to 0-based internally before accessing the Vec.
pub fn array_get_prim(vm: &mut VM) -> Result<(), TbxError> {
    let elem_idx_raw = vm.pop_int()?;
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
        index: pool_idx,
        size: vm.arrays.len(),
    })?;
    let size = arr.len();
    // Translate 1-based user index to 0-based internal index.
    // Index 0 or negative is out of bounds.
    if elem_idx_raw < 1 {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let elem_idx = (elem_idx_raw - 1) as usize;
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let value = arr[elem_idx].clone();
    vm.push(value)?;
    Ok(())
}

/// ARRAY_ADDR — compute the address of an array element.
///
/// Stack: `[..., Cell::Array(pool_idx), Cell::Int(elem_idx)]` → `Cell::ArrayAddr { pool_idx, elem_idx }`
///
/// Array indices are 1-based from the user's perspective: valid range is `1..=N`.
/// The index is translated to 0-based internally before storing in `Cell::ArrayAddr`.
pub fn array_addr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let elem_idx_raw = vm.pop_int()?;
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    // Validate bounds at address-computation time.
    let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
        index: pool_idx,
        size: vm.arrays.len(),
    })?;
    let size = arr.len();
    // Translate 1-based user index to 0-based internal index.
    // Index 0 or negative is out of bounds.
    if elem_idx_raw < 1 {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let elem_idx = (elem_idx_raw - 1) as usize;
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    vm.push(Cell::ArrayAddr { pool_idx, elem_idx })?;
    Ok(())
}

/// ARRAY_LEN — return the length of an array.
///
/// Pops `Cell::Array(pool_idx)` from the stack and pushes the number of elements
/// as `Cell::Int`.
///
/// Stack: `[..., Cell::Array(pool_idx)]` → `Cell::Int(len)`
pub fn array_len_prim(vm: &mut VM) -> Result<(), TbxError> {
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
        index: pool_idx,
        size: vm.arrays.len(),
    })?;
    let len = arr.len() as i64;
    vm.push(Cell::Int(len))?;
    Ok(())
}

/// ARRAY_CONCAT — concatenate two arrays and return a new array.
///
/// Pops two `Cell::Array` handles from the stack and pushes a new array whose
/// contents are all elements of `a` followed by all elements of `b`.
///
/// Stack: `[..., a: Array, b: Array]` → `Cell::Array(new_idx)`
pub fn array_concat_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b_pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let a_pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };

    let a = vm
        .arrays
        .get(a_pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: a_pool_idx,
            size: vm.arrays.len(),
        })?;
    let b = vm
        .arrays
        .get(b_pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: b_pool_idx,
            size: vm.arrays.len(),
        })?;

    let mut result: Vec<Cell> = Vec::with_capacity(a.len() + b.len());
    result.extend_from_slice(a);
    result.extend_from_slice(b);

    let pool_idx = vm.arrays.len();
    vm.arrays.push(result);
    vm.push(Cell::Array(pool_idx))?;
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

/// SHUFFLE — randomly permute the elements of an array in place.
///
/// Pops `Cell::Array(pool_idx)` from the stack, shuffles the array's elements
/// using the VM's RNG, and pushes the same `Cell::Array(pool_idx)` back.
///
/// Stack signature: `( arr:Array -- arr:Array )`
pub fn shuffle_prim(vm: &mut VM) -> Result<(), TbxError> {
    use rand::seq::SliceRandom;
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let arrays_len = vm.arrays.len();
    let arr = vm
        .arrays
        .get_mut(pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: pool_idx,
            size: arrays_len,
        })?;
    arr.shuffle(&mut vm.rng);
    vm.push(Cell::Array(pool_idx))
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
    // ARRAY creates an array; ARRAY_GET reads an element; ARRAY_ADDR computes
    // an element address (used internally by the expression compiler for `A(I)` and `&A(I)`).
    // TO_ARRAY packs stack values into a new array; FROM_ARRAY expands one onto the stack.
    // ARRAY_LEN returns the length of an array; ARRAY_CONCAT concatenates two arrays.
    let mut to_array_entry = WordEntry::new_primitive("TO_ARRAY", to_array_prim);
    to_array_entry.is_variadic = true;
    // arity stays 0: TO_ARRAY accepts zero or more arguments.
    vm.register(to_array_entry);
    vm.register(WordEntry::new_primitive("FROM_ARRAY", from_array_prim));
    vm.register(WordEntry::new_primitive("ARRAY", array_prim));
    vm.register(WordEntry::new_primitive("ARRAY_LEN", array_len_prim));
    vm.register(WordEntry::new_primitive("ARRAY_CONCAT", array_concat_prim));
    let mut array_get_entry = WordEntry::new_primitive("ARRAY_GET", array_get_prim);
    array_get_entry.flags = FLAG_SYSTEM;
    vm.register(array_get_entry);
    let mut array_addr_entry = WordEntry::new_primitive("ARRAY_ADDR", array_addr_prim);
    array_addr_entry.flags = FLAG_SYSTEM;
    vm.register(array_addr_entry);

    // Variadic argument primitives.
    // VA_COUNT returns the total argument count of the current call.
    // ARG_ADDR converts an argument index to a StackAddr for FETCH/STORE.
    vm.register(WordEntry::new_primitive("VA_COUNT", va_count_prim));
    vm.register(WordEntry::new_primitive("ARG_ADDR", arg_addr_prim));

    // Random number primitives.
    // RND(n) returns a random integer in [1, n]; RANDOMIZE re-seeds the RNG from OS entropy;
    // SHUFFLE permutes an array in place and returns the same array handle.
    vm.register(WordEntry::new_primitive("RND", rnd_prim));
    vm.register(WordEntry::new_primitive("RANDOMIZE", randomize_prim));
    vm.register(WordEntry::new_primitive("SHUFFLE", shuffle_prim));

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

    // --- PUTSTR tests ---

    #[test]
    fn test_putstr_basic() {
        let mut vm = VM::new();
        vm.push(Cell::string("hello")).unwrap();
        putstr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "hello");
    }

    #[test]
    fn test_putstr_empty() {
        let mut vm = VM::new();
        vm.push(Cell::string("")).unwrap();
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

    // --- str_prim tests ---

    #[test]
    fn test_str_prim_from_int() {
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        str_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("42"));
    }

    #[test]
    fn test_str_prim_from_float() {
        let mut vm = VM::new();
        vm.push(Cell::Float(1.5)).unwrap();
        str_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("1.5"));
    }

    #[test]
    fn test_str_prim_from_bool() {
        let mut vm = VM::new();
        vm.push(Cell::Bool(true)).unwrap();
        str_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("true"));
    }

    #[test]
    fn test_str_prim_from_cell_str() {
        let mut vm = VM::new();
        vm.push(Cell::string("existing")).unwrap();
        str_prim(&mut vm).unwrap();
        // STR on a Str returns the same content (Rc clone, no copy).
        assert_eq!(vm.pop().unwrap(), Cell::string("existing"));
    }

    #[test]
    fn test_str_prim_underflow() {
        let mut vm = VM::new();
        assert_eq!(str_prim(&mut vm), Err(TbxError::StackUnderflow));
    }

    // --- str_concat_prim tests ---

    #[test]
    fn test_str_concat_basic() {
        let mut vm = VM::new();
        vm.push(Cell::string("foo")).unwrap();
        vm.push(Cell::string("bar")).unwrap();
        str_concat_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("foobar"));
    }

    #[test]
    fn test_str_concat_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::string("foo")).unwrap();
        vm.push(Cell::Int(42)).unwrap();
        assert!(matches!(
            str_concat_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // ----------------------------------------------------------------
    // D-2 (#589) regression tests: confirm string primitives operate on
    // `Cell::Str(Rc<str>)` without going through `VM::strings`.
    // ----------------------------------------------------------------

    #[test]
    fn test_d2_str_prim_pushes_cell_str_with_rc() {
        // `STR` on an Int produces a `Cell::Str` carrying an `Rc<str>` that
        // matches the decimal rendering of the input.
        let mut vm = VM::new();
        vm.push(Cell::Int(123)).unwrap();
        str_prim(&mut vm).unwrap();
        let cell = vm.pop().unwrap();
        match &cell {
            Cell::Str(rc) => assert_eq!(rc.as_ref(), "123"),
            other => panic!("expected Cell::Str, got {other:?}"),
        }
    }

    #[test]
    fn test_d2_str_prim_on_str_reuses_underlying_rc() {
        // `STR` on a `Cell::Str` should reuse the underlying `Rc<str>` (an
        // identity-like conversion) rather than allocating a new buffer.
        let mut vm = VM::new();
        let original: std::rc::Rc<str> = "shared".into();
        vm.push(Cell::Str(original.clone())).unwrap();
        str_prim(&mut vm).unwrap();
        match vm.pop().unwrap() {
            Cell::Str(rc) => {
                assert!(
                    std::rc::Rc::ptr_eq(&rc, &original),
                    "STR on Cell::Str should reuse the inner Rc, not allocate"
                );
            }
            other => panic!("expected Cell::Str, got {other:?}"),
        }
    }

    #[test]
    fn test_d2_str_concat_produces_fresh_rc_with_combined_content() {
        // `STR_CONCAT` produces a new `Cell::Str` whose content is the
        // concatenation of the two operands.  The resulting Rc must own
        // its own buffer (it cannot alias either input by `Rc::ptr_eq`).
        let mut vm = VM::new();
        let left: std::rc::Rc<str> = "foo".into();
        let right: std::rc::Rc<str> = "bar".into();
        vm.push(Cell::Str(left.clone())).unwrap();
        vm.push(Cell::Str(right.clone())).unwrap();
        str_concat_prim(&mut vm).unwrap();
        match vm.pop().unwrap() {
            Cell::Str(rc) => {
                assert_eq!(rc.as_ref(), "foobar");
                assert!(!std::rc::Rc::ptr_eq(&rc, &left));
                assert!(!std::rc::Rc::ptr_eq(&rc, &right));
            }
            other => panic!("expected Cell::Str, got {other:?}"),
        }
    }

    #[test]
    fn test_d2_pop_string_value_returns_underlying_rc() {
        // `pop_string_value` returns the inner `Rc<str>`; the handle must
        // be the very Rc that was pushed on the stack (no copy).
        let mut vm = VM::new();
        let original: std::rc::Rc<str> = "abc".into();
        vm.push(Cell::Str(original.clone())).unwrap();
        let popped = vm.pop_string_value().expect("expected Cell::Str on stack");
        assert!(
            std::rc::Rc::ptr_eq(&popped, &original),
            "pop_string_value should return the same Rc that was pushed"
        );
    }

    #[test]
    fn test_d2_putstr_emits_rc_backed_literal_content() {
        // End-to-end check: a Cell::Str carrying an Rc<str> with literal
        // content is correctly emitted by PUTSTR through the output buffer.
        let mut vm = VM::new();
        vm.push(Cell::string("rc-literal")).unwrap();
        putstr_prim(&mut vm).unwrap();
        assert_eq!(vm.take_output(), "rc-literal");
    }

    // --- str_len_prim tests ---

    #[test]
    fn test_str_len_basic() {
        let mut vm = VM::new();
        vm.push(Cell::string("hello")).unwrap();
        str_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(5));
    }

    #[test]
    fn test_str_len_empty() {
        let mut vm = VM::new();
        vm.push(Cell::string("")).unwrap();
        str_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(0));
    }

    #[test]
    fn test_str_len_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        assert!(matches!(
            str_len_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- str_eq_prim tests ---

    #[test]
    fn test_str_eq_equal() {
        let mut vm = VM::new();
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::string("hello")).unwrap();
        str_eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Bool(true));
    }

    #[test]
    fn test_str_eq_not_equal() {
        let mut vm = VM::new();
        vm.push(Cell::string("foo")).unwrap();
        vm.push(Cell::string("bar")).unwrap();
        str_eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Bool(false));
    }

    #[test]
    fn test_str_eq_same_rc_is_equal() {
        // Two Cell::Str that share the same Rc compare equal.
        let mut vm = VM::new();
        let s: std::rc::Rc<str> = "x".into();
        vm.push(Cell::Str(s.clone())).unwrap();
        vm.push(Cell::Str(s)).unwrap();
        str_eq_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Bool(true));
    }

    // --- str_indexof_prim tests ---

    #[test]
    fn test_str_indexof_found_returns_1_based_position() {
        let mut vm = VM::new();
        vm.push(Cell::string("hello world")).unwrap();
        vm.push(Cell::string("world")).unwrap();
        str_indexof_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(7));
    }

    #[test]
    fn test_str_indexof_not_found_returns_zero() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::string("z")).unwrap();
        str_indexof_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(0));
    }

    #[test]
    fn test_str_indexof_counts_unicode_chars() {
        let mut vm = VM::new();
        vm.push(Cell::string("あいうえお")).unwrap();
        vm.push(Cell::string("うえ")).unwrap();
        str_indexof_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(3));
    }

    #[test]
    fn test_str_indexof_empty_needle_returns_one() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::string("")).unwrap();
        str_indexof_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::Int(1));
    }

    #[test]
    fn test_str_indexof_type_error() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(
            str_indexof_prim(&mut vm),
            Err(TbxError::TypeError { .. })
        ));
    }

    // --- str_slice_prim tests ---

    #[test]
    fn test_str_slice_basic() {
        let mut vm = VM::new();
        vm.push(Cell::string("abcdef")).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        str_slice_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("bcd"));
    }

    #[test]
    fn test_str_slice_negative_start_counts_from_end() {
        let mut vm = VM::new();
        vm.push(Cell::string("abcdef")).unwrap();
        vm.push(Cell::Int(-3)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        str_slice_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("de"));
    }

    #[test]
    fn test_str_slice_clips_past_end() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(10)).unwrap();
        str_slice_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("bc"));
    }

    #[test]
    fn test_str_slice_too_negative_start_clips_to_beginning() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::Int(-10)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        str_slice_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("ab"));
    }

    #[test]
    fn test_str_slice_counts_unicode_chars() {
        let mut vm = VM::new();
        vm.push(Cell::string("あいうえお")).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        str_slice_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("いう"));
    }

    #[test]
    fn test_str_slice_zero_length_returns_empty_string() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        str_slice_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string(""));
    }

    #[test]
    fn test_str_slice_start_zero_is_invalid_argument() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(
            str_slice_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_str_slice_negative_length_is_invalid_argument() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(-1)).unwrap();
        assert!(matches!(
            str_slice_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    // --- str_trim_prim tests ---

    #[test]
    fn test_str_trim_removes_ascii_spaces() {
        let mut vm = VM::new();
        vm.push(Cell::string("  hello  ")).unwrap();
        str_trim_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("hello"));
    }

    #[test]
    fn test_str_trim_removes_unicode_whitespace() {
        let mut vm = VM::new();
        vm.push(Cell::string("\u{3000}abc\u{3000}")).unwrap();
        str_trim_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("abc"));
    }

    #[test]
    fn test_str_trim_keeps_inner_whitespace() {
        let mut vm = VM::new();
        vm.push(Cell::string("  hello world  ")).unwrap();
        str_trim_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("hello world"));
    }

    #[test]
    fn test_str_trim_all_whitespace_becomes_empty() {
        let mut vm = VM::new();
        vm.push(Cell::string("\n\t\u{3000}")).unwrap();
        str_trim_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string(""));
    }

    // --- str_upper_prim tests ---

    #[test]
    fn test_str_upper_ascii() {
        let mut vm = VM::new();
        vm.push(Cell::string("Abc123")).unwrap();
        str_upper_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("ABC123"));
    }

    #[test]
    fn test_str_upper_unicode_can_change_length() {
        let mut vm = VM::new();
        vm.push(Cell::string("straße")).unwrap();
        str_upper_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("STRASSE"));
    }

    // --- str_lower_prim tests ---

    #[test]
    fn test_str_lower_ascii() {
        let mut vm = VM::new();
        vm.push(Cell::string("AbC123")).unwrap();
        str_lower_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("abc123"));
    }

    #[test]
    fn test_str_lower_unicode() {
        let mut vm = VM::new();
        vm.push(Cell::string("ÄÖÜ")).unwrap();
        str_lower_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("äöü"));
    }

    // --- str_replace_first_prim tests ---

    #[test]
    fn test_str_replace_first_replaces_only_first_match() {
        let mut vm = VM::new();
        vm.push(Cell::string("abcabc")).unwrap();
        vm.push(Cell::string("ab")).unwrap();
        vm.push(Cell::string("X")).unwrap();
        str_replace_first_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("Xcabc"));
    }

    #[test]
    fn test_str_replace_first_returns_copy_when_not_found() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::string("z")).unwrap();
        vm.push(Cell::string("X")).unwrap();
        str_replace_first_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("abc"));
    }

    #[test]
    fn test_str_replace_first_handles_unicode() {
        let mut vm = VM::new();
        vm.push(Cell::string("あいうあい")).unwrap();
        vm.push(Cell::string("あい")).unwrap();
        vm.push(Cell::string("x")).unwrap();
        str_replace_first_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("xうあい"));
    }

    #[test]
    fn test_str_replace_first_empty_needle_is_invalid_argument() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::string("")).unwrap();
        vm.push(Cell::string("X")).unwrap();
        assert!(matches!(
            str_replace_first_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    // --- str_replace_all_prim tests ---

    #[test]
    fn test_str_replace_all_replaces_all_matches() {
        let mut vm = VM::new();
        vm.push(Cell::string("abcabc")).unwrap();
        vm.push(Cell::string("ab")).unwrap();
        vm.push(Cell::string("X")).unwrap();
        str_replace_all_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("XcXc"));
    }

    #[test]
    fn test_str_replace_all_returns_copy_when_not_found() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::string("z")).unwrap();
        vm.push(Cell::string("X")).unwrap();
        str_replace_all_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("abc"));
    }

    #[test]
    fn test_str_replace_all_handles_unicode() {
        let mut vm = VM::new();
        vm.push(Cell::string("あいうあい")).unwrap();
        vm.push(Cell::string("あい")).unwrap();
        vm.push(Cell::string("x")).unwrap();
        str_replace_all_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("xうx"));
    }

    #[test]
    fn test_str_replace_all_uses_non_overlapping_matches() {
        let mut vm = VM::new();
        vm.push(Cell::string("aaaa")).unwrap();
        vm.push(Cell::string("aa")).unwrap();
        vm.push(Cell::string("b")).unwrap();
        str_replace_all_prim(&mut vm).unwrap();
        assert_eq!(vm.pop().unwrap(), Cell::string("bb"));
    }

    #[test]
    fn test_str_replace_all_empty_needle_is_invalid_argument() {
        let mut vm = VM::new();
        vm.push(Cell::string("abc")).unwrap();
        vm.push(Cell::string("")).unwrap();
        vm.push(Cell::string("X")).unwrap();
        assert!(matches!(
            str_replace_all_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
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

    #[test]
    fn test_pool_ref_from_cell_array_only() {
        // PoolRef tracks array pool entries only; `Cell::Str` is `Rc<str>`-backed
        // and produces no PoolRef.
        assert_eq!(pool_ref_from_cell(&Cell::Array(3)), Some(PoolRef(3)));
        assert_eq!(pool_ref_from_cell(&Cell::string("hello")), None);
        assert_eq!(
            pool_ref_from_cell(&Cell::ArrayAddr {
                pool_idx: 1,
                elem_idx: 2,
            }),
            None
        );
        assert_eq!(pool_ref_from_cell(&Cell::DictAddr(0)), None);
        assert_eq!(pool_ref_from_cell(&Cell::StackAddr(0)), None);
    }

    #[test]
    fn test_promote_pool_ref_to_global_never_moves_boundary_backward() {
        let mut vm = VM::new();

        vm.global_array_pool_len = 5;
        promote_pool_ref_to_global(&mut vm, PoolRef(1));
        assert_eq!(vm.global_array_pool_len, 5);
        promote_pool_ref_to_global(&mut vm, PoolRef(7));
        assert_eq!(vm.global_array_pool_len, 8);
    }

    #[test]
    fn test_current_call_pool_bounds_uses_innermost_call() {
        let mut vm = VM::new();
        vm.return_stack.push(ReturnFrame::TopLevel);
        vm.return_stack.push(ReturnFrame::Call {
            callee_xt: crate::cell::Xt(0),
            return_pc: 0,
            saved_bp: 0,
            saved_array_pool_len: 3,
            actual_arity: 0,
        });
        vm.return_stack.push(ReturnFrame::Call {
            callee_xt: crate::cell::Xt(1),
            return_pc: 0,
            saved_bp: 0,
            saved_array_pool_len: 7,
            actual_arity: 0,
        });

        assert_eq!(
            current_call_pool_bounds(&vm),
            Some(PoolBounds { array_len: 7 })
        );
    }

    #[test]
    fn test_classify_pool_ref_global() {
        let mut vm = VM::new();
        vm.global_array_pool_len = 1;

        assert_eq!(classify_pool_ref(&vm, PoolRef(0)), PoolRefLifetime::Global);
    }

    #[test]
    fn test_classify_pool_ref_caller_owned() {
        let mut vm = VM::new();
        vm.return_stack.push(ReturnFrame::TopLevel);
        vm.return_stack.push(ReturnFrame::Call {
            callee_xt: crate::cell::Xt(0),
            return_pc: 0,
            saved_bp: 0,
            saved_array_pool_len: 5,
            actual_arity: 0,
        });

        assert_eq!(
            classify_pool_ref(&vm, PoolRef(4)),
            PoolRefLifetime::CallerOwned
        );
    }

    #[test]
    fn test_classify_pool_ref_frame_local_in_call_frame() {
        let mut vm = VM::new();
        vm.return_stack.push(ReturnFrame::TopLevel);
        vm.return_stack.push(ReturnFrame::Call {
            callee_xt: crate::cell::Xt(0),
            return_pc: 0,
            saved_bp: 0,
            saved_array_pool_len: 5,
            actual_arity: 0,
        });

        assert_eq!(
            classify_pool_ref(&vm, PoolRef(5)),
            PoolRefLifetime::FrameLocal
        );
    }

    #[test]
    fn test_classify_pool_ref_top_level_non_global_is_frame_local() {
        let mut vm = VM::new();
        vm.return_stack.push(ReturnFrame::TopLevel);

        assert_eq!(
            classify_pool_ref(&vm, PoolRef(0)),
            PoolRefLifetime::FrameLocal
        );
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
        vm.push(Cell::Int(3)).unwrap();
        array_prim(&mut vm).unwrap();
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

    // --- array_prim ---

    #[test]
    fn test_array_prim_creates_array() {
        let mut vm = VM::new();
        vm.push(Cell::Int(3)).unwrap();
        array_prim(&mut vm).unwrap();
        // Stack should contain Cell::Array(0) — first pool slot.
        assert_eq!(vm.pop(), Ok(Cell::Array(0)));
        // The pool should have one entry of length 3.
        assert_eq!(vm.arrays.len(), 1);
        assert_eq!(vm.arrays[0].len(), 3);
    }

    #[test]
    fn test_array_prim_initialises_to_none() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        array_prim(&mut vm).unwrap();
        vm.pop().unwrap(); // discard handle
        assert_eq!(vm.arrays[0][0], Cell::None);
        assert_eq!(vm.arrays[0][1], Cell::None);
    }

    #[test]
    fn test_array_prim_size_zero_returns_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            array_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_array_prim_negative_size_returns_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap();
        assert!(matches!(
            array_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_array_prim_multiple_arrays_get_distinct_indices() {
        let mut vm = VM::new();
        vm.push(Cell::Int(2)).unwrap();
        array_prim(&mut vm).unwrap();
        vm.push(Cell::Int(4)).unwrap();
        array_prim(&mut vm).unwrap();
        let second = vm.pop().unwrap();
        let first = vm.pop().unwrap();
        assert_eq!(first, Cell::Array(0));
        assert_eq!(second, Cell::Array(1));
    }

    // --- array_get_prim ---

    #[test]
    fn test_array_get_prim_reads_element() {
        // User index 1 maps to internal index 0.
        let mut vm = VM::new();
        vm.arrays
            .push(vec![Cell::Int(10), Cell::Int(20), Cell::Int(30)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        array_get_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(10)));
    }

    #[test]
    fn test_array_get_prim_reads_second_element() {
        // User index 2 maps to internal index 1.
        let mut vm = VM::new();
        vm.arrays
            .push(vec![Cell::Int(10), Cell::Int(20), Cell::Int(30)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        array_get_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_array_get_prim_out_of_bounds() {
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::Int(1)]);
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
        vm.arrays.push(vec![Cell::Int(1)]);
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
        vm.arrays.push(vec![Cell::Int(1)]);
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
        vm.arrays.push(vec![Cell::Int(0), Cell::Int(0)]);
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
        vm.arrays.push(vec![Cell::Int(0), Cell::Int(0)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            array_addr_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: 0, .. })
        ));
    }

    // --- store_prim with ArrayFrameEscape guard ---

    #[test]
    fn test_store_array_to_dict_addr_is_escape_error() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::None); // dict[0] = placeholder
                                        // Try to store Cell::Array(0) into a global variable slot.
        vm.push(Cell::Array(0)).unwrap(); // value
        vm.push(Cell::DictAddr(0)).unwrap(); // address
        assert_eq!(store_prim(&mut vm), Err(TbxError::ArrayFrameEscape));
    }

    #[test]
    fn test_set_array_to_dict_addr_is_escape_error() {
        let mut vm = VM::new();
        vm.dictionary.push(Cell::None); // dict[0] = placeholder
                                        // set_prim: stack is [..., addr, value]
        vm.push(Cell::DictAddr(0)).unwrap(); // address
        vm.push(Cell::Array(0)).unwrap(); // value
        assert_eq!(set_prim(&mut vm), Err(TbxError::ArrayFrameEscape));
    }

    // --- store/set to ArrayAddr ---

    #[test]
    fn test_store_to_array_addr() {
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::None, Cell::None]);
        vm.push(Cell::Int(99)).unwrap();
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 1,
        })
        .unwrap();
        store_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0][1], Cell::Int(99));
    }

    #[test]
    fn test_set_to_array_addr() {
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::None, Cell::None]);
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::Int(42)).unwrap();
        set_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0][0], Cell::Int(42));
    }

    // --- array element write: Cell::Str ---
    //
    // After #588 (D-1), `Cell::Str` is `Rc<str>`-backed and `check_array_element_write`
    // blanket-rejects any string with `StringFrameEscape` (see the
    // function-level comment).  The lifetime-distinction tests below were
    // structured around the pool-index model and are deferred until #591
    // (which can use Rc-based ownership without lifetime tracking) revisits
    // the array-write policy.

    #[test]
    #[ignore = "#591: blanket reject in D-1; per-lifetime allow rules will be reintroduced when the array-write path is liberated."]
    fn test_set_global_str_to_array_element_is_allowed() {
        // Pre-#588: a globally-promoted string could be stored into any array.
        // Now `check_array_element_write` rejects all `Cell::Str` values; the
        // successor test will simply assert that `Cell::string(...)` can be
        // stored into a frame-local array via Rc-based ownership (#591).
    }

    #[test]
    #[ignore = "#591: blanket reject in D-1; see test_set_global_str_to_array_element_is_allowed."]
    fn test_store_global_str_to_array_element_is_allowed() {}

    #[test]
    fn test_set_str_to_array_element_is_string_frame_escape() {
        // D-1 blanket reject: any `Cell::Str` written through `SET` to an
        // array element fails with `StringFrameEscape`, regardless of the
        // string's origin (frame-local, caller-owned, or globally-promoted).
        // This conservatively preserves the pre-Phase-5B "no strings in
        // arrays" contract until #591 liberates the path.
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::None]);
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::string("local")).unwrap();
        assert_eq!(set_prim(&mut vm), Err(TbxError::StringFrameEscape));
    }

    #[test]
    fn test_store_str_to_array_element_is_string_frame_escape() {
        // Same as the SET path above, exercised through STORE.
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::None]);
        vm.push(Cell::string("local")).unwrap();
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        assert_eq!(store_prim(&mut vm), Err(TbxError::StringFrameEscape));
    }

    #[test]
    #[ignore = "#591: requires lifetime-aware allow rules that depend on per-string pool indices, which Rc<str> removes."]
    fn test_set_caller_owned_str_to_global_array_element_is_string_frame_escape() {}

    #[test]
    #[ignore = "#591: requires lifetime-aware allow rules that depend on per-string pool indices, which Rc<str> removes."]
    fn test_set_caller_owned_str_to_frame_local_array_element_is_allowed() {}

    #[test]
    #[ignore = "#591: requires lifetime-aware allow rules that depend on per-string pool indices, which Rc<str> removes."]
    fn test_set_caller_owned_str_to_caller_owned_array_element_is_string_frame_escape() {}

    #[test]
    fn test_set_nested_array_to_array_element_is_invalid_array_element() {
        // Cell::Array must always be rejected as an array element.
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::None]); // pool_idx = 0: target array
        vm.arrays.push(vec![Cell::None]); // pool_idx = 1: value to store
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
        vm.arrays.push(vec![Cell::Int(77)]);
        vm.push(Cell::ArrayAddr {
            pool_idx: 0,
            elem_idx: 0,
        })
        .unwrap();
        fetch_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(77)));
    }

    // --- to_array_prim ---

    #[test]
    fn test_to_array_prim_basic() {
        // Stack: [1, 2, 3, Int(3)] → Cell::Array(0) with elems [1, 2, 3]
        let mut vm = VM::new();
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(3)).unwrap();
        vm.push(Cell::Int(3)).unwrap(); // arity
        to_array_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Array(0)));
        assert_eq!(vm.arrays[0], vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]);
    }

    #[test]
    fn test_to_array_prim_empty() {
        // Stack: [Int(0)] → Cell::Array(0) with empty vec
        let mut vm = VM::new();
        vm.push(Cell::Int(0)).unwrap(); // arity = 0
        to_array_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Array(0)));
        assert!(vm.arrays[0].is_empty());
    }

    #[test]
    fn test_to_array_prim_single_element() {
        // Stack: [Int(42), Int(1)] → Cell::Array(0) with elems [42]
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        vm.push(Cell::Int(1)).unwrap(); // arity
        to_array_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Array(0)));
        assert_eq!(vm.arrays[0], vec![Cell::Int(42)]);
    }

    #[test]
    fn test_to_array_prim_preserves_order() {
        // Ensure push order: first arg (10) → index 0, last arg (30) → index 2.
        let mut vm = VM::new();
        vm.push(Cell::Int(10)).unwrap();
        vm.push(Cell::Int(20)).unwrap();
        vm.push(Cell::Int(30)).unwrap();
        vm.push(Cell::Int(3)).unwrap(); // arity
        to_array_prim(&mut vm).unwrap();
        let _ = vm.pop().unwrap(); // discard Cell::Array handle
        assert_eq!(vm.arrays[0][0], Cell::Int(10));
        assert_eq!(vm.arrays[0][1], Cell::Int(20));
        assert_eq!(vm.arrays[0][2], Cell::Int(30));
    }

    #[test]
    fn test_to_array_prim_negative_arity_returns_error() {
        let mut vm = VM::new();
        vm.push(Cell::Int(-1)).unwrap(); // negative arity
        assert!(matches!(
            to_array_prim(&mut vm),
            Err(TbxError::InvalidArgument { .. })
        ));
    }

    #[test]
    fn test_to_array_prim_multiple_calls_get_distinct_pool_indices() {
        let mut vm = VM::new();
        // First call: TO_ARRAY(1) → Array(0)
        vm.push(Cell::Int(1)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        to_array_prim(&mut vm).unwrap();
        // Second call: TO_ARRAY(2) → Array(1)
        vm.push(Cell::Int(2)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        to_array_prim(&mut vm).unwrap();
        let second = vm.pop().unwrap();
        let first = vm.pop().unwrap();
        assert_eq!(first, Cell::Array(0));
        assert_eq!(second, Cell::Array(1));
    }

    // --- from_array_prim ---

    #[test]
    fn test_from_array_prim_pushes_elements_in_order() {
        // Array [7, 8, 9]: FROM_ARRAY should push 7, 8, 9 (7 first, 9 last/top).
        let mut vm = VM::new();
        vm.arrays
            .push(vec![Cell::Int(7), Cell::Int(8), Cell::Int(9)]);
        vm.push(Cell::Array(0)).unwrap();
        from_array_prim(&mut vm).unwrap();
        // Stack should now be [7, 8, 9] (top = 9).
        assert_eq!(vm.pop(), Ok(Cell::Int(9)));
        assert_eq!(vm.pop(), Ok(Cell::Int(8)));
        assert_eq!(vm.pop(), Ok(Cell::Int(7)));
    }

    #[test]
    fn test_from_array_prim_empty_array_pushes_nothing() {
        let mut vm = VM::new();
        vm.arrays.push(vec![]);
        let stack_depth_before = vm.data_stack.len();
        vm.push(Cell::Array(0)).unwrap();
        from_array_prim(&mut vm).unwrap();
        // Stack depth must be equal to before (the handle was consumed, nothing added).
        assert_eq!(vm.data_stack.len(), stack_depth_before);
    }

    #[test]
    fn test_from_array_prim_type_error_on_non_array() {
        // Passing an Int where Array is expected must return TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        assert!(matches!(
            from_array_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Array",
                ..
            })
        ));
    }

    #[test]
    fn test_from_array_prim_to_array_roundtrip() {
        // TO_ARRAY then FROM_ARRAY must restore the original elements.
        let mut vm = VM::new();
        vm.push(Cell::Int(100)).unwrap();
        vm.push(Cell::Int(200)).unwrap();
        vm.push(Cell::Int(300)).unwrap();
        vm.push(Cell::Int(3)).unwrap(); // arity
        to_array_prim(&mut vm).unwrap(); // → Cell::Array(0) on stack
        from_array_prim(&mut vm).unwrap(); // consume Array, push 100, 200, 300
        assert_eq!(vm.pop(), Ok(Cell::Int(300)));
        assert_eq!(vm.pop(), Ok(Cell::Int(200)));
        assert_eq!(vm.pop(), Ok(Cell::Int(100)));
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
        vm.arrays.push(vec![Cell::None; 5]);
        vm.push(Cell::Array(0)).unwrap();
        array_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(5)));
    }

    #[test]
    fn test_array_len_prim_one_element() {
        // ARRAY_LEN on a 1-element array must return 1.
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::None]);
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

    // --- array_concat_prim ---

    #[test]
    fn test_array_concat_prim_concatenates_in_order() {
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::Int(1), Cell::Int(2)]);
        vm.arrays.push(vec![Cell::Int(3), Cell::Int(4)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Array(1)).unwrap();
        array_concat_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Array(2)));
        assert_eq!(
            vm.arrays[2],
            vec![Cell::Int(1), Cell::Int(2), Cell::Int(3), Cell::Int(4)]
        );
    }

    #[test]
    fn test_array_concat_prim_allows_empty_arrays() {
        let mut vm = VM::new();
        vm.arrays.push(Vec::new());
        vm.arrays.push(vec![Cell::Int(7)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Array(1)).unwrap();
        array_concat_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Array(2)));
        assert_eq!(vm.arrays[2], vec![Cell::Int(7)]);
    }

    #[test]
    fn test_array_concat_prim_does_not_mutate_inputs() {
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::Int(10)]);
        vm.arrays.push(vec![Cell::Int(20)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Array(1)).unwrap();
        array_concat_prim(&mut vm).unwrap();
        assert_eq!(vm.arrays[0], vec![Cell::Int(10)]);
        assert_eq!(vm.arrays[1], vec![Cell::Int(20)]);
    }

    #[test]
    fn test_array_concat_prim_type_error_on_non_array_rhs() {
        let mut vm = VM::new();
        vm.arrays.push(vec![Cell::Int(1)]);
        vm.push(Cell::Array(0)).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        assert!(matches!(
            array_concat_prim(&mut vm),
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

    // --- shuffle_prim ---

    #[test]
    fn test_shuffle_prim_preserves_elements() {
        // SHUFFLE must not add or remove elements from the array.
        use rand::SeedableRng;
        let mut vm = VM::new();
        vm.rng = rand::rngs::SmallRng::seed_from_u64(123);
        vm.arrays.push(vec![
            Cell::Int(1),
            Cell::Int(2),
            Cell::Int(3),
            Cell::Int(4),
            Cell::Int(5),
        ]);
        vm.push(Cell::Array(0)).unwrap();
        shuffle_prim(&mut vm).unwrap();
        // The returned value must be Cell::Array(0).
        assert_eq!(vm.pop(), Ok(Cell::Array(0)));
        // All original elements must still be present (order may differ).
        let mut values: Vec<i64> = vm.arrays[0]
            .iter()
            .map(|c| match c {
                Cell::Int(n) => *n,
                _ => panic!("unexpected cell type"),
            })
            .collect();
        values.sort_unstable();
        assert_eq!(values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_shuffle_prim_deterministic() {
        // With a fixed seed, SHUFFLE must produce the same permutation every time.
        use rand::SeedableRng;
        let mut vm = VM::new();
        vm.rng = rand::rngs::SmallRng::seed_from_u64(42);
        vm.arrays
            .push(vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]);
        vm.push(Cell::Array(0)).unwrap();
        shuffle_prim(&mut vm).unwrap();
        vm.pop().unwrap();
        let first_run: Vec<i64> = vm.arrays[0]
            .iter()
            .map(|c| match c {
                Cell::Int(n) => *n,
                _ => panic!("unexpected cell type"),
            })
            .collect();

        // Reset and run again with the same seed.
        let mut vm2 = VM::new();
        vm2.rng = rand::rngs::SmallRng::seed_from_u64(42);
        vm2.arrays
            .push(vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]);
        vm2.push(Cell::Array(0)).unwrap();
        shuffle_prim(&mut vm2).unwrap();
        vm2.pop().unwrap();
        let second_run: Vec<i64> = vm2.arrays[0]
            .iter()
            .map(|c| match c {
                Cell::Int(n) => *n,
                _ => panic!("unexpected cell type"),
            })
            .collect();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn test_shuffle_prim_empty_array() {
        // SHUFFLE on an empty array must not error.
        let mut vm = VM::new();
        vm.arrays.push(vec![]);
        vm.push(Cell::Array(0)).unwrap();
        shuffle_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Array(0)));
        assert_eq!(vm.arrays[0].len(), 0);
    }

    #[test]
    fn test_shuffle_prim_type_error() {
        // SHUFFLE on a non-Array cell must return TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(42)).unwrap();
        assert!(matches!(
            shuffle_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Array",
                ..
            })
        ));
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
}
