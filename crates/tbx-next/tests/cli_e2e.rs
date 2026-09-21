use std::fs;
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

fn example_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs")
        .join("next")
        .join("examples")
        .join(name)
}

fn run_with_file(path: &Path) -> Output {
    Command::new(tbx_next_bin())
        .arg(path)
        .current_dir(fixture_directory())
        .output()
        .expect("tbx-next binary should run")
}

fn run_with_file_and_stdin(path: &Path, input: &str) -> Output {
    let mut child = Command::new(tbx_next_bin())
        .arg(path)
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");

    child
        .stdin
        .as_mut()
        .expect("child stdin should be piped")
        .write_all(input.as_bytes())
        .expect("runtime input should be written to child");

    child
        .wait_with_output()
        .expect("tbx-next binary should finish")
}

fn run_with_args(args: &[&str]) -> Output {
    Command::new(tbx_next_bin())
        .args(args)
        .output()
        .expect("tbx-next binary should run")
}

fn run_with_args_and_stdin(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(tbx_next_bin())
        .args(args)
        .current_dir(fixture_directory())
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

fn run_with_stdin(source: &str) -> Output {
    let mut child = Command::new(tbx_next_bin())
        .current_dir(fixture_directory())
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

fn fixture_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
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
fn file_source_can_use_rnd_without_a_seed_option() {
    let output = run_with_file(&fixture_path("rnd.tbx"));

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    let value = stdout_text(&output)
        .trim()
        .parse::<i16>()
        .expect("RND output should be an integer");
    assert!((1..=10).contains(&value));
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn same_seed_reproduces_the_rnd_series_for_a_file_source() {
    let first = run_with_args(&["--seed", "42", fixture_path("rnd.tbx").to_str().unwrap()]);
    let second = run_with_args(&["--seed", "42", fixture_path("rnd.tbx").to_str().unwrap()]);

    assert!(first.status.success(), "{}", stderr_text(&first));
    assert!(second.status.success(), "{}", stderr_text(&second));
    assert_eq!(stdout_text(&first), stdout_text(&second));
    assert_eq!(stderr_text(&first), "");
    assert_eq!(stderr_text(&second), "");
}

#[test]
fn seed_option_reproduces_an_rnd_series_for_stdin_source() {
    let first = run_with_args_and_stdin(&["--seed", "42"], "PUTDEC RND(10)\n");
    let second = run_with_args_and_stdin(&["--seed", "42"], "PUTDEC RND(10)\n");

    assert!(first.status.success(), "{}", stderr_text(&first));
    assert!(second.status.success(), "{}", stderr_text(&second));
    assert_eq!(stdout_text(&first), stdout_text(&second));
}

#[test]
fn invalid_seed_fails_before_source_execution() {
    let output = run_with_args(&["--seed", "oops"]);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr_text(&output).contains("invalid arguments"));
    assert!(stderr_text(&output).contains("seed must be a decimal integer"));
}

#[test]
fn file_source_uses_process_stdin_for_runtime_input() {
    let output = run_with_file_and_stdin(
        &fixture_path("runtime_input_file.tbx"),
        "not a number\n42\n",
    );

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "0\n42\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_source_runtime_input_eof_is_a_recoverable_failure() {
    let output = run_with_file_and_stdin(&fixture_path("runtime_input_eof.tbx"), "");

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "0\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn stdin_success_uses_top_level_eval_through_real_binary() {
    let output = run_with_stdin("EVAL 6\nEVAL 7\nADD\nPUTDEC\nCR\n");

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "13\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn stdin_success_evaluates_expression_before_runtime_print_word() {
    let output = run_with_stdin("PUTDEC 2 + 3 * 4\nCR\n");

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "14\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn stdin_success_runs_multi_item_print_through_real_binary() {
    let output = run_with_stdin("LET A = 4\nLET B = 5\nPRINT \"TOTAL = \", A + B, \"!\"\n");

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "TOTAL = 9!");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_stdlib_control_structures_and_user_syntax_through_real_binary() {
    let path = fixture_path("m21_stdlib_control_e2e.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "2\n1\n2\n1\n3\n2\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_uses_prime_procedure_stack_argument_independent_of_global_variable() {
    let path = fixture_path("prime_stack_argument.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "0\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_prime_example_with_local_references() {
    let path = example_path("prime.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "2\n3\n5\n7\n11\n13\n17\n19\n23\n29\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_grades_example_with_for_and_select() {
    let path = example_path("grades.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(
        stdout_text(&output),
        "100 -> A\n95 -> A\n82 -> B\n76 -> C\n61 -> D\n58 -> F\nPassed: 5\n"
    );
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_global_array_squares_example_with_stable_output() {
    let path = example_path("squares.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(
        stdout_text(&output),
        "1 1\n2 4\n3 9\n4 16\n5 25\n6 36\n7 49\n8 64\n9 81\n10 100\n"
    );
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_nonrecursive_eight_queen_example() {
    let path = example_path("eightqueen.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "92\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_nonrecursive_maze_example_with_backtracking() {
    let path = example_path("maze.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(
        stdout_text(&output),
        "########\n#S***G##\n#+######\n#++#####\n########\nMAZE SOLVED\n"
    );
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_m28_sttr1_interaction_poc() {
    let output = run_with_file_and_stdin(
        &fixture_path("m28/sttr1_interaction_poc.tbx"),
        "invalid\n99\n1\n0\ninvalid\n95\n15\n2\n3\n4\n5\n6\n7\n9\n",
    );

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(
        stdout_text(&output),
        "STTR1 INTERACTION POC\n\
DATA STACK START: 0\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND INPUT ERROR\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
INVALID COMMAND\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
SHORT RANGE SCAN\n\
P.......\n\
.*......\n\
..K.....\n\
........\n\
........\n\
.....B..\n\
........\n\
........\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COURSE X10 (10=1.0, 15=1.5, ..., 90=9.0):\n\
COURSE INPUT ERROR\n\
COURSE X10 (10=1.0, 15=1.5, ..., 90=9.0):\n\
COURSE OUT OF RANGE\n\
COURSE X10 (10=1.0, 15=1.5, ..., 90=9.0):\n\
COURSE ACCEPTED X10 = 15\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND 2 REPORT\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND 3 REPORT\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND 4 REPORT\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND 5 REPORT\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND 6 REPORT\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
COMMAND 7 REPORT\n\
COMMAND (0 NAV, 1 SCAN, 2-7 REPORTS, 9 QUIT):\n\
POC COMPLETE\n\
DATA STACK END: 0\n"
    );
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_guess_example_with_runtime_input() {
    let path = example_path("guess.tbx");
    let input = (1..=100)
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let output = run_with_file_and_stdin(&path, &format!("invalid\n{input}\n"));
    let stdout = stdout_text(&output);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert!(
        stdout.contains("Guess a number from 1 to 100: "),
        "{stdout}"
    );
    assert!(stdout.contains("Please enter a number.\n"), "{stdout}");
    assert!(stdout.ends_with("Correct!\n"), "{stdout}");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_integer_mandelbrot_example_with_stable_output() {
    let path = example_path("mandelbrot.tbx");
    let expected = fs::read_to_string(fixture_path("mandelbrot_expected.txt"))
        .expect("Mandelbrot output fixture should be readable");

    let output = run_with_file(&path);
    let stdout = stdout_text(&output);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stderr_text(&output), "");
    assert_eq!(stdout, expected);

    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 25);
    assert!(
        lines.iter().all(|line| line.chars().count() == 79),
        "all lines should contain 79 characters"
    );
}

#[test]
fn stdin_success_runs_stdlib_control_structures_and_user_syntax_through_real_binary() {
    let source = include_str!("fixtures/m21_stdlib_control_e2e.tbx");

    let output = run_with_stdin(source);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "2\n1\n2\n1\n3\n2\n");
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

#[test]
fn file_use_loads_nested_sources_and_publishes_words_in_order() {
    let path = fixture_path("m22/normal/main.tbx");
    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "1\n2\n3\n30\n4\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_use_completed_duplicate_is_a_noop() {
    let path = fixture_path("m22/duplicate/main.tbx");
    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "2\n20\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_use_cycle_fails_with_requesting_source_location() {
    let path = fixture_path("m22/cycle/main.tbx");
    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("B.tbx:1:1:"), "{stderr}");
    assert!(stderr.contains("main.tbx"), "{stderr}");
}

#[test]
fn file_use_acquisition_failure_keeps_request_location_and_specification() {
    let path = fixture_path("m22/acquisition-failure/main.tbx");
    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(
        stderr.contains("m22/acquisition-failure/main.tbx:1:1:"),
        "{stderr}"
    );
    assert!(stderr.contains("missing/dependency.tbx"), "{stderr}");
    assert!(
        stderr.contains("additional source acquisition failed"),
        "{stderr}"
    );
}

#[test]
fn file_use_compile_failure_reports_additional_source_display_name_and_location() {
    let path = fixture_path("m22/compile-failure/main.tbx");
    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("bad.tbx:1:7:"), "{stderr}");
    assert!(stderr.contains("1 | LET A ="), "{stderr}");
    assert!(stderr.contains("source word error"), "{stderr}");
}

#[test]
fn file_use_runtime_failure_maps_to_additional_source() {
    let path = fixture_path("m22/runtime-failure/main.tbx");
    let output = run_with_file(&path);
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("bad.tbx:2:8:"), "{stderr}");
    assert!(stderr.contains("2 | EVAL 1 / 0"), "{stderr}");
    assert!(stderr.contains("runtime error"), "{stderr}");
}

#[test]
fn file_use_shares_global_arrays_through_nested_sources_in_both_directions() {
    let output = run_with_file(&fixture_path("m24/use-arrays/main.tbx"));

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "35\n22\n");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_use_array_name_conflict_uses_the_shared_namespace() {
    let output = run_with_file(&fixture_path("m24/use-array-conflict/main.tbx"));
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("library.tbx:1:"), "{stderr}");
    assert!(stderr.contains("source word error"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn file_use_array_out_of_bounds_is_a_located_runtime_failure() {
    let output = run_with_file(&fixture_path("m24/use-array-runtime-failure/main.tbx"));
    let stderr = stderr_text(&output);

    assert!(!output.status.success());
    assert_eq!(stdout_text(&output), "");
    assert!(stderr.contains("main.tbx:2:"), "{stderr}");
    assert!(stderr.contains("runtime error"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn file_success_runs_the_m28_sttr1_numeric_poc() {
    let output = run_with_file(&fixture_path("m28/sttr1_numeric_poc.tbx"));

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(
        stdout_text(&output),
        "DEPTH START 0\nDEPTH INDEX 0\n1\n8\n57\n64\n37\n55\n10 0\n7 7\n0 10\n99 99\n8 3\n0\nDEPTH ISQRT 0\n10\n14\n98\n1\nDEPTH PERCENT 0\n1\n0\n0\nDEPTH FACTOR 0\n99\n199\nRND OK\n0\nDEPTH DAMAGE 0\n500\n16\n29850\n5970\nDEPTH END 0\n"
    );
    assert_eq!(stderr_text(&output), "");
}
