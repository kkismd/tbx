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

// ---------------------------------------------------------------------------
// TUPLE primitive error-path tests (issue #655)
// ---------------------------------------------------------------------------

/// TUPLE(ARRAY(3)) must fail at runtime with an invalid-element-type error
/// because Cell::Array is a forbidden tuple element type.
#[test]
fn test_tuple_with_array_element_is_invalid() {
    let mut interp = Interpreter::new();
    let src = "DEF T()\n  VAR A\n  LET A = ARRAY(3)\n  RETURN TUPLE(A)\nEND\nT\n";
    let err = interp
        .exec_source(src)
        .expect_err("TUPLE(Array) should fail with invalid tuple element error");
    assert!(
        err.to_string().contains("tuple element type not allowed"),
        "expected 'tuple element type not allowed', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Regression tests for issue #657: [] token introduction
// ---------------------------------------------------------------------------

/// TUPLE(1, 2, 3) must still compile and produce the correct STR output.
/// Ensures that introducing LBracket/RBracket did not break tuple parsing.
#[test]
fn test_tuple_regression_issue_657() {
    let mut interp = Interpreter::new();
    // Use PUTSTR + take_output instead of ASSERT (which requires helper.tbx).
    let src = "DEF CHECK()\n  VAR T\n  LET T = TUPLE(1, 2, 3)\n  PUTSTR STR(T)\nEND\nCHECK\n";
    interp
        .exec_source(src)
        .expect("TUPLE(1, 2, 3) should still work after [] token introduction");
    assert_eq!(interp.take_output(), "(1, 2, 3)");
}

/// F(1) function call syntax must still work after [] token introduction.
#[test]
fn test_function_call_regression_issue_657() {
    let mut interp = Interpreter::new();
    let src = "DEF DOUBLE(X)\n  RETURN X * 2\nEND\nPUTDEC DOUBLE(3)\n";
    interp
        .exec_source(src)
        .expect("function call F(1) should still work after [] token introduction");
    assert_eq!(interp.take_output(), "6");
}

/// STR(TUPLE(1, 2)) must still produce the correct output.
#[test]
fn test_str_tuple_regression_issue_657() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  PUTSTR STR(TUPLE(1, 2))\nEND\nCHECK\n";
    interp
        .exec_source(src)
        .expect("STR(TUPLE(1, 2)) should still work after [] token introduction");
    assert_eq!(interp.take_output(), "(1, 2)");
}

// ---------------------------------------------------------------------------
// Tuple projection T[i] tests (issue #659)
// ---------------------------------------------------------------------------

/// T[i] basic projection: each element can be accessed by 1-based index.
#[test]
fn test_tuple_projection_basic() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  VAR T\n  LET T = TUPLE(2026, 5, 18)\n  PRINTLN T[1]\n  PRINTLN T[2]\n  PRINTLN T[3]\nEND\nCHECK\n";
    interp
        .exec_source(src)
        .expect("tuple projection T[i] should work");
    assert_eq!(interp.take_output(), "2026\n5\n18\n");
}

/// T[I] with a variable index must evaluate the variable at runtime.
#[test]
fn test_tuple_projection_with_variable_index() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  VAR I\n  VAR T\n  LET I = 2\n  LET T = TUPLE(10, 20, 30)\n  PRINTLN T[I]\nEND\nCHECK\n";
    interp
        .exec_source(src)
        .expect("tuple projection T[I] with variable index should work");
    assert_eq!(interp.take_output(), "20\n");
}

/// T[1 + 1] with an arithmetic expression as index must work.
#[test]
fn test_tuple_projection_with_expr_index() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  VAR T\n  LET T = TUPLE(10, 20, 30)\n  PRINTLN T[1 + 1]\nEND\nCHECK\n";
    interp
        .exec_source(src)
        .expect("tuple projection T[1+1] with expression index should work");
    assert_eq!(interp.take_output(), "20\n");
}

/// Mixed-type tuples: string, integer, and boolean elements.
#[test]
fn test_tuple_projection_mixed_types() {
    let mut interp = Interpreter::new();
    // Use `1 = 1` to produce a Bool(true) value, since TRUE is not a registered symbol.
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR T\n",
        "  LET T = TUPLE(\"tbx\", 1, 1 = 1)\n",
        "  PUTSTR T[1]\n",
        "  PUTSTR \" \"\n",
        "  PUTDEC T[2]\n",
        "  PUTSTR \" \"\n",
        "  PUTVAL T[3]\n",
        "END\n",
        "CHECK\n"
    );
    interp
        .exec_source(src)
        .expect("tuple projection on mixed-type tuple should work");
    assert_eq!(interp.take_output(), "tbx 1 TRUE");
}

/// T[0] and T[N+1] must produce an out-of-bounds error.
#[test]
fn test_tuple_projection_index_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  VAR T\n  LET T = TUPLE(1, 2, 3)\n  PRINTLN T[0]\nEND\nCHECK\n";
    interp
        .exec_source(src)
        .expect_err("T[0] should fail with out-of-bounds error");

    let mut interp2 = Interpreter::new();
    let src2 = "DEF CHECK()\n  VAR T\n  LET T = TUPLE(1, 2, 3)\n  PRINTLN T[4]\nEND\nCHECK\n";
    interp2
        .exec_source(src2)
        .expect_err("T[4] should fail with out-of-bounds error for a 3-element tuple");
}

/// T[1.5] with a non-integer index must produce a TypeError.
#[test]
fn test_tuple_projection_wrong_index_type() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  VAR T\n  LET T = TUPLE(1, 2, 3)\n  PRINTLN T[1.5]\nEND\nCHECK\n";
    let err = interp
        .exec_source(src)
        .expect_err("T[1.5] should fail with a type error");
    assert!(
        err.to_string().contains("type error"),
        "expected 'type error', got: {err}"
    );
}

/// X[1] on a non-tuple value must produce a TypeError.
#[test]
fn test_tuple_projection_non_tuple_target() {
    let mut interp = Interpreter::new();
    let src = "DEF CHECK()\n  VAR X\n  LET X = 42\n  PRINTLN X[1]\nEND\nCHECK\n";
    let err = interp
        .exec_source(src)
        .expect_err("X[1] on non-tuple should fail with a type error");
    assert!(
        err.to_string().contains("type error"),
        "expected 'type error', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// DIM @A[n] — array binding declaration (issue #663)
// ---------------------------------------------------------------------------

/// `DIM @A[n]` inside a DEF must succeed and not interfere with RETURN.
#[test]
fn test_dim_local_array_declaration_succeeds() {
    let mut interp = Interpreter::new();
    // F() declares a local array and returns 1; PUTDEC prints it.
    let src = "DEF F()\n  DIM @A[8]\n  RETURN 1\nEND\nPUTDEC F()\n";
    interp
        .exec_source(src)
        .expect("DIM @A[8] inside DEF should succeed");
    assert_eq!(interp.take_output(), "1");
}

/// `DIM @A[N]` with a variable-size expression inside DEF must succeed.
#[test]
fn test_dim_local_array_with_var_size_succeeds() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  VAR N = 8\n  DIM @A[N]\n  RETURN 1\nEND\nPUTDEC F()\n";
    interp
        .exec_source(src)
        .expect("DIM @A[N] with variable size inside DEF should succeed");
    assert_eq!(interp.take_output(), "1");
}

/// `DIM @A[4 + 4]` with an arithmetic expression inside DEF must succeed.
#[test]
fn test_dim_local_array_with_expr_size_succeeds() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A[4 + 4]\n  RETURN 1\nEND\nPUTDEC F()\n";
    interp
        .exec_source(src)
        .expect("DIM @A[4 + 4] inside DEF should succeed");
    assert_eq!(interp.take_output(), "1");
}

/// `DIM @G[4]` at the top level (global) must succeed.
#[test]
fn test_dim_global_array_declaration_succeeds() {
    let mut interp = Interpreter::new();
    let src = "DIM @G[4]\n";
    interp
        .exec_source(src)
        .expect("DIM @G[4] at top level should succeed");
}

/// Duplicate `DIM @A[n]` inside a DEF must produce an error.
#[test]
fn test_dim_duplicate_local_array_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A[8]\n  DIM @A[8]\n  RETURN 0\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("duplicate DIM @A inside DEF should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `VAR A` followed by `DIM @A[n]` inside a DEF must produce a name collision error.
#[test]
fn test_dim_collides_with_var_local_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  VAR A\n  DIM @A[8]\n  RETURN 0\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @A after VAR A inside DEF should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `DIM @A[n]` followed by `VAR A` inside a DEF must produce a name collision error.
#[test]
fn test_var_collides_with_dim_local_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A[8]\n  VAR A\n  RETURN 0\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("VAR A after DIM @A inside DEF should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `DIM @A` without `[n]` must produce a parse error.
#[test]
fn test_dim_missing_brackets_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A\n  RETURN 0\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @A without [n] should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `DIM @A[]` with an empty size must produce a parse error.
#[test]
fn test_dim_empty_brackets_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A[]\n  RETURN 0\nEND\n";
    let err = interp.exec_source(src).expect_err("DIM @A[] should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `DIM @[8]` without an identifier must produce a parse error.
#[test]
fn test_dim_missing_ident_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @[8]\n  RETURN 0\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @[8] without identifier should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `DIM @A(8)` using parentheses instead of brackets must produce a parse error.
#[test]
fn test_dim_parens_instead_of_brackets_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A(8)\n  RETURN 0\nEND\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @A(8) should fail — new syntax requires brackets");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `DIM @A[0]` with size zero must produce an error.
#[test]
fn test_dim_zero_size_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A[0]\n  RETURN 0\nEND\nF\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @A[0] should fail with invalid size");
    let msg = err.to_string();
    assert!(
        msg.contains("positive") || msg.contains("invalid"),
        "expected size error in message, got: {msg}"
    );
}

/// `DIM @A[-1]` with negative size must produce an error.
#[test]
fn test_dim_negative_size_is_error() {
    let mut interp = Interpreter::new();
    let src = "DEF F()\n  DIM @A[-1]\n  RETURN 0\nEND\nF\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @A[-1] should fail with invalid size");
    let msg = err.to_string();
    assert!(
        msg.contains("positive") || msg.contains("invalid"),
        "expected size error in message, got: {msg}"
    );
}

/// `DIM @G[4]` at global scope followed by a duplicate `DIM @G[4]` must error.
#[test]
fn test_dim_duplicate_global_array_is_error() {
    let mut interp = Interpreter::new();
    let src = "DIM @G[4]\nDIM @G[4]\n";
    let err = interp
        .exec_source(src)
        .expect_err("duplicate DIM @G at top level should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}

/// `VAR G` at global scope followed by `DIM @G[4]` must produce a name collision error.
#[test]
fn test_dim_collides_with_global_var_is_error() {
    let mut interp = Interpreter::new();
    let src = "VAR G\nDIM @G[4]\n";
    let err = interp
        .exec_source(src)
        .expect_err("DIM @G after global VAR G should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid expression"),
        "expected 'invalid expression' in error message, got: {msg}"
    );
}
