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

#[derive(Clone, Default)]
struct SharedOutput(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl SharedOutput {
    fn into_string(self) -> String {
        let bytes = self.0.lock().expect("shared output lock poisoned").clone();
        String::from_utf8(bytes).expect("shared output must be valid UTF-8")
    }
}

impl std::io::Write for SharedOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("shared output lock poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
    let src = "DEF T()\n  DIM @A[3]\n  RETURN @A[0]\nEND\nT\n";
    let err = interp
        .exec_source(src)
        .expect_err("index 0 should be out of bounds");
    assert!(
        err.to_string().contains("array index out of bounds"),
        "expected 'array index out of bounds', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Negative test: legacy A(i) array value access syntax must not work (#676)
// ---------------------------------------------------------------------------

/// `A(i)` must no longer be usable as an array value access expression.
///
/// The `A(i)` syntax for reading array elements has been removed in favour of
/// the `@A[i]` sigil syntax.  A global variable `A` followed by `(index)` no
/// longer compiles as an array element read; it is interpreted as a function
/// call on a variable, which fails at runtime with a type error.
#[test]
fn test_legacy_global_array_paren_syntax_is_not_array_access() {
    let mut interp = Interpreter::new();
    // Set up a global array and attempt to read element 2 using the old syntax.
    let src = "DIM @A[2]\nLET @A[1] = 10\nLET @A[2] = 20\nPUTDEC A(2)\n";
    let err = interp
        .exec_source(src)
        .expect_err("A(i) must not work as array value access");
    // The error occurs because the variable handle (DictAddr) ends up where a
    // number is expected — it is NOT silently returning 20.
    let msg = err.to_string();
    assert!(
        !msg.is_empty(),
        "A(i) must produce an error, not succeed silently"
    );
}

/// `A(i)` must no longer be usable as an array value access expression for
/// local variables either.
///
/// When `A` is a local variable holding an array, `A(i)` used to read element
/// `i`.  That path has been removed; `A(i)` is now parsed as a function call,
/// which fails at runtime with a type error.
#[test]
fn test_legacy_local_array_paren_syntax_is_not_array_access() {
    let mut interp = Interpreter::new();
    // Define a function that sets up a local array and attempts to read
    // element 1 using the old A(i) syntax.
    let src = "DEF F()\n  DIM @A[3]\n  LET @A[1] = 10\n  RETURN A(1)\nEND\nF()\n";
    let err = interp
        .exec_source(src)
        .expect_err("A(i) must not work as local array value access");
    let msg = err.to_string();
    assert!(
        !msg.is_empty(),
        "A(i) must produce an error for local variables, not succeed silently"
    );
}

// ---------------------------------------------------------------------------
// Negative test: legacy &A(i) array element address syntax must not work (#671)
// ---------------------------------------------------------------------------

/// `SET &A(i), value` for a global variable must be rejected.
///
/// The `&A(i)` syntax for writing array elements via SET has been removed in
/// favour of `&@A[i]`.  This test confirms that the legacy syntax now results
/// in a compile-time or runtime error.
#[test]
fn test_legacy_global_array_paren_addr_syntax_is_not_array_addr_access() {
    let mut interp = Interpreter::new();
    let src = "DIM @A[2]\nLET @A[1] = 10\nLET @A[2] = 20\nSET &A(1), 99\n";
    interp
        .exec_source(src)
        .expect_err("SET &A(i) for global variable must fail");
}

/// `SET &A(i), value` using a local variable must be rejected.
///
/// The `&A(i)` syntax for writing array elements via SET has been removed in
/// favour of `&@A[i]`.  This test confirms that the legacy syntax now results
/// in a compile-time or runtime error.
#[test]
fn test_legacy_local_array_paren_addr_syntax_is_not_array_addr_access() {
    let mut interp = Interpreter::new();
    // Define a function that sets up a local array and attempts to write
    // element 1 using the old `SET &A(i), value` syntax.
    let src = "DEF F()\n  DIM @A[3]\n  SET &A(1), 99\n  RETURN @A[1]\nEND\nF()\n";
    // Either compilation or runtime must fail — &A(i) no longer writes to
    // an array element.
    interp
        .exec_source(src)
        .expect_err("SET &A(i) for local variable must fail");
}

// ---------------------------------------------------------------------------
// Array element string tests (issue #591, D-4: Rc<str> liberation)
// ---------------------------------------------------------------------------
//
// Since #591, `Cell::Str(Rc<str>)` is permitted as an array element for all
// array lifetimes (global, caller-owned, frame-local).  The `Rc` handle keeps
// the string alive independently of any stack frame, so no per-source-lifetime
// classification is needed.  Nested `Cell::Array` is still rejected.

/// SET &@A[1], STR("hello") inside a word must succeed (#591).
/// STR() produces a runtime Rc<str>-backed string, which is now allowed as
/// an array element.  The word is called without parentheses (statement form)
/// because it has no return value.
#[test]
fn test_set_runtime_str_into_array_is_allowed() {
    let mut interp = Interpreter::new();
    // Note: void DEF is called without parentheses (statement form).
    let src = "DEF T()\n  DIM @A[1]\n  SET &@A[1], STR(\"hello\")\nEND\nT\n";
    interp
        .exec_source(src)
        .expect("storing runtime Str in array should succeed");
}

/// SET &@A[1], "hello" (compile-time literal) must succeed (#591).
/// Array indices are 1-based in TBX.
#[test]
fn test_set_literal_str_into_array_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "DIM @A[1]\nSET &@A[1], \"hello\"\nPUTSTR @A[1]\n";
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
    let src = "DEF MAKE()\n  DIM @A[1]\n  SET &@A[1], \"inside\"\n  PUTSTR @A[1]\nEND\nMAKE\n";
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
    let src =
        "DIM @A[1]\nDEF F()\n  SET &@A[1], STR_CONCAT(\"foo\", \"bar\")\nEND\nF\nPUTSTR @A[1]\n";
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
    let src = "DEF F()\n  DIM @A[1]\n  SET &@A[1], STR(\"hello\")\n  PUTSTR @A[1]\nEND\nF\n";
    interp
        .exec_source(src)
        .expect("storing runtime Str in frame-local array should succeed");
    assert_eq!(interp.take_output(), "hello");
}

/// Caller-owned string parameter stored in a frame-local array must succeed (#591).
#[test]
fn test_set_caller_owned_str_param_into_frame_local_array_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "DEF USE(S)\n  DIM @A[1]\n  SET &@A[1], S\n  PUTSTR @A[1]\nEND\nUSE(\"arg\")\n";
    interp
        .exec_source(src)
        .expect("storing caller-owned Str param in frame-local array should succeed");
    assert_eq!(interp.take_output(), "arg");
}

/// Str values can be stored in array elements and read back (#591).
#[test]
fn test_to_array_with_str_elements_is_allowed() {
    let mut interp = Interpreter::new();
    // Create an array and store string elements; read back via PUTSTR.
    let src = "DIM @A[2]\nSET &@A[1], STR(\"alpha\")\nSET &@A[2], STR(\"beta\")\nPUTSTR @A[1]\nPUTSTR @A[2]\n";
    interp
        .exec_source(src)
        .expect("storing Str elements in array should succeed");
    assert_eq!(interp.take_output(), "alphabeta");
}

/// STR_LEN / STR_EQ / STR_CONCAT can operate on a Cell::Str read from an array element.
#[test]
fn test_str_ops_on_array_element_str() {
    let mut interp = Interpreter::new();
    // Use PUTSTR to exercise reading the element and passing it to string primitives.
    // STR_CONCAT output confirms the element was successfully read as Str.
    let src = "DIM @A[1]\nSET &@A[1], \"hello\"\nPUTSTR STR_CONCAT(@A[1], \"!\")\n";
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
    let src = "DEF T()\n  DIM @A[3]\n  DIM @B[2]\n  SET &@A[1], B\nEND\nT\n";
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
    let src = "DEF T()\n  DIM @A[3]\n  RETURN TUPLE(A)\nEND\nT\n";
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
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR T\n",
        "  LET T = TUPLE(\"tbx\", 1, TRUE)\n",
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

/// `DIM @G[expr]` where the size expression evaluation fails at runtime
/// must not leave the VM's return stack, pc, or bp in a corrupted state.
/// After the error, a subsequent valid operation must succeed, proving
/// that the VM state was fully restored.
///
/// The `1 / 0` size expression is evaluated in a temporary code buffer via
/// `vm.run()`.  Before entering `vm.run()`, it pushes `ReturnFrame::TopLevel`
/// onto the return stack; a division-by-zero mid-run leaves that frame (and
/// any deeper frames that had been pushed) without ever popping them.
/// The fix snapshots and restores `return_stack`, `pc`, and `bp` on error.
#[test]
fn test_dim_global_size_expr_error_restores_vm_state() {
    let mut interp = Interpreter::new();

    // A division-by-zero inside the size expression causes vm.run() to abort
    // partway through, leaving ReturnFrame::TopLevel on the return stack if
    // the state is not properly restored.
    let err = interp
        .exec_source("DIM @G[1 / 0]\n")
        .expect_err("DIM @G[1/0] should fail with division by zero");
    assert!(
        err.to_string().contains("division by zero"),
        "expected 'division by zero' in error message, got: {err}"
    );

    // The VM must still be usable: a normal statement must succeed, showing
    // that return_stack / pc / bp were cleanly restored after the failure.
    interp
        .exec_source("PUTDEC 1\n")
        .expect("VM should be usable after DIM size-expression error");
    assert_eq!(
        interp.take_output(),
        "1",
        "PUTDEC 1 should produce '1' after recovery"
    );
}

// ---------------------------------------------------------------------------
// @A[i] — array binding element read (issue #665)
// ---------------------------------------------------------------------------

/// `@A[i]` on a local array binding declared with `DIM @A[n]` must return
/// the value previously written with `SET &@A[i], v`.
#[test]
fn test_at_array_local_index_read() {
    let mut interp = Interpreter::new();
    // DIM @A[3] creates a local array of 3 elements.
    // SET &@A[1], 10 writes 10 to element 1.
    // RETURN @A[1] reads it back via the new @A[i] syntax.
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A[1], 10\n",
        "  RETURN @A[1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("@A[1] should read back the value written by SET &@A[1], 10");
    assert_eq!(interp.take_output(), "10");
}

/// `@A[I + 1]` with an arithmetic expression as the index must evaluate the
/// expression at runtime and return the correct element.
#[test]
fn test_at_array_local_expr_index_read() {
    let mut interp = Interpreter::new();
    // DIM @A[3] + SET &@A[2], 20 + VAR I = 1 + RETURN @A[I + 1] == 20.
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A[2], 20\n",
        "  VAR I = 1\n",
        "  RETURN @A[I + 1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("@A[I + 1] should read element 2 written by SET &@A[2], 20");
    assert_eq!(interp.take_output(), "20");
}

/// `@G[i]` on a global array binding declared with `DIM @G[n]` at the top
/// level must return the value previously written with `SET &@G[i], v`.
#[test]
fn test_at_array_global_index_read() {
    let mut interp = Interpreter::new();
    // DIM @G[3] at top level + SET &@G[1], 30 + PUTDEC @G[1] == 30.
    let src = concat!("DIM @G[3]\n", "SET &@G[1], 30\n", "PUTDEC @G[1]\n",);
    interp
        .exec_source(src)
        .expect("@G[1] should read back the value written by SET &@G[1], 30");
    assert_eq!(interp.take_output(), "30");
}

// ---------------------------------------------------------------------------
// &@A[i] — array element address access (issue #667)
// ---------------------------------------------------------------------------

/// `SET &@A[i], v` on a local array binding must write the value and
/// `@A[i]` must read it back.
#[test]
fn test_set_at_array_local_element_address() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A[1], 10\n",
        "  RETURN @A[1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("SET &@A[1], 10 should write 10 to element 1 of local array");
    assert_eq!(interp.take_output(), "10");
}

/// `SET &@A[I + 1], v` with an arithmetic index expression must write to the
/// correct element.
#[test]
fn test_set_at_array_local_expr_index() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  VAR I = 1\n",
        "  SET &@A[I + 1], 20\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("SET &@A[I + 1], 20 should write 20 to element 2");
    assert_eq!(interp.take_output(), "20");
}

/// `SET &@G[i], v` on a global array binding declared at the top level must
/// write the value and `@G[i]` must read it back.
#[test]
fn test_set_at_array_global_element_address() {
    let mut interp = Interpreter::new();
    let src = concat!("DIM @G[3]\n", "SET &@G[1], 30\n", "PUTDEC @G[1]\n",);
    interp
        .exec_source(src)
        .expect("SET &@G[1], 30 should write 30 to element 1 of global array");
    assert_eq!(interp.take_output(), "30");
}

/// `SET &@A[2], 99` then `@A[2]` must return 99.
#[test]
fn test_set_at_array_second_element() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A[2], 99\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("SET &@A[2], 99 should write 99 to element 2");
    assert_eq!(interp.take_output(), "99");
}

/// `&@A` without brackets must produce a compile-time error.
#[test]
fn test_at_array_address_missing_bracket_is_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A, 10\n",
        "  RETURN 0\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("&@A without brackets should fail");
}

/// `&@` without an identifier must produce a compile-time error.
#[test]
fn test_at_array_address_missing_ident_is_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  SET &@[1], 10\n",
        "  RETURN 0\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("&@[1] without identifier should fail");
}

/// `&@A[i]` on an undefined binding must produce an UndefinedSymbol error.
#[test]
fn test_at_array_address_undefined_binding_is_error() {
    let mut interp = Interpreter::new();
    let src = "SET &@A[1], 10\n";
    interp
        .exec_source(src)
        .expect_err("&@A[1] on undefined binding should fail");
}

// ───────────────────────────────────────────────────────────────────────────
// LET @A[i] = expr — array element assignment sugar (issue #669)
// ───────────────────────────────────────────────────────────────────────────

/// `LET @A[i] = expr` on a local array binding must write the value and
/// `@A[i]` must read it back.
#[test]
fn test_let_at_array_local_element_assignment() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  LET @A[1] = 10\n",
        "  RETURN @A[1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[1] = 10 should write 10 to element 1 of local array");
    assert_eq!(interp.take_output(), "10");
}

/// `LET @A[I + 1] = expr` with an arithmetic index expression must write to
/// the correct element.
#[test]
fn test_let_at_array_index_expression() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  VAR I = 1\n",
        "  LET @A[I + 1] = 20\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[I + 1] = 20 should write 20 to element 2");
    assert_eq!(interp.take_output(), "20");
}

/// `LET @A[i] = <arithmetic expr>` must evaluate the RHS before storing.
#[test]
fn test_let_at_array_rhs_expression() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  LET @A[1] = 10 + 20\n",
        "  RETURN @A[1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[1] = 10 + 20 should store 30");
    assert_eq!(interp.take_output(), "30");
}

/// `LET @G[i] = expr` on a global array binding declared with `DIM @G[n]`
/// at the top level must write the value when called from inside a DEF body.
///
/// Note: `LET` uses `COMPILE_LVALUE` which requires compile mode; top-level
/// (execute-mode) `LET @G[i] = expr` is intentionally unsupported, matching
/// the same constraint as top-level `LET V = expr` for scalar variables.
#[test]
fn test_let_at_array_global_element_assignment() {
    let mut interp = Interpreter::new();
    // DIM @G at top level; LET @G[i] inside DEF to stay in compile mode.
    let src = concat!(
        "DIM @G[3]\n",
        "DEF F()\n",
        "  LET @G[1] = 30\n",
        "END\n",
        "F\n",
        "PUTDEC @G[1]\n",
    );
    interp
        .exec_source(src)
        .expect("LET @G[1] = 30 inside DEF should write 30 to element 1 of global array");
    assert_eq!(interp.take_output(), "30");
}

/// `LET @A[i] = expr` must produce the same result as `SET &@A[i], expr`.
#[test]
fn test_let_at_array_equivalent_to_set_address() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  LET @A[2] = 99\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[2] = 99 should be equivalent to SET &@A[2], 99");
    assert_eq!(interp.take_output(), "99");
}

/// `LET A = expr` scalar assignment must still work after adding array sugar.
#[test]
fn test_let_scalar_assignment_regression() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  VAR A\n",
        "  LET A = 42\n",
        "  RETURN A\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET A = 42 scalar assignment should still work");
    assert_eq!(interp.take_output(), "42");
}

/// `SET &@A[i], expr` must continue to work after introducing `LET @A[i]`.
#[test]
fn test_set_at_array_address_regression() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A[2], 99\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("SET &@A[2], 99 should still work");
    assert_eq!(interp.take_output(), "99");
}

/// `LET @A = expr` (missing brackets) must produce a compile-time error.
#[test]
fn test_let_at_array_missing_bracket_is_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  LET @A = 10\n",
        "  RETURN 0\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A = 10 without brackets should fail");
}

/// `LET @[1] = expr` (missing identifier after `@`) must produce a compile-time error.
#[test]
fn test_let_at_array_missing_ident_is_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  LET @[1] = 10\n",
        "  RETURN 0\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @[1] = 10 without identifier should fail");
}

/// `LET @A[] = expr` (empty index) must produce a compile-time error.
#[test]
fn test_let_at_array_empty_index_is_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  LET @A[] = 10\n",
        "  RETURN 0\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[] = 10 with empty index should fail");
}

/// `LET @A[1] = expr` on an undefined array binding must produce an error.
#[test]
fn test_let_at_array_undefined_binding_is_error() {
    let mut interp = Interpreter::new();
    let src = "LET @A[1] = 10\n";
    interp
        .exec_source(src)
        .expect_err("LET @A[1] on undefined binding should fail");
}

/// `LET T[i] = expr` (tuple projection assignment) must remain unsupported.
#[test]
fn test_let_tuple_projection_assignment_is_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  VAR T = TUPLE(1, 2, 3)\n",
        "  LET T[1] = 10\n",
        "  RETURN 0\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET T[1] = 10 tuple projection assignment should fail");
}

// ---------------------------------------------------------------------------
// Whole-array surface ban tests (issue #718)
//
// SET / STORE must never write a Cell::Array handle to a scalar variable slot.
// DIM @A[n] local initialisation is the only allowed path and uses the hidden
// ARRAY_STORE_LOCAL primitive internally.
// ---------------------------------------------------------------------------

/// `SET &B, A` inside a word (StackAddr destination) must fail with TypeError.
#[test]
fn test_set_array_handle_to_local_var_is_type_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF BAD_LOCAL()\n",
        "  DIM @A[2]\n",
        "  VAR B\n",
        "  SET &B, A\n",
        "END\n",
        "BAD_LOCAL\n",
    );
    let err = interp
        .exec_source(src)
        .expect_err("SET &B, A (StackAddr write of array handle) must fail");
    assert!(
        err.to_string().contains("type error"),
        "expected type error, got: {err}"
    );
}

/// `SET &G, A` at the top level (DictAddr destination) must fail with TypeError.
#[test]
fn test_set_array_handle_to_global_var_is_type_error() {
    let mut interp = Interpreter::new();
    let src = concat!("DIM @A[2]\n", "VAR G\n", "SET &G, A\n",);
    let err = interp
        .exec_source(src)
        .expect_err("SET &G, A (DictAddr write of array handle) must fail");
    assert!(
        err.to_string().contains("type error"),
        "expected type error, got: {err}"
    );
}

/// `LET B = A` inside a word must fail with TypeError (same array-handle ban).
#[test]
fn test_let_array_handle_to_local_var_is_type_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF BAD_LET()\n",
        "  DIM @A[2]\n",
        "  VAR B\n",
        "  LET B = A\n",
        "END\n",
        "BAD_LET\n",
    );
    let err = interp
        .exec_source(src)
        .expect_err("LET B = A (array handle assignment) must fail");
    assert!(
        err.to_string().contains("type error"),
        "expected type error, got: {err}"
    );
}

/// `ARRAY_GET(A, 1)` must not be callable from surface because ARRAY_GET is a
/// hidden system helper.
#[test]
fn test_array_get_is_hidden_from_surface() {
    let mut interp = Interpreter::new();
    let src = concat!("DIM @A[2]\n", "LET @A[1] = 10\n", "ARRAY_GET(A, 1)\n",);
    // ARRAY_GET is a hidden system helper; any compile/runtime error is acceptable.
    interp
        .exec_source(src)
        .expect_err("ARRAY_GET must not be callable from surface");
}

/// `ARRAY_ADDR(A, 1)` must not be callable from surface because ARRAY_ADDR is a
/// hidden system helper.
#[test]
fn test_array_addr_is_hidden_from_surface() {
    let mut interp = Interpreter::new();
    let src = concat!("DIM @A[2]\n", "ARRAY_ADDR(A, 1)\n",);
    let err = interp
        .exec_source(src)
        .expect_err("ARRAY_ADDR must not be callable from surface");
    assert!(
        err.to_string().contains("undefined symbol"),
        "expected undefined symbol, got: {err}"
    );
}

/// DIM @A[n] inside a DEF must still work correctly after the ban is in effect.
///
/// This regression test ensures that the hidden ARRAY_STORE_LOCAL path used by
/// the DIM compiler continues to initialise the local slot properly.
#[test]
fn test_dim_local_array_still_works_after_surface_ban() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  SET &@A[1], 10\n",
        "  SET &@A[2], 20\n",
        "  SET &@A[3], 30\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("DIM local array must still work after surface ban");
    assert_eq!(interp.take_output(), "20");
}

// ---------------------------------------------------------------------------
// &@A[x, y] — 2D array element address access (issue #748)
// ---------------------------------------------------------------------------

/// `SET &@A[x, y], v` on a global 2D array binding must write the value and
/// `@A[x, y]` must read it back via PUTDEC.
///
/// TBX code under test:
///   DIM @A[3, 2]
///   SET &@A[2, 1], 99
///   PUTDEC @A[2, 1]
///
/// Expected output: `99`
#[test]
fn test_set_at_array_2d_global_element_address() {
    let mut interp = Interpreter::new();
    let src = concat!("DIM @A[3, 2]\n", "SET &@A[2, 1], 99\n", "PUTDEC @A[2, 1]\n",);
    interp
        .exec_source(src)
        .expect("SET &@A[2, 1], 99 should write 99 to 2D element (2, 1) of global array");
    assert_eq!(interp.take_output(), "99");
}

/// `SET &@A[x, y], v` on a local 2D array binding inside a DEF must write the
/// value and `@A[x, y]` must read it back.
#[test]
fn test_set_at_array_2d_local_element_address() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[2, 1], 99\n",
        "  RETURN @A[2, 1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("SET &@A[2, 1], 99 should write 99 to 2D element (2, 1) of local array");
    assert_eq!(interp.take_output(), "99");
}

// LET @A[x, y] = expr — 2D array element assignment sugar (issue #749)
// ---------------------------------------------------------------------------

/// `LET @A[x, y] = expr` on a global 2D array binding must write the value
/// and `@A[x, y]` must read it back.
///
/// TBX code under test:
///   DIM @A[3, 2]          ← global (top-level)
///   DEF F()
///     LET @A[2, 1] = 99   ← LET inside DEF, writing to global array
///     PUTDEC @A[2, 1]
///   END
///   F
///
/// Expected output: `99`
#[test]
fn test_let_at_array_2d_global_element_assignment() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DIM @A[3, 2]\n",
        "DEF F()\n",
        "  LET @A[2, 1] = 99\n",
        "  PUTDEC @A[2, 1]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[2, 1] = 99 should write 99 to 2D element (2, 1) of global array");
    assert_eq!(interp.take_output(), "99");
}

/// `LET @A[x, y] = expr` on a local 2D array binding inside a DEF must write
/// the value and `@A[x, y]` must read it back.
#[test]
fn test_let_at_array_2d_local_element_assignment() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[2, 1] = 99\n",
        "  RETURN @A[2, 1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[2, 1] = 99 should write 99 to local 2D element (2, 1)");
    assert_eq!(interp.take_output(), "99");
}

/// `LET @A[x, y] = expr` must map to the same flat index as `SET &@A[x, y], expr`.
///
/// 3×2 array, fill all 6 elements via `LET @A[x, y]`, then read back with
/// `@A[x, y]` to verify the index formula `(y-1)*width + (x-1)`.
#[test]
fn test_let_at_array_2d_index_formula() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[1, 1] = 11\n",
        "  LET @A[2, 1] = 21\n",
        "  LET @A[3, 1] = 31\n",
        "  LET @A[1, 2] = 12\n",
        "  LET @A[2, 2] = 22\n",
        "  LET @A[3, 2] = 32\n",
        "  PUTDEC @A[1, 1]\n",
        "  PUTDEC @A[3, 1]\n",
        "  PUTDEC @A[1, 2]\n",
        "  PUTDEC @A[3, 2]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[x, y] should write to the correct flat index");
    // (1,1)→11, (3,1)→31, (1,2)→12, (3,2)→32
    assert_eq!(interp.take_output(), "11311232");
}

/// `LET @A[x, y] = expr` must produce the same element as `SET &@A[x, y], expr`.
#[test]
fn test_let_at_array_2d_equivalent_to_set_address() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[2, 1], 42\n",
        "  LET @A[2, 1] = 99\n",
        "  RETURN @A[2, 1]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[x, y] should overwrite the same element as SET &@A[x, y]");
    assert_eq!(interp.take_output(), "99");
}

// ---------------------------------------------------------------------------
// 2D array end-to-end: four-corner convention test (issue #751)
// ---------------------------------------------------------------------------
//
// These two tests implement the canonical example from issue #751.  They write
// to the four corners of a 3×2 array and read them back, verifying both the
// write path (LET vs. SET) and the [x, y] index convention.
//
// DIM @A[3, 2]:  width=3, height=2
//   A[1,1] → flat 0   A[3,1] → flat 2
//   A[1,2] → flat 3   A[3,2] → flat 5
//
// Detection value: accessing A[3, 1] (x=3) succeeds only when the first
// dimension is treated as width (3), not height (2).  If width and height
// were swapped, x=3 would exceed width=2 and trigger a bounds error.

/// Canonical 2D convention test via `LET @A[x, y]`.
///
/// Global `DIM @A[3, 2]`; writes the four corners inside a DEF (LET requires
/// compile mode), then reads them back.
///
/// Expected output: `10304060`
#[test]
fn test_2d_array_four_corners_via_let() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DIM @A[3, 2]\n",
        "DEF F()\n",
        "  LET @A[1, 1] = 10\n",
        "  LET @A[3, 1] = 30\n",
        "  LET @A[1, 2] = 40\n",
        "  LET @A[3, 2] = 60\n",
        "  PUTDEC @A[1, 1]\n",
        "  PUTDEC @A[3, 1]\n",
        "  PUTDEC @A[1, 2]\n",
        "  PUTDEC @A[3, 2]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[x, y] four-corner writes must succeed and read back correctly");
    assert_eq!(interp.take_output(), "10304060");
}

/// Canonical 2D convention test via `SET &@A[x, y], expr`.
///
/// Global `DIM @A[3, 2]`; writes the four corners at top level via SET
/// (no DEF needed for SET), then reads them back.
///
/// Expected output: `10304060`
#[test]
fn test_2d_array_four_corners_via_set() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DIM @A[3, 2]\n",
        "SET &@A[1, 1], 10\n",
        "SET &@A[3, 1], 30\n",
        "SET &@A[1, 2], 40\n",
        "SET &@A[3, 2], 60\n",
        "PUTDEC @A[1, 1]\n",
        "PUTDEC @A[3, 1]\n",
        "PUTDEC @A[1, 2]\n",
        "PUTDEC @A[3, 2]\n",
    );
    interp
        .exec_source(src)
        .expect("SET &@A[x, y] four-corner writes must succeed and read back correctly");
    assert_eq!(interp.take_output(), "10304060");
}

// ---------------------------------------------------------------------------

/// `LET @A[i] = expr` (1D syntax) must still work after introducing 2D support.
#[test]
fn test_let_at_array_1d_regression_after_2d() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[4]\n",
        "  LET @A[3] = 77\n",
        "  RETURN @A[3]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("LET @A[i] 1D assignment must still work after 2D changes");
    assert_eq!(interp.take_output(), "77");
}

/// `LET @A[x, y, z] = expr` (arity ≥ 3) must be rejected at compile time.
#[test]
fn test_let_at_array_2d_arity_3_is_compile_error() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[1, 1, 1] = 0\n",
        "END\n",
        "F\n",
    );
    assert!(
        interp.exec_source(src).is_err(),
        "LET @A[x, y, z] with arity >= 3 must be a compile error"
    );
}

// ---------------------------------------------------------------------------
// 2D array bounds and rank mismatch errors (issue #750)
// ---------------------------------------------------------------------------

// --- @A[x, y] bounds errors (ARRAY_GET_2D) ---

/// `@A[0, 1]` must be rejected: x=0 is invalid in 1-based indexing.
#[test]
fn test_2d_array_get_x_zero_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  RETURN @A[0, 1]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[0, 1] must fail: x=0 is out of bounds (1-based)");
}

/// `@A[1, 0]` must be rejected: y=0 is invalid in 1-based indexing.
#[test]
fn test_2d_array_get_y_zero_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  RETURN @A[1, 0]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[1, 0] must fail: y=0 is out of bounds (1-based)");
}

/// `@A[width + 1, 1]` must be rejected: x exceeds declared width.
#[test]
fn test_2d_array_get_x_exceeds_width_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    // DIM @A[3, 2] → width=3; @A[4, 1] is out of bounds.
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  RETURN @A[4, 1]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[4, 1] must fail: x exceeds width=3");
}

/// `@A[1, height + 1]` must be rejected: y exceeds declared height.
#[test]
fn test_2d_array_get_y_exceeds_height_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    // DIM @A[3, 2] → height=2; @A[1, 3] is out of bounds.
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  RETURN @A[1, 3]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[1, 3] must fail: y exceeds height=2");
}

// --- &@A[x, y] bounds errors (ARRAY_ADDR_2D) ---

/// `&@A[0, 1]` via `SET` must be rejected: x=0 is out of bounds.
#[test]
fn test_2d_array_addr_x_zero_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[0, 1], 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("SET &@A[0, 1], 10 must fail: x=0 is out of bounds (1-based)");
}

/// `&@A[1, 0]` via `SET` must be rejected: y=0 is out of bounds.
#[test]
fn test_2d_array_addr_y_zero_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[1, 0], 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("SET &@A[1, 0], 10 must fail: y=0 is out of bounds (1-based)");
}

/// `&@A[width + 1, 1]` via `SET` must be rejected: x exceeds declared width.
#[test]
fn test_2d_array_addr_x_exceeds_width_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[4, 1], 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("SET &@A[4, 1], 10 must fail: x exceeds width=3");
}

/// `&@A[1, height + 1]` via `SET` must be rejected: y exceeds declared height.
#[test]
fn test_2d_array_addr_y_exceeds_height_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[1, 3], 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("SET &@A[1, 3], 10 must fail: y exceeds height=2");
}

// --- LET @A[x, y] = expr bounds errors (same checks via ARRAY_ADDR_2D) ---

/// `LET @A[0, 1] = expr` must be rejected: x=0 is out of bounds.
#[test]
fn test_2d_let_x_zero_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[0, 1] = 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[0, 1] = 10 must fail: x=0 is out of bounds (1-based)");
}

/// `LET @A[1, 0] = expr` must be rejected: y=0 is out of bounds.
#[test]
fn test_2d_let_y_zero_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[1, 0] = 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[1, 0] = 10 must fail: y=0 is out of bounds (1-based)");
}

/// `LET @A[width + 1, 1] = expr` must be rejected: x exceeds declared width.
#[test]
fn test_2d_let_x_exceeds_width_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[4, 1] = 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[4, 1] = 10 must fail: x exceeds width=3");
}

/// `LET @A[1, height + 1] = expr` must be rejected: y exceeds declared height.
#[test]
fn test_2d_let_y_exceeds_height_is_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[1, 3] = 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[1, 3] = 10 must fail: y exceeds height=2");
}

// --- Rank mismatch: 1D array binding + 2D accessor ---

/// `@A[x, y]` on a 1D array binding must be rejected with a type error.
#[test]
fn test_2d_get_on_1d_array_is_rank_mismatch() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[6]\n",
        "  RETURN @A[1, 1]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[1, 1] on a 1D array must fail with rank mismatch");
}

/// `&@A[x, y]` on a 1D array binding must be rejected with a type error.
#[test]
fn test_2d_addr_on_1d_array_is_rank_mismatch() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[6]\n",
        "  SET &@A[1, 1], 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("&@A[1, 1] on a 1D array must fail with rank mismatch");
}

/// `LET @A[x, y] = expr` on a 1D array binding must be rejected with a type error.
#[test]
fn test_2d_let_on_1d_array_is_rank_mismatch() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[6]\n",
        "  LET @A[1, 1] = 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[1, 1] on a 1D array must fail with rank mismatch");
}

// --- Rank mismatch: 2D array binding + 1D accessor ---

/// `@A[i]` on a 2D array binding must be rejected with a rank mismatch error.
#[test]
fn test_1d_get_on_2d_array_is_rank_mismatch() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  RETURN @A[1]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[1] on a 2D array must fail with rank mismatch");
}

/// `&@A[i]` on a 2D array binding must be rejected with a rank mismatch error.
#[test]
fn test_1d_addr_on_2d_array_is_rank_mismatch() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  SET &@A[1], 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("&@A[1] on a 2D array must fail with rank mismatch");
}

/// `LET @A[i] = expr` on a 2D array binding must be rejected with a rank mismatch error.
#[test]
fn test_1d_let_on_2d_array_is_rank_mismatch() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3, 2]\n",
        "  LET @A[1] = 10\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("LET @A[1] on a 2D array must fail with rank mismatch");
}

// --- Regression: 1D array bounds behavior must be unchanged ---

/// 1D array: index 0 must still be rejected.
#[test]
fn test_1d_array_index_zero_is_still_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  RETURN @A[0]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[0] on a 1D array must still fail after 2D changes");
}

/// 1D array: index exceeding size must still be rejected.
#[test]
fn test_1d_array_index_exceeding_size_is_still_out_of_bounds() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  RETURN @A[4]\n",
        "END\n",
        "F\n",
    );
    interp
        .exec_source(src)
        .expect_err("@A[4] on a 3-element 1D array must still fail after 2D changes");
}

/// 1D array: valid access must still work.
#[test]
fn test_1d_array_valid_access_regression() {
    let mut interp = Interpreter::new();
    let src = concat!(
        "DEF F()\n",
        "  DIM @A[3]\n",
        "  LET @A[2] = 42\n",
        "  RETURN @A[2]\n",
        "END\n",
        "PUTDEC F()\n",
    );
    interp
        .exec_source(src)
        .expect("1D array valid access must still work after 2D changes");
    assert_eq!(interp.take_output(), "42");
}

// GETDEC? integration tests (issue #790)
//
// GETDEC? reads one line from stdin, parses it as a signed decimal integer,
// and returns TUPLE(n, TRUE) on success or TUPLE(0, FALSE) on failure.
// These tests inject mock input via vm_mut().input_reader to avoid real stdin.
// ---------------------------------------------------------------------------

/// Helper: run TBX source with a given mock input string, return output.
fn run_with_input(tbx_src: &str, mock_input: &str) -> String {
    use std::io::Cursor;

    let mut interp = Interpreter::new();
    interp.vm_mut().input_reader = Box::new(Cursor::new(mock_input.to_string()));
    interp
        .exec_source(tbx_src)
        .unwrap_or_else(|e| panic!("exec_source failed: {e}"));
    interp.take_output()
}

/// `123\n` → TUPLE(123, TRUE): valid integer input succeeds.
/// R[1] should be 123, R[2] should be TRUE.
#[test]
fn test_getdec_safe_valid_integer_tuple_projection() {
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR R\n",
        "  LET R = GETDEC?()\n",
        "  PUTDEC R[1]\n",
        "  PUTSTR \" \"\n",
        "  PUTVAL R[2]\n",
        "END\n",
        "CHECK\n",
    );
    let out = run_with_input(src, "123\n");
    assert_eq!(
        out, "123 TRUE",
        "valid integer should produce TUPLE(123, TRUE)"
    );
}

/// `abc\n` → TUPLE(0, FALSE): non-numeric input fails gracefully.
/// R[1] should be 0, R[2] should be FALSE.
#[test]
fn test_getdec_safe_non_numeric_tuple_projection() {
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR R\n",
        "  LET R = GETDEC?()\n",
        "  PUTDEC R[1]\n",
        "  PUTSTR \" \"\n",
        "  PUTVAL R[2]\n",
        "END\n",
        "CHECK\n",
    );
    let out = run_with_input(src, "abc\n");
    assert_eq!(
        out, "0 FALSE",
        "non-numeric input should produce TUPLE(0, FALSE)"
    );
}

/// `12abc\n` → TUPLE(0, FALSE): partial numeric input fails gracefully.
/// Even though the string starts with digits, it must not parse as a valid integer.
#[test]
fn test_getdec_safe_partial_numeric_tuple_projection() {
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR R\n",
        "  LET R = GETDEC?()\n",
        "  PUTDEC R[1]\n",
        "  PUTSTR \" \"\n",
        "  PUTVAL R[2]\n",
        "END\n",
        "CHECK\n",
    );
    let out = run_with_input(src, "12abc\n");
    assert_eq!(
        out, "0 FALSE",
        "partial numeric input should produce TUPLE(0, FALSE)"
    );
}

fn run_trek_src_with_input_and_flushed_output(tbx_src: &str, mock_input: &str) -> (String, String) {
    use std::io::Cursor;

    let mut interp = Interpreter::new();
    interp
        .set_base_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .expect("manifest dir is absolute");
    interp.vm_mut().input_reader = Box::new(Cursor::new(mock_input.to_string()));
    let flushed = SharedOutput::default();
    interp.vm_mut().output_writer = Box::new(flushed.clone());
    interp
        .exec_source(tbx_src)
        .unwrap_or_else(|e| panic!("exec_source failed: {e}"));
    (interp.take_output(), flushed.into_string())
}

#[test]
fn test_library_computer_eof_flushes_prompt_and_buffers_menu() {
    let src = concat!(
        "USE \"examples/trek/state.tbx\"\n",
        "USE \"examples/trek/util.tbx\"\n",
        "USE \"examples/trek/init.tbx\"\n",
        "USE \"examples/trek/scan.tbx\"\n",
        "USE \"examples/trek/combat.tbx\"\n",
        "USE \"examples/trek/nav.tbx\"\n",
        "USE \"examples/trek/library.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  LIBRARY_COMPUTER\n",
        "END\n",
        "RUN\n",
    );
    let (buffered, flushed) = run_trek_src_with_input_and_flushed_output(src, "");
    assert_eq!(flushed, "COMPUTER ACTIVE AND AWAITING COMMAND");
    assert_eq!(
        buffered,
        concat!(
            "FUNCTIONS AVAILABLE FROM COMPUTER\n",
            "   0 = CUMULATIVE GALACTIC RECORD\n",
            "   1 = STATUS REPORT\n",
            "   2 = PHOTON TORPEDO DATA\n",
        )
    );
}

#[test]
fn test_trek_command_loop_refreshes_docking_after_navigation_before_next_prompt() {
    let src = concat!(
        "USE \"examples/trek/state.tbx\"\n",
        "USE \"examples/trek/util.tbx\"\n",
        "USE \"examples/trek/init.tbx\"\n",
        "USE \"examples/trek/scan.tbx\"\n",
        "USE \"examples/trek/combat.tbx\"\n",
        "USE \"examples/trek/nav.tbx\"\n",
        "USE \"examples/trek/library.tbx\"\n",
        "USE \"examples/trek/command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  CLEAR_SECTOR\n",
        "  LET ENT_SX = 4\n",
        "  LET ENT_SY = 4\n",
        "  LET @SECTOR[ENT_SX, ENT_SY] = 1\n",
        "  LET @SECTOR[6, 4] = 3\n",
        "  LET DOCKED = FALSE\n",
        "  LET CONDITION = \"GREEN\"\n",
        "  LET ENERGY = MAX_ENERGY - 100\n",
        "  LET TORPEDOES = MAX_TORPEDOES - 2\n",
        "  LET SHIELDS = 250\n",
        "  LET KLINGONS_HERE = 0\n",
        "  LET START_STARDATE = 2000\n",
        "  LET STARDATE = 2000\n",
        "  LET MISSION_DAYS = 0\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  RUN_COMMAND_LOOP\n",
        "END\n",
        "RUN\n",
    );
    let (buffered, flushed) =
        run_trek_src_with_input_and_flushed_output(src, "0\n1\n0.2\n0\n1\n1.0\n");
    let combined = format!("{flushed}{buffered}");
    assert_eq!(
        combined
            .matches("SHIELDS DROPPED FOR DOCKING PURPOSES\n")
            .count(),
        1,
        "navigation should trigger exactly one docking refresh before the next prompt"
    );
}

#[test]
fn test_trek_command_loop_does_not_double_refresh_after_command_one_scan() {
    let src = concat!(
        "USE \"examples/trek/state.tbx\"\n",
        "USE \"examples/trek/util.tbx\"\n",
        "USE \"examples/trek/init.tbx\"\n",
        "USE \"examples/trek/scan.tbx\"\n",
        "USE \"examples/trek/combat.tbx\"\n",
        "USE \"examples/trek/nav.tbx\"\n",
        "USE \"examples/trek/library.tbx\"\n",
        "USE \"examples/trek/command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  CLEAR_SECTOR\n",
        "  LET ENT_SX = 4\n",
        "  LET ENT_SY = 4\n",
        "  LET @SECTOR[ENT_SX, ENT_SY] = 1\n",
        "  LET @SECTOR[5, 4] = 3\n",
        "  LET DOCKED = FALSE\n",
        "  LET CONDITION = \"GREEN\"\n",
        "  LET ENERGY = MAX_ENERGY - 100\n",
        "  LET TORPEDOES = MAX_TORPEDOES - 2\n",
        "  LET SHIELDS = 250\n",
        "  LET KLINGONS_HERE = 0\n",
        "  LET START_STARDATE = 2000\n",
        "  LET STARDATE = 2000\n",
        "  LET MISSION_DAYS = 0\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  RUN_COMMAND_LOOP\n",
        "END\n",
        "RUN\n",
    );
    let (buffered, flushed) = run_trek_src_with_input_and_flushed_output(src, "1\n0\n1\n1.0\n");
    let combined = format!("{flushed}{buffered}");
    assert_eq!(
        combined
            .matches("SHIELDS DROPPED FOR DOCKING PURPOSES\n")
            .count(),
        2,
        "initial scan and command 1 scan should dock once each without an extra post-command refresh"
    );
}

#[test]
fn test_trek_command_loop_does_not_refresh_after_non_navigation_command() {
    let src = concat!(
        "USE \"examples/trek/state.tbx\"\n",
        "USE \"examples/trek/util.tbx\"\n",
        "USE \"examples/trek/init.tbx\"\n",
        "USE \"examples/trek/scan.tbx\"\n",
        "USE \"examples/trek/combat.tbx\"\n",
        "USE \"examples/trek/nav.tbx\"\n",
        "USE \"examples/trek/library.tbx\"\n",
        "USE \"examples/trek/command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  CLEAR_SECTOR\n",
        "  LET ENT_SX = 4\n",
        "  LET ENT_SY = 4\n",
        "  LET @SECTOR[ENT_SX, ENT_SY] = 1\n",
        "  LET @SECTOR[5, 4] = 3\n",
        "  LET DOCKED = TRUE\n",
        "  LET CONDITION = \"DOCKED\"\n",
        "  LET SHIELDS = 200\n",
        "  LET KLINGONS_HERE = 0\n",
        "  LET START_STARDATE = 2000\n",
        "  LET STARDATE = 2000\n",
        "  LET MISSION_DAYS = 0\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  RUN_COMMAND_LOOP\n",
        "  PUTDEC SHIELDS\n",
        "END\n",
        "RUN\n",
    );
    let (buffered, flushed) =
        run_trek_src_with_input_and_flushed_output(src, "5\n123\n0\n1\n1.0\n");
    let combined = format!("{flushed}{buffered}");
    assert_eq!(
        combined
            .matches("SHIELDS DROPPED FOR DOCKING PURPOSES\n")
            .count(),
        1,
        "only the initial short-range scan should refresh docking for a non-navigation command cycle"
    );
    assert!(
        combined.ends_with("123"),
        "shield control value should survive until loop exit without post-command reset"
    );
}

#[test]
fn test_dispatch_command_7_eof_flushes_prompt_and_buffers_menu() {
    let src = concat!(
        "USE \"examples/trek/state.tbx\"\n",
        "USE \"examples/trek/util.tbx\"\n",
        "USE \"examples/trek/init.tbx\"\n",
        "USE \"examples/trek/scan.tbx\"\n",
        "USE \"examples/trek/combat.tbx\"\n",
        "USE \"examples/trek/nav.tbx\"\n",
        "USE \"examples/trek/library.tbx\"\n",
        "USE \"examples/trek/command.tbx\"\n",
        "DEF RUN()\n",
        "  INIT_GAME\n",
        "  VAR DISCARDED = GET_OUTPUT()\n",
        "  DISPATCH_COMMAND 7\n",
        "END\n",
        "RUN\n",
    );
    let (buffered, flushed) = run_trek_src_with_input_and_flushed_output(src, "");
    assert_eq!(flushed, "COMPUTER ACTIVE AND AWAITING COMMAND");
    assert_eq!(
        buffered,
        concat!(
            "FUNCTIONS AVAILABLE FROM COMPUTER\n",
            "   0 = CUMULATIVE GALACTIC RECORD\n",
            "   1 = STATUS REPORT\n",
            "   2 = PHOTON TORPEDO DATA\n",
        )
    );
}

/// Empty line `\n` → TUPLE(0, FALSE): empty input fails gracefully.
#[test]
fn test_getdec_safe_empty_line_tuple_projection() {
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR R\n",
        "  LET R = GETDEC?()\n",
        "  PUTDEC R[1]\n",
        "  PUTSTR \" \"\n",
        "  PUTVAL R[2]\n",
        "END\n",
        "CHECK\n",
    );
    let out = run_with_input(src, "\n");
    assert_eq!(out, "0 FALSE", "empty line should produce TUPLE(0, FALSE)");
}

/// `123\n` with `IF R[2] ... ELSE ... ENDIF` — success branch is taken.
/// Verifies the typical usage pattern: branch on ok flag, use value on success.
#[test]
fn test_getdec_safe_if_branch_pattern() {
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR R\n",
        "  LET R = GETDEC?()\n",
        "  IF R[2]\n",
        "    PUTSTR \"ok:\"\n",
        "    PUTDEC R[1]\n",
        "  ELSE\n",
        "    PUTSTR \"err\"\n",
        "  ENDIF\n",
        "END\n",
        "CHECK\n",
    );
    let out = run_with_input(src, "123\n");
    assert_eq!(out, "ok:123", "valid input should take the success branch");
}

/// `abc\n` with `IF R[2] ... ELSE ... ENDIF` — failure branch is taken.
/// Verifies the typical usage pattern: branch on ok flag, report error on failure.
#[test]
fn test_getdec_safe_if_branch_failure_pattern() {
    let src = concat!(
        "DEF CHECK()\n",
        "  VAR R\n",
        "  LET R = GETDEC?()\n",
        "  IF R[2]\n",
        "    PUTSTR \"ok:\"\n",
        "    PUTDEC R[1]\n",
        "  ELSE\n",
        "    PUTSTR \"err\"\n",
        "  ENDIF\n",
        "END\n",
        "CHECK\n",
    );
    let out = run_with_input(src, "abc\n");
    assert_eq!(
        out, "err",
        "non-numeric input should take the failure branch"
    );
}
