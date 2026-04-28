// Integration tests for the build.rs sanitization and collision-detection helpers.
//
// build.rs is compiled as a standalone binary so its code cannot be accessed via
// the normal crate path.  Instead we include the shared helper file directly so
// that the same implementation is tested here.

// Include the shared helper functions from build_support.rs.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/build_support.rs"));

#[test]
fn sanitize_replaces_single_hyphen() {
    assert_eq!(sanitize_fn_name("test_foo-bar"), "test_foo_bar");
}

#[test]
fn sanitize_replaces_multiple_hyphens() {
    // e.g. test_a-b-c.tbx  →  test_a_b_c
    assert_eq!(sanitize_fn_name("test_a-b-c"), "test_a_b_c");
}

#[test]
fn sanitize_no_hyphen_is_unchanged() {
    assert_eq!(sanitize_fn_name("test_hello_world"), "test_hello_world");
}

#[test]
#[should_panic(expected = "cannot form a valid Rust identifier")]
fn validate_panics_on_space_in_stem() {
    // e.g. "test_foo bar.tbx" — space is not a valid identifier character.
    let fn_name = sanitize_fn_name("test_foo bar");
    validate_stem(&fn_name, "test_foo bar");
}

#[test]
#[should_panic(expected = "cannot form a valid Rust identifier")]
fn validate_panics_on_dot_in_stem() {
    let fn_name = sanitize_fn_name("test_foo.bar");
    validate_stem(&fn_name, "test_foo.bar");
}

#[test]
fn validate_accepts_valid_identifier() {
    // Should not panic.
    let fn_name = sanitize_fn_name("test_foo_bar");
    validate_stem(&fn_name, "test_foo_bar");
}

#[test]
fn collision_detected_for_hyphen_vs_underscore() {
    // test_foo_bar.tbx and test_foo-bar.tbx both produce fn_name "test_foo_bar".
    let mut seen = std::collections::HashSet::new();
    let first = sanitize_fn_name("test_foo_bar");
    let second = sanitize_fn_name("test_foo-bar");
    assert!(seen.insert(first), "first insert must succeed");
    assert!(
        !seen.insert(second),
        "second insert must fail due to collision"
    );
}

#[test]
fn no_collision_for_distinct_names() {
    let mut seen = std::collections::HashSet::new();
    let first = sanitize_fn_name("test_alpha");
    let second = sanitize_fn_name("test_beta");
    assert!(seen.insert(first));
    assert!(seen.insert(second));
}

#[test]
#[should_panic(expected = "cannot form a valid Rust identifier")]
fn validate_panics_on_digit_leading_stem() {
    // A stem starting with a digit produces an invalid Rust identifier.
    let fn_name = sanitize_fn_name("1test");
    validate_stem(&fn_name, "1test");
}

#[test]
#[should_panic(expected = "cannot form a valid Rust identifier")]
fn validate_panics_on_empty_stem() {
    // An empty stem cannot form any identifier.
    let fn_name = sanitize_fn_name("");
    validate_stem(&fn_name, "");
}
