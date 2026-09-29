use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return a `&str` view of a `Cell::Str` value.
///
/// Use when the caller only needs to read the string content and does not
/// need to retain an `Rc` handle after the call.
fn expect_str(cell: &Cell) -> Result<&str, TbxError> {
    cell.as_str()
        .map(|s| s.as_ref())
        .ok_or_else(|| TbxError::TypeError {
            expected: "Str",
            got: cell.type_name(),
        })
}

/// Return a cloned `Rc<str>` from a `Cell::Str` value.
///
/// Use when the caller may need to return the same `Rc` without allocating a
/// new buffer (e.g. the "no match" branch of `STR_REPLACE_FIRST`).
fn expect_str_rc(cell: &Cell) -> Result<Rc<str>, TbxError> {
    cell.as_str().cloned().ok_or_else(|| TbxError::TypeError {
        expected: "Str",
        got: cell.type_name(),
    })
}

// ---------------------------------------------------------------------------
// String primitives
// ---------------------------------------------------------------------------

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
    let s: Rc<str> = match &cell {
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
    let b = expect_str(&b_cell)?;
    let a = expect_str(&a_cell)?;
    // Concatenation always produces a fresh string; allocate a `String`
    // first to amortise the join, then convert into a new `Rc<str>` for
    // the resulting `Cell::Str`.
    let mut result = String::with_capacity(a.len() + b.len());
    result.push_str(a);
    result.push_str(b);
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
    let s = expect_str(&s_cell)?;
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
    let b = expect_str(&b_cell)?;
    let a = expect_str(&a_cell)?;
    // `&str` `==` compares string content directly.
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
    let needle = expect_str(&needle_cell)?;
    let haystack = expect_str(&haystack_cell)?;
    // `&str` `find` / slicing work directly without an intermediate `String`
    // allocation.
    let pos = haystack
        .find(needle)
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

    let s = expect_str(&s_cell)?;
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
    let s = expect_str(&s_cell)?;
    let trimmed = s.trim_matches(char::is_whitespace).to_string();
    vm.push(Cell::string(trimmed))?;
    Ok(())
}

/// STR_UPPER — convert a string to locale-independent Unicode uppercase.
///
/// Stack: `[..., s: Str]` → `Cell::Str(new)`
pub fn str_upper_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s_cell = vm.pop()?;
    let s = expect_str(&s_cell)?;
    let upper = s.to_uppercase();
    vm.push(Cell::string(upper))?;
    Ok(())
}

/// STR_LOWER — convert a string to locale-independent Unicode lowercase.
///
/// Stack: `[..., s: Str]` → `Cell::Str(new)`
pub fn str_lower_prim(vm: &mut VM) -> Result<(), TbxError> {
    let s_cell = vm.pop()?;
    let s = expect_str(&s_cell)?;
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
    let replacement = expect_str(&replacement_cell)?;
    let needle = expect_str(&needle_cell)?;
    // Use `expect_str_rc` for `s` so that we can return the same `Rc<str>`
    // cheaply when the needle is not found, without allocating a new buffer.
    let s_rc = expect_str_rc(&s_cell)?;

    if needle.is_empty() {
        return Err(TbxError::InvalidArgument {
            message: "STR_REPLACE_FIRST needle must not be empty".to_string(),
        });
    }

    // When no occurrence is found we can return the same `Rc<str>` by
    // cloning it cheaply (Rc reference-count bump) without allocating a
    // new buffer.
    if let Some(idx) = s_rc.find(needle) {
        let (prefix, rest) = s_rc.split_at(idx);
        let suffix = &rest[needle.len()..];
        let mut result = String::with_capacity(prefix.len() + replacement.len() + suffix.len());
        result.push_str(prefix);
        result.push_str(replacement);
        result.push_str(suffix);
        vm.push(Cell::string(result))?;
    } else {
        vm.push(Cell::Str(s_rc))?;
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
    let replacement = expect_str(&replacement_cell)?;
    let needle = expect_str(&needle_cell)?;
    let s = expect_str(&s_cell)?;

    if needle.is_empty() {
        return Err(TbxError::InvalidArgument {
            message: "STR_REPLACE_ALL needle must not be empty".to_string(),
        });
    }

    // `str::replace` always allocates a fresh `String`, even when the needle
    // is absent.  We keep that simple behaviour here.
    let result = s.replace(needle, replacement);
    vm.push(Cell::string(result))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        let original: Rc<str> = "shared".into();
        vm.push(Cell::Str(original.clone())).unwrap();
        str_prim(&mut vm).unwrap();
        match vm.pop().unwrap() {
            Cell::Str(rc) => {
                assert!(
                    Rc::ptr_eq(&rc, &original),
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
        let left: Rc<str> = "foo".into();
        let right: Rc<str> = "bar".into();
        vm.push(Cell::Str(left.clone())).unwrap();
        vm.push(Cell::Str(right.clone())).unwrap();
        str_concat_prim(&mut vm).unwrap();
        match vm.pop().unwrap() {
            Cell::Str(rc) => {
                assert_eq!(rc.as_ref(), "foobar");
                assert!(!Rc::ptr_eq(&rc, &left));
                assert!(!Rc::ptr_eq(&rc, &right));
            }
            other => panic!("expected Cell::Str, got {other:?}"),
        }
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
        let s: Rc<str> = "x".into();
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
}
