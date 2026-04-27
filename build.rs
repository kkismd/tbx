// build.rs — generates one #[test] function per lib/tests/test_*.tbx file.
//
// The generated source is written to $OUT_DIR/tbx_lib_tests_generated.rs and
// included by tests/tbx_lib_tests.rs via include!().

use std::env;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Trigger a rebuild whenever any file inside lib/tests/ changes.
    println!("cargo:rerun-if-changed=lib/tests/");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let tests_dir = PathBuf::from(&manifest_dir).join("lib/tests");

    // Collect test_*.tbx files in sorted order for deterministic output.
    let mut tbx_files: Vec<PathBuf> = fs::read_dir(&tests_dir)
        .unwrap_or_else(|e| panic!("cannot read lib/tests/: {e}"))
        .map(|e| {
            e.unwrap_or_else(|e| panic!("cannot read dir entry: {e}"))
                .path()
        })
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("test_") && n.ends_with(".tbx"))
                    .unwrap_or(false)
        })
        .collect();
    tbx_files.sort();

    let mut out = String::new();
    for path in &tbx_files {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("non-UTF-8 file name");

        // Strip the .tbx extension and replace '-' with '_' to form a valid Rust identifier.
        let stem = file_name
            .strip_suffix(".tbx")
            .expect("file_name ends with .tbx");
        let fn_name = stem.replace('-', "_");

        writeln!(
            out,
            r#"#[test]
fn {fn_name}() {{
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let base_dir = ::std::path::PathBuf::from(manifest_dir);
    let path = base_dir.join("lib/tests/{file_name}");
    if let Err(e) = run_tbx_test(&path, &base_dir) {{
        panic!("{{}}", e);
    }}
}}
"#
        )
        .expect("write to String never fails");
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = PathBuf::from(out_dir).join("tbx_lib_tests_generated.rs");
    fs::write(&dest, &out).unwrap_or_else(|e| panic!("cannot write {}: {e}", dest.display()));
}
