use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn tbx_next_bin() -> &'static str {
    env!("CARGO_BIN_EXE_tbx-next")
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_with_file(path: &Path) -> Output {
    Command::new(tbx_next_bin())
        .arg(path)
        .output()
        .expect("tbx-next binary should run")
}

fn run_with_args(args: &[&str]) -> Output {
    Command::new(tbx_next_bin())
        .args(args)
        .output()
        .expect("tbx-next binary should run")
}

fn run_with_stdin(source: &str) -> Output {
    let mut child = Command::new(tbx_next_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");

    child
        .stdin
        .as_mut()
        .expect("child stdin should be piped")
        .write_all(source.as_bytes())
        .expect("source should be written to child stdin");

    child
        .wait_with_output()
        .expect("tbx-next binary should finish")
}

fn stdout_text(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("stdout should be UTF-8")
}

fn stderr_text(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("stderr should be UTF-8")
}

#[test]
fn file_success_runs_m20_paths_through_real_binary() {
    let path = fixture_path("m20_success.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "3\n1\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn stdin_success_uses_top_level_eval_through_real_binary() {
    let output = run_with_stdin("EVAL 6\nEVAL 7\nADD\nPRINT\nCR\n");

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "13\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_source_failure_reports_file_location_on_stderr_only() {
    let path = fixture_path("compile_failure.tbx");

    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains(path.to_string_lossy().as_ref()), "{stderr}");
    assert!(stderr.contains(":1:7:"), "{stderr}");
    assert!(stderr.contains("source word error"), "{stderr}");
    assert!(stderr.contains("1 | LET A ="), "{stderr}");
}

#[test]
fn stdin_source_failure_reports_stdin_location_on_stderr_only() {
    let output = run_with_stdin("UNKNOWN\n");
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("<stdin>:1:1:"), "{stderr}");
    assert!(stderr.contains("compile error"), "{stderr}");
    assert!(stderr.contains("1 | UNKNOWN"), "{stderr}");
}

#[test]
fn runtime_failure_in_compiled_word_reports_definition_location() {
    let path = fixture_path("runtime_failure_in_definition.tbx");

    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains(path.to_string_lossy().as_ref()), "{stderr}");
    assert!(stderr.contains(":2:8:"), "{stderr}");
    assert!(stderr.contains("runtime error"), "{stderr}");
    assert!(stderr.contains("2 | EVAL 1 / 0"), "{stderr}");
    assert!(!stderr.contains(":4:1:"), "{stderr}");
}

#[test]
fn two_or_more_args_fail_before_source_acquisition_without_fake_location() {
    let output = run_with_args(&["first.tbx", "second.tbx"]);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("invalid arguments"), "{stderr}");
    assert!(
        stderr.contains("expected at most one source file"),
        "{stderr}"
    );
    assert!(!stderr.contains(":1:1"), "{stderr}");
    assert!(!stderr.contains("<stdin>"), "{stderr}");
}

#[test]
fn missing_file_failure_mentions_requested_path_without_fake_location() {
    let path = fixture_path("missing-file-does-not-exist.tbx");

    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains(path.to_string_lossy().as_ref()), "{stderr}");
    assert!(stderr.contains("failed to read"), "{stderr}");
    assert!(!stderr.contains(":1:1"), "{stderr}");
}
