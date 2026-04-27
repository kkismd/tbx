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

#[test]
fn test_lib_tbx_files() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let base_dir = PathBuf::from(manifest_dir);
    let tests_dir = base_dir.join("lib/tests");
    let mut test_files: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|e| panic!("cannot read lib/tests/: {e}"))
        .map(|e| e.unwrap_or_else(|e| panic!("cannot read dir entry in lib/tests/: {e}")))
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
        if let Err(e) = run_tbx_test(path, &base_dir) {
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
