use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const OUTPUT_LIMIT: usize = 256 * 1024;
const OUTPUT_TAIL: usize = 4096;

enum ProcessEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    StdoutDone,
    StderrDone,
}

#[derive(Debug)]
struct CapturedProcess {
    output: Output,
}

#[derive(Default)]
struct ProcessOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

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
    let child = Command::new(tbx_next_bin())
        .arg(path)
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");

    run_child(
        child,
        input,
        None,
        "file runtime input",
        input.lines().count(),
    )
    .output
}

fn run_with_args(args: &[&str]) -> Output {
    Command::new(tbx_next_bin())
        .args(args)
        .output()
        .expect("tbx-next binary should run")
}

fn run_with_args_and_stdin(args: &[&str], source: &str) -> Output {
    let child = Command::new(tbx_next_bin())
        .args(args)
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");

    run_child(child, source, None, "stdin source", source.lines().count()).output
}

fn run_with_stdin(source: &str) -> Output {
    let child = Command::new(tbx_next_bin())
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");

    run_child(child, source, None, "stdin source", source.lines().count()).output
}

fn run_until_stdout_marker(path: &Path, input: &str, marker: &str) -> Output {
    run_until_nth_stdout_marker_with_args(&[], path, input, marker, 1)
}

fn run_until_nth_stdout_marker_with_args(
    args: &[&str],
    path: &Path,
    input: &str,
    marker: &str,
    occurrence: usize,
) -> Output {
    assert!(!marker.is_empty(), "stdout marker must not be empty");
    assert!(occurrence > 0, "stdout marker occurrence must be positive");
    let child = Command::new(tbx_next_bin())
        .args(args)
        .arg(path)
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");

    let captured = run_child(
        child,
        input,
        Some((marker.as_bytes(), occurrence)),
        &format!("stdout marker {marker:?} occurrence {occurrence}"),
        input.lines().count(),
    );
    captured.output
}

fn run_child(
    child: Child,
    input: &str,
    marker: Option<(&[u8], usize)>,
    expectation: &str,
    input_lines: usize,
) -> CapturedProcess {
    run_child_with_timeout(
        child,
        input,
        marker,
        expectation,
        input_lines,
        PROCESS_TIMEOUT,
    )
}

fn run_child_with_timeout(
    mut child: Child,
    input: &str,
    marker: Option<(&[u8], usize)>,
    expectation: &str,
    input_lines: usize,
    timeout: Duration,
) -> CapturedProcess {
    let (tx, rx) = mpsc::sync_channel(16);
    let stdout = child.stdout.take().expect("child stdout should be piped");
    let stderr = child.stderr.take().expect("child stderr should be piped");
    let stdout_tx = tx.clone();
    let stdout_thread = thread::spawn(move || drain(stdout, stdout_tx, true));
    let stderr_thread = thread::spawn(move || drain(stderr, tx, false));
    let mut stdin = child.stdin.take().expect("child stdin should be piped");
    let input = input.as_bytes().to_vec();
    let input_thread = thread::spawn(move || {
        let result = stdin.write_all(&input);
        drop(stdin);
        result
    });

    let deadline = Instant::now() + timeout;
    let mut exit_status = None;
    let mut output = ProcessOutput::default();
    let mut already_exited = false;
    let mut marker_reached = false;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut overflow = false;
    let mut stop_reason = None;

    loop {
        receive_events(
            &rx,
            &mut output,
            &mut stdout_done,
            &mut stderr_done,
            &mut overflow,
        );
        if let Some((expected, occurrence)) = marker {
            if output
                .stdout
                .windows(expected.len())
                .filter(|window| *window == expected)
                .take(occurrence)
                .count()
                == occurrence
            {
                marker_reached = true;
                stop_reason = Some("marker reached".to_owned());
                break;
            }
        }
        if overflow {
            stop_reason = Some(format!("output exceeded {OUTPUT_LIMIT} bytes per stream"));
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = Some(status);
                already_exited = true;
                if marker.is_none() || (stdout_done && stderr_done) {
                    break;
                }
            }
            Ok(None) => {}
            Err(error) => {
                stop_reason = Some(format!("failed checking child status: {error}"));
                break;
            }
        }
        if Instant::now() >= deadline {
            stop_reason = Some(format!("timed out after {timeout:?}"));
            break;
        }
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(event) => apply_event(
                event,
                &mut output,
                &mut stdout_done,
                &mut stderr_done,
                &mut overflow,
            ),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // Reader completion only means both pipes have closed. The child may
            // still be running, so keep checking its status until exit or deadline.
            // Avoid repeatedly polling a disconnected channel without a pause.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    if marker_reached && !already_exited {
        if let Ok(Some(status)) = child.try_wait() {
            exit_status = Some(status);
            already_exited = true;
        }
    }
    if stop_reason.is_some() && !already_exited {
        let _ = child.kill();
    }
    if !already_exited {
        exit_status = Some(
            child
                .wait()
                .expect("child should be reaped after monitoring"),
        );
    }
    while !stdout_done || !stderr_done {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => apply_event(
                event,
                &mut output,
                &mut stdout_done,
                &mut stderr_done,
                &mut overflow,
            ),
            Err(_) => break,
        }
    }
    let input_write_result = match input_thread.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("stdin writer thread panicked".to_owned()),
    };
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    let output = Output {
        status: exit_status.expect("child exit status should be collected"),
        stdout: output.stdout,
        stderr: output.stderr,
    };

    // Interpret stdin delivery only after the child stop reason is known. A marker-triggered
    // stop and a natural exit without the marker can both close stdin before writing completes.
    if let Err(error) = &input_write_result {
        let natural_exit_without_marker =
            marker.is_some() && !marker_reached && stop_reason.is_none() && exit_status.is_some();
        if !marker_reached && !natural_exit_without_marker && stop_reason.is_none() {
            panic!(
                "stdin write failed: {error}\n{}\nstdout tail:\n{}\nstderr tail:\n{}",
                diagnostic(expectation, input_lines, &input_write_result, &output),
                tail(&output.stdout),
                tail(&output.stderr)
            );
        }
    }

    if let Some(reason) = stop_reason {
        if !marker_reached {
            panic!(
                "{}\nstdout tail:\n{}\nstderr tail:\n{}",
                diagnostic(
                    &format!("{expectation}: {reason}"),
                    input_lines,
                    &input_write_result,
                    &output
                ),
                tail(&output.stdout),
                tail(&output.stderr)
            );
        }
    } else if marker.is_some() && !marker_reached {
        panic!(
            "{}\nstdout tail:\n{}\nstderr tail:\n{}",
            diagnostic(
                &format!("{expectation} not reached before process exit"),
                input_lines,
                &input_write_result,
                &output
            ),
            tail(&output.stdout),
            tail(&output.stderr)
        );
    }
    CapturedProcess { output }
}

fn drain<R: Read>(mut reader: R, tx: SyncSender<ProcessEvent>, stdout: bool) {
    let mut buffer = [0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(size) => {
                let event = if stdout {
                    ProcessEvent::Stdout(buffer[..size].to_vec())
                } else {
                    ProcessEvent::Stderr(buffer[..size].to_vec())
                };
                if tx.send(event).is_err() {
                    return;
                }
            }
        }
    }
    let _ = tx.send(if stdout {
        ProcessEvent::StdoutDone
    } else {
        ProcessEvent::StderrDone
    });
}

fn receive_events(
    rx: &Receiver<ProcessEvent>,
    output: &mut ProcessOutput,
    stdout_done: &mut bool,
    stderr_done: &mut bool,
    overflow: &mut bool,
) {
    while let Ok(event) = rx.try_recv() {
        apply_event(event, output, stdout_done, stderr_done, overflow);
    }
}

fn apply_event(
    event: ProcessEvent,
    output: &mut ProcessOutput,
    stdout_done: &mut bool,
    stderr_done: &mut bool,
    overflow: &mut bool,
) {
    match event {
        ProcessEvent::Stdout(bytes) => append_bounded(&mut output.stdout, &bytes, overflow),
        ProcessEvent::Stderr(bytes) => append_bounded(&mut output.stderr, &bytes, overflow),
        ProcessEvent::StdoutDone => *stdout_done = true,
        ProcessEvent::StderrDone => *stderr_done = true,
    }
}

fn append_bounded(output: &mut Vec<u8>, bytes: &[u8], overflow: &mut bool) {
    if output.len() + bytes.len() > OUTPUT_LIMIT {
        *overflow = true;
        let excess = (output.len() + bytes.len()).saturating_sub(OUTPUT_LIMIT);
        if excess >= output.len() {
            output.clear();
            output.extend_from_slice(&bytes[bytes.len().saturating_sub(OUTPUT_TAIL)..]);
        } else {
            output.drain(..excess);
            output.extend_from_slice(bytes);
        }
        if output.len() > OUTPUT_LIMIT {
            output.drain(..output.len() - OUTPUT_LIMIT);
        }
    } else {
        output.extend_from_slice(bytes);
    }
}

fn tail(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(OUTPUT_TAIL)..]).into_owned()
}

fn diagnostic(
    expectation: &str,
    input_lines: usize,
    input_write_result: &Result<(), String>,
    output: &Output,
) -> String {
    let input_delivery = match input_write_result {
        Ok(()) => format!("scripted stdin was fully sent and closed ({input_lines} lines)"),
        Err(error) => {
            format!("scripted stdin write did not complete ({input_lines} lines expected): {error}")
        }
    };
    format!(
        "{expectation}; {input_delivery}\nstdout tail:\n{}\nstderr tail:\n{}",
        tail(&output.stdout),
        tail(&output.stderr)
    )
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
fn marker_stops_a_still_running_process_and_keeps_both_streams() {
    let input = "x".repeat(16 * 1024 * 1024);
    let output = run_until_stdout_marker(
        &fixture_path("e2e_marker_then_loop.tbx"),
        &input,
        "EXPECTED READY MARKER",
    );

    assert!(stdout_text(&output).contains("EXPECTED READY MARKER"));
    // The marker is the success condition; killing the still-running process is expected.
    assert!(!output.status.success());
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn marker_observed_before_or_during_natural_exit_is_recovered() {
    let output = run_until_stdout_marker(
        &fixture_path("e2e_marker_then_exit.tbx"),
        "",
        "EXPECTED READY MARKER",
    );

    assert!(stdout_text(&output).contains("EXPECTED READY MARKER"));
}

#[test]
fn missing_marker_after_natural_exit_fails_with_recovered_output() {
    let result = std::panic::catch_unwind(|| {
        run_until_stdout_marker(
            &fixture_path("e2e_no_marker_exit.tbx"),
            "first line\nsecond line\n",
            "EXPECTED READY MARKER",
        )
    });
    let panic = result.expect_err("a naturally exited process without marker should fail");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("failure should include a diagnostic");

    assert!(message.contains("not reached before process exit"));
    assert!(
        message.contains("scripted stdin was fully sent and closed (2 lines)")
            || message.contains("scripted stdin write did not complete (2 lines expected)")
    );
    assert!(message.contains("PROCESS EXITED WITHOUT MARKER"));
}

#[test]
fn early_process_exit_reports_incomplete_stdin_write() {
    let input = "x".repeat(16 * 1024 * 1024);
    let result = std::panic::catch_unwind(|| {
        run_until_stdout_marker(
            &fixture_path("e2e_no_marker_exit.tbx"),
            &input,
            "EXPECTED READY MARKER",
        )
    });
    let panic = result.expect_err("early child exit should fail an incomplete stdin write");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("failure should include a diagnostic");

    assert!(message.contains("not reached before process exit"));
    assert!(message.contains("PROCESS EXITED WITHOUT MARKER"));
    assert!(message.contains("scripted stdin write did not complete"));
    assert!(!message.contains("scripted stdin was fully sent and closed"));
}

#[test]
fn stdin_eof_reprompt_loop_times_out_with_output_diagnostic() {
    let child = Command::new(tbx_next_bin())
        .arg(fixture_path("runtime_input_eof_loop.tbx"))
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");
    let result = std::panic::catch_unwind(|| {
        run_child_with_timeout(
            child,
            "",
            None,
            "file runtime input",
            0,
            Duration::from_millis(250),
        )
    });
    let panic = result.expect_err("an EOF re-prompt loop should be stopped by the timeout");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("failure should include a diagnostic");

    assert!(message.contains("timed out"));
    assert!(message.contains("scripted stdin was fully sent and closed (0 lines)"));
    assert!(message.contains("PROMPT"));
}

#[test]
fn marker_helper_times_out_when_process_stays_alive_without_marker() {
    let child = Command::new(tbx_next_bin())
        .arg(fixture_path("e2e_silent_loop.tbx"))
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");
    let result = std::panic::catch_unwind(|| {
        run_child_with_timeout(
            child,
            "",
            Some((b"EXPECTED READY MARKER", 1)),
            "stdout marker",
            0,
            Duration::from_millis(250),
        )
    });
    let panic = result.expect_err("marker helper should fail when marker is not reached");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("failure should include a diagnostic");

    assert!(message.contains("timed out"));
    assert!(message.contains("stdout marker"));
}

#[test]
fn excessive_interactive_output_is_bounded_and_stopped() {
    let child = Command::new(tbx_next_bin())
        .arg(fixture_path("e2e_output_flood.tbx"))
        .current_dir(fixture_directory())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tbx-next binary should spawn");
    let result = std::panic::catch_unwind(|| {
        run_child_with_timeout(
            child,
            "",
            Some((b"NEVER EMITTED MARKER", 1)),
            "stdout marker",
            0,
            Duration::from_secs(3),
        )
    });
    let panic = result.expect_err("excessive output should stop the child");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("failure should include a diagnostic");

    assert!(message.contains("output exceeded"));
    assert!(message.contains("OUTPUT FLOOD"));
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
fn explicit_seed_preserves_the_compatible_rnd_series_for_stdin_source() {
    let output = run_with_args_and_stdin(
        &["--seed", "42"],
        "PUTDEC RND(10)\nCR\nPUTDEC RND(100)\nCR\nPUTDEC RND(97)\nCR\nPUTDEC RND(32767)\nCR\nPUTDEC RND(10)\nCR\n",
    );

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "1\n99\n38\n12313\n9\n");
    assert_eq!(stderr_text(&output), "");
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
    let output = run_with_stdin("VAR LEFT_VALUE\nVAR RIGHT_VALUE\nLET LEFT_VALUE = 4\nLET RIGHT_VALUE = 5\nPRINT \"TOTAL = \", LEFT_VALUE + RIGHT_VALUE, \"!\"\n");

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
fn file_success_returns_early_from_runtime_word_inside_for_loop() {
    let path = fixture_path("early_return.tbx");

    let output = run_with_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr:\n{}",
        stderr_text(&output)
    );
    assert_eq!(stdout_text(&output), "11\n1\n");
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
fn seeded_sttr1_entry_point_returns_to_command_after_the_normal_trace() {
    let output = run_until_nth_stdout_marker_with_args(
        &["--seed", "42"],
        &example_path("sttr1/main.tbx"),
        "1\n2\n6\n7\n1\n",
        "COMMAND (0-7):",
        5,
    );
    let stdout = stdout_text(&output);

    assert!(stdout.contains("STAR TREK MISSION"), "{stdout}");
    assert!(stdout.contains("DEADLINE "), "{stdout}");
    assert!(stdout.contains("KLINGONS "), "{stdout}");
    assert!(stdout.contains("STARBASES "), "{stdout}");
    assert!(stdout.contains("COMMANDS (0-7)"), "{stdout}");
    assert!(stdout.contains("SHORT RANGE SCAN"), "{stdout}");
    assert!(stdout.contains("LONG RANGE SCAN"), "{stdout}");
    assert!(stdout.contains("DAMAGE REPORT"), "{stdout}");
    assert!(
        stdout.contains("COMPUTER OPTION (0 CHART, 1 STATUS, 2 KLINGONS):"),
        "{stdout}"
    );
    assert!(stdout.contains("KLINGONS_LEFT "), "{stdout}");
    assert!(stdout.contains("DEADLINE - STARDATE "), "{stdout}");
    assert!(stdout.contains("BASES_LEFT "), "{stdout}");
    assert!(stdout.matches("COMMAND (0-7):").count() >= 5, "{stdout}");
    // Reaching the fifth prompt is the success condition; the game remains interactive.
    assert!(!output.status.success());
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn file_success_runs_the_guess_example_with_runtime_input() {
    let path = example_path("guess.tbx");
    let input = (1..=100)
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
