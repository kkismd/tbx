// Integration test runner for TBX standard library tests.
//
// NOTE: USE paths in .tbx files are resolved relative to `base_dir`.
// `run_tbx_test` sets `base_dir` to `CARGO_MANIFEST_DIR` so that relative
// paths like `USE "lib/tests/helper.tbx"` work correctly regardless of the
// process CWD.
use std::path::{Path, PathBuf};
use tbx::interpreter::Interpreter;

fn run_tbx_test(path: &PathBuf, base_dir: &Path) -> Result<(), String> {
    let mut interp = Interpreter::new();
    interp
        .set_base_dir(base_dir.to_path_buf())
        .expect("CARGO_MANIFEST_DIR is always absolute");
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    interp
        .exec_source(&src)
        .map_err(|e| format!("{}: {e}", path.display()))
}

// Individual #[test] functions for each lib/tests/test_*.tbx file are generated
// by build.rs and included here.
include!(concat!(env!("OUT_DIR"), "/tbx_lib_tests_generated.rs"));

// ---------------------------------------------------------------------------
// Error-path integration tests: verify that invalid TBX programs are rejected
// at compile time rather than producing silent runtime failures.
// ---------------------------------------------------------------------------

/// A DEF…END block containing an unterminated string literal must fail at
/// compile time with `InvalidExpression`, not silently compile and then crash
/// at runtime with a `TypeError`.
#[test]
fn test_unterminated_string_in_def_is_compile_error() {
    use tbx::interpreter::Interpreter;
    let mut interp = Interpreter::new();
    // The closing `"` is intentionally omitted to produce Token::Error.
    let src = "DEF BAD_WORD\n  PUTSTR \"unterminated\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("expected a compile-time error for unterminated string literal");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

#[test]
fn test_sqrt_negative_float_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("SQRT -1.0\n")
        .expect_err("sqrt of negative should fail");
    assert!(err.to_string().contains("sqrt of negative"), "{err}");
}

#[test]
fn test_sqrt_negative_int_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("SQRT -1\n")
        .expect_err("sqrt of negative should fail");
    assert!(err.to_string().contains("sqrt of negative"), "{err}");
}

#[test]
fn test_hour_negative_timestamp_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("HOUR -1.0\n")
        .expect_err("HOUR with negative timestamp should fail");
    assert!(
        err.to_string().contains("non-negative"),
        "expected 'non-negative' in error message, got: {err}"
    );
}

#[test]
fn test_minute_negative_timestamp_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("MINUTE -1.0\n")
        .expect_err("MINUTE with negative timestamp should fail");
    assert!(
        err.to_string().contains("non-negative"),
        "expected 'non-negative' in error message, got: {err}"
    );
}

#[test]
fn test_second_negative_timestamp_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("SECOND -1.0\n")
        .expect_err("SECOND with negative timestamp should fail");
    assert!(
        err.to_string().contains("non-negative"),
        "expected 'non-negative' in error message, got: {err}"
    );
}

#[test]
fn test_array_index_zero_is_out_of_bounds() {
    use std::path::PathBuf;
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut interp = Interpreter::new();
    interp
        .set_base_dir(base)
        .expect("CARGO_MANIFEST_DIR is always absolute");
    // Array indices are 1-based; index 0 must return ArrayIndexOutOfBounds.
    let src = "DEF T()\n  VAR A\n  LET A = ARRAY(3)\n  RETURN A(0)\nEND\nT\n";
    let err = interp
        .exec_source(src)
        .expect_err("index 0 should be out of bounds");
    assert!(
        err.to_string().contains("array index out of bounds"),
        "expected 'array index out of bounds', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Array element string tests (issue #591, D-4: Rc<str> liberation)
// ---------------------------------------------------------------------------
//
// Since #591, `Cell::Str(Rc<str>)` is permitted as an array element for all
// array lifetimes (global, caller-owned, frame-local).  The `Rc` handle keeps
// the string alive independently of any stack frame, so no per-source-lifetime
// classification is needed.  Nested `Cell::Array` is still rejected.

/// SET &A(1), STR("hello") inside a word must succeed (#591).
/// STR() produces a runtime Rc<str>-backed string, which is now allowed as
/// an array element.  The word is called without parentheses (statement form)
/// because it has no return value.
#[test]
fn test_set_runtime_str_into_array_is_allowed() {
    let mut interp = Interpreter::new();
    // Note: void DEF is called without parentheses (statement form).
    let src = "DEF T()\n  VAR A\n  LET A = ARRAY(1)\n  SET &A(1), STR(\"hello\")\nEND\nT\n";
    interp
        .exec_source(src)
        .expect("storing runtime Str in array should succeed");
}

/// SET &A(1), "hello" (compile-time literal) must succeed (#591).
/// Array indices are 1-based in TBX.
#[test]
fn test_set_literal_str_into_array_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "VAR A\nSET &A, ARRAY(1)\nSET &A(1), \"hello\"\nPUTSTR A(1)\n";
    interp
        .exec_source(src)
        .expect("storing string literal in array should succeed");
    assert_eq!(interp.take_output(), "hello");
}

/// Same as above but inside a compiled word to verify frame-local arrays also
/// accept string literals.
#[test]
fn test_set_literal_str_into_array_inside_def_is_allowed() {
    let mut interp = Interpreter::new();
    let src =
        "DEF MAKE()\n  VAR A\n  SET &A, ARRAY(1)\n  SET &A(1), \"inside\"\n  PUTSTR A(1)\nEND\nMAKE\n";
    interp
        .exec_source(src)
        .expect("storing string literal in array inside DEF should succeed");
    assert_eq!(interp.take_output(), "inside");
}

/// STR_CONCAT result stored in global array, read back after word return (#591).
/// Exercises the runtime-string safety: the Rc handle outlives the call frame.
/// The void DEF is called without parentheses (statement form).
#[test]
fn test_set_runtime_str_into_global_array_survives_word_return() {
    let mut interp = Interpreter::new();
    // F is a void word; call it without parentheses to avoid DROP_TO_MARKER mismatch.
    let src = "VAR A\nSET &A, ARRAY(1)\nDEF F()\n  SET &A(1), STR_CONCAT(\"foo\", \"bar\")\nEND\nF\nPUTSTR A(1)\n";
    interp
        .exec_source(src)
        .expect("storing runtime Str in global array should succeed");
    assert_eq!(interp.take_output(), "foobar");
}

/// frame-local array can store and immediately read back a runtime string (#591).
/// The void DEF is called without parentheses (statement form).
#[test]
fn test_set_runtime_str_into_frame_local_array_is_allowed() {
    let mut interp = Interpreter::new();
    // F is a void word; call it without parentheses to avoid DROP_TO_MARKER mismatch.
    let src = "DEF F()\n  VAR A\n  SET &A, ARRAY(1)\n  SET &A(1), STR(\"hello\")\n  PUTSTR A(1)\nEND\nF\n";
    interp
        .exec_source(src)
        .expect("storing runtime Str in frame-local array should succeed");
    assert_eq!(interp.take_output(), "hello");
}

/// Caller-owned string parameter stored in a frame-local array must succeed (#591).
#[test]
fn test_set_caller_owned_str_param_into_frame_local_array_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "DEF USE(S)\n  VAR A\n  SET &A, ARRAY(1)\n  SET &A(1), S\n  PUTSTR A(1)\nEND\nUSE(\"arg\")\n";
    interp
        .exec_source(src)
        .expect("storing caller-owned Str param in frame-local array should succeed");
    assert_eq!(interp.take_output(), "arg");
}

/// TO_ARRAY(STR("a"), STR("b")) must now succeed (#591); the results can be
/// read back via array indexing.
#[test]
fn test_to_array_with_str_elements_is_allowed() {
    let mut interp = Interpreter::new();
    // Store TO_ARRAY result, read element 1 and 2 back via PUTSTR.
    let src = "VAR A\nSET &A, TO_ARRAY(STR(\"alpha\"), STR(\"beta\"))\nPUTSTR A(1)\nPUTSTR A(2)\n";
    interp
        .exec_source(src)
        .expect("TO_ARRAY with Str elements should succeed");
    assert_eq!(interp.take_output(), "alphabeta");
}

/// STR_LEN / STR_EQ / STR_CONCAT can operate on a Cell::Str read from an array element.
#[test]
fn test_str_ops_on_array_element_str() {
    let mut interp = Interpreter::new();
    // Use PUTSTR to exercise reading the element and passing it to string primitives.
    // STR_CONCAT output confirms the element was successfully read as Str.
    let src = "VAR A\nSET &A, ARRAY(1)\nSET &A(1), \"hello\"\nPUTSTR STR_CONCAT(A(1), \"!\")\n";
    interp
        .exec_source(src)
        .expect("string ops on array element should succeed");
    assert_eq!(interp.take_output(), "hello!");
}

/// Storing a nested array (Cell::Array) as an element must still fail.
#[test]
fn test_set_array_into_array_is_invalid_element_type() {
    let mut interp = Interpreter::new();
    // Create an outer array and a nested array, then try to store the inner in outer.
    let src =
        "DEF T()\n  VAR A, B\n  LET A = ARRAY(3)\n  LET B = ARRAY(2)\n  SET &A(1), B\nEND\nT\n";
    let err = interp
        .exec_source(src)
        .expect_err("storing Array in array element should fail");
    assert!(
        err.to_string().contains("invalid array element type"),
        "expected 'invalid array element type', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Duplicate local variable name tests (issue #634)
//
// Declaring the same local name twice in the same DEF is always an error,
// regardless of whether an initializer (`= expr`) is present.
// ---------------------------------------------------------------------------

/// `VAR A, A` — two identical names in the same VAR declaration must fail.
#[test]
fn test_duplicate_local_var_in_single_declaration_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF T()\n  VAR A, A\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("VAR A, A should fail with duplicate local variable error");
    assert!(
        err.to_string().contains("duplicate"),
        "expected 'duplicate' in error message, got: {err}"
    );
}

/// `VAR A` followed by a second `VAR A` (no initializer either time) must fail.
#[test]
fn test_duplicate_local_var_without_initializer_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF T()\n  VAR A\n  VAR A\nEND\n";
    let err = interp.exec_source(src).expect_err(
        "second VAR A (no initializer) should fail with duplicate local variable error",
    );
    assert!(
        err.to_string().contains("duplicate"),
        "expected 'duplicate' in error message, got: {err}"
    );
}

/// `VAR A` (no initializer) followed by `VAR A = 1` must fail.
#[test]
fn test_duplicate_local_var_no_init_then_with_init_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF T()\n  VAR A\n  VAR A = 1\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("VAR A then VAR A = 1 should fail with duplicate local variable error");
    assert!(
        err.to_string().contains("duplicate"),
        "expected 'duplicate' in error message, got: {err}"
    );
}

/// `VAR A = 1` followed by a plain `VAR A` (no initializer) must fail.
#[test]
fn test_duplicate_local_var_with_init_then_no_init_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF T()\n  VAR A = 1\n  VAR A\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("VAR A = 1 then VAR A should fail with duplicate local variable error");
    assert!(
        err.to_string().contains("duplicate"),
        "expected 'duplicate' in error message, got: {err}"
    );
}
