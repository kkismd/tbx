// build_support.rs — helper functions shared between build.rs and unit tests.
//
// This file is include!()-d by build.rs and by tests/build_sanitize_tests.rs
// so that the sanitization and validation logic can be unit-tested without
// duplicating the implementation.

/// Converts a file stem to a valid Rust function name by replacing `-` with `_`.
fn sanitize_fn_name(stem: &str) -> String {
    stem.replace('-', "_")
}

/// Validates that `fn_name` (the sanitized form of `stem`) consists only of
/// ASCII alphanumerics and `_`.  Panics with an informative message otherwise.
fn validate_stem(fn_name: &str, stem: &str) {
    if !fn_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        panic!(
            "build.rs: file stem `{stem}` contains characters that cannot form \
             a valid Rust identifier; rename the file to use only ASCII alphanumerics and '-'/'_'"
        );
    }
}
