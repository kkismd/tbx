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
    let src = "DEF T()\n  VAR A\n  LET A = ARRAY(3)\n  RETURN A(0)\nEND\nT()\n";
    let err = interp
        .exec_source(src)
        .expect_err("index 0 should be out of bounds");
    assert!(
        err.to_string().contains("array index out of bounds"),
        "expected 'array index out of bounds', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Array element type restriction tests (issue #487)
// ---------------------------------------------------------------------------

/// SET &A(i), "..." must now succeed because Cell::Str is Rc<str>-backed.
/// (Phase 5B: string lifetime is managed by reference counting.)
#[test]
fn test_set_runtime_str_into_array_is_allowed() {
    let mut interp = Interpreter::new();
    // String literals are now Rc<str>-backed handles that can be stored in any array.
    let src = "VAR A\nSET &A, ARRAY(3)\nSET &A(1), \"hello\"\n";
    interp
        .exec_source(src)
        .expect("storing Rc<str>-backed Cell::Str in array should succeed");
}

/// SET &A(1), "hello" (compile-time literal) must succeed.
/// String literals are stored into VM::strings as global compile-time literals
/// at compile time, so they satisfy the Global lifetime requirement.
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
/// accept global string literals.
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

/// TO_ARRAY(STR("a"), STR("b")) must now succeed because Cell::Str is Rc<str>-backed.
/// (Phase 5B: Str is no longer excluded from array elements.)
#[test]
fn test_to_array_with_str_elements_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "TO_ARRAY(STR(\"a\"), STR(\"b\"))\n";
    interp
        .exec_source(src)
        .expect("TO_ARRAY with Rc<str>-backed Str elements should succeed");
}

/// Storing a nested array (Cell::Array) as an element must fail.
#[test]
fn test_set_array_into_array_is_invalid_element_type() {
    let mut interp = Interpreter::new();
    // Create an outer array and a nested array, then try to store the inner in outer.
    let src =
        "DEF T()\n  VAR A, B\n  LET A = ARRAY(3)\n  LET B = ARRAY(2)\n  SET &A(1), B\nEND\nT()\n";
    let err = interp
        .exec_source(src)
        .expect_err("storing Array in array element should fail");
    assert!(
        err.to_string().contains("invalid array element type"),
        "expected 'invalid array element type', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Caller-owned string in array element (issue #567)
// ---------------------------------------------------------------------------

/// A caller-owned string parameter may be stored in a frame-local array element.
///
/// DEF USE(S)
///   VAR A
///   SET &A, ARRAY(1)
///   SET &A(1), S
///   PUTSTR A(1)
/// END
/// USE("arg")
///
/// S is caller-owned from the perspective of USE's call frame, and A is
/// frame-local, so this is safe and must succeed.
#[test]
fn test_set_caller_owned_str_param_into_frame_local_array_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "DEF USE(S)\n  VAR A\n  SET &A, ARRAY(1)\n  SET &A(1), S\n  PUTSTR A(1)\nEND\nUSE(\"arg\")\n";
    interp
        .exec_source(src)
        .expect("storing caller-owned Str param in frame-local array should succeed");
    assert_eq!(interp.take_output(), "arg");
}

/// A string literal can now be stored in a frame-local array.
/// Cell::Str is Rc<str>-backed; reference counting replaces pool-based lifetimes.
/// (Phase 5B)
#[test]
fn test_set_runtime_str_into_frame_local_array_is_allowed() {
    let mut interp = Interpreter::new();
    let src = "DEF T\n  VAR A\n  SET &A, ARRAY(1)\n  SET &A(1), \"hello\"\n  PUTSTR A(1)\nEND\nT\n";
    interp
        .exec_source(src)
        .expect("storing Rc<str>-backed Str in frame-local array should succeed");
    assert_eq!(interp.take_output(), "hello");
}

// ---------------------------------------------------------------------------
// Phase 5A deny-cases not directly expressible at TBX integration level
// ---------------------------------------------------------------------------
//
// The following Phase 5A combinations are denied by the runtime but cannot
// be exercised through integration tests due to current TBX limitations:
//
// 1. CallerOwned Str → Global Array:
//    String literals are always promoted to the global string pool at compile
//    time, so they are never CallerOwned.  A dynamically-generated (frame-local)
//    string returned from a word becomes CallerOwned in the caller, but there
//    is currently no way to pass that returned value as a parameter to another
//    word via a DEF-body statement call (DEF-inside-DEF statement calls trigger
//    a DROP_TO_MARKER mismatch due to a known limitation in the statement
//    compilation path).
//
// 2. CallerOwned Str → CallerOwned Array:
//    For the same reason as above, building a scenario where both the array
//    and the string are CallerOwned (i.e., created in distinct outer frames)
//    is not achievable through the integration-test harness in its current form.
//
// Both cases are covered at the unit-test level:
//   - `test_set_caller_owned_str_to_global_array_element_is_string_frame_escape`
//   - `test_set_caller_owned_str_to_caller_owned_array_element_is_string_frame_escape`
// in `src/primitives.rs`.
