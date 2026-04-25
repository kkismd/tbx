// Integration test runner for TBX standard library tests.
//
// NOTE: USE paths in .tbx files are resolved relative to CWD.
// `cargo test` sets CWD to the package root, which satisfies this requirement.
use std::path::PathBuf;
use tbx::interpreter::Interpreter;

fn run_tbx_test(path: &PathBuf) -> Result<(), String> {
    let mut interp = Interpreter::new();
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    interp
        .exec_source(&src)
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[test]
fn test_lib_tbx_files() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let tests_dir = PathBuf::from(manifest_dir).join("lib/tests");
    let mut test_files: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|e| panic!("cannot read lib/tests/: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("test_") && n.ends_with(".tbx"))
                    .unwrap_or(false)
        })
        .collect();
    test_files.sort();
    assert!(
        !test_files.is_empty(),
        "no test_*.tbx files found in lib/tests/"
    );

    // Run all files and collect failures before reporting.
    let mut failures: Vec<String> = Vec::new();
    for path in &test_files {
        eprintln!("running tbx test: {}", path.display());
        if let Err(e) = run_tbx_test(path) {
            failures.push(e);
        }
    }
    if !failures.is_empty() {
        panic!(
            "{} TBX test(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
