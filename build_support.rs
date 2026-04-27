// build_support.rs — helper functions shared between build.rs and unit tests.
//
// This file is include!()-d by build.rs and by tests/build_sanitize_tests.rs
// so that the sanitization and validation logic can be unit-tested without
// duplicating the implementation.

/// Converts a file stem to a valid Rust function name by replacing `-` with `_`.
fn sanitize_fn_name(stem: &str) -> String {
    stem.replace('-', "_")
}

/// Validates that `fn_name` (the sanitized form of `stem`) is a valid Rust identifier:
/// - non-empty
/// - first character is an ASCII letter or `_` (digits are not valid at the start)
/// - remaining characters are ASCII alphanumerics or `_`
///
/// Panics with an informative message if any condition is violated.
fn validate_stem(fn_name: &str, stem: &str) {
    let first_ok = fn_name
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic() || c == '_')
        .unwrap_or(false); // empty fn_name is invalid
    if !first_ok || !fn_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        panic!(
            "build.rs: file stem `{stem}` contains characters that cannot form \
             a valid Rust identifier; rename the file to use only ASCII alphanumerics and '-'/'_'"
        );
    }
}
