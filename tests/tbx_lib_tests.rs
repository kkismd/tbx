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
