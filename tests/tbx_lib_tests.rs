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
fn test_min_negative_timestamp_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("MIN -1.0\n")
        .expect_err("MIN with negative timestamp should fail");
    assert!(
        err.to_string().contains("non-negative"),
        "expected 'non-negative' in error message, got: {err}"
    );
}

#[test]
fn test_sec_negative_timestamp_is_error() {
    let mut interp = Interpreter::new();
    let err = interp
        .exec_source("SEC -1.0\n")
        .expect_err("SEC with negative timestamp should fail");
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
