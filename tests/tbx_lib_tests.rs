// Integration test runner for TBX standard library tests.
use std::path::PathBuf;
use tbx::interpreter::Interpreter;

fn run_tbx_test(path: &PathBuf) {
    let mut interp = Interpreter::new();
    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    interp.exec_source(&src).unwrap_or_else(|e| {
        panic!("{}: {e}", path.display());
    });
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
            p.file_name()
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
    for path in &test_files {
        eprintln!("running tbx test: {}", path.display());
        run_tbx_test(path);
    }
}
