use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const FIXTURES: &str = "tests/fixtures/sim65";
const CYCLE_LIMIT: &str = "1000";

struct TempDir(PathBuf);

impl TempDir {
    fn new(workspace: &Path) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after UNIX epoch")
            .as_nanos();
        let temp_root = workspace.join(".tmp");
        fs::create_dir_all(&temp_root).expect("create repository temporary directory");
        let path = temp_root.join(format!("sim65-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("create temporary sim65 directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_tool(tool: &str, args: &[&OsStr], context: &str) -> Result<Output, String> {
    Command::new(tool)
        .args(args)
        .output()
        .map_err(|error| format!("could not start {tool} while {context}: {error}"))
}

fn require_success(tool: &str, output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{tool} failed while {context} (status {}):\n{}",
        status_description(output.status),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn status_description(status: ExitStatus) -> String {
    status.code().map_or_else(
        || "terminated by signal".to_owned(),
        |code| code.to_string(),
    )
}

fn build_and_run(fixture: &str, cycle_limit: &str) -> Output {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = workspace.join(FIXTURES).join(format!("{fixture}.s"));
    let temp = TempDir::new(workspace);
    let object = temp.0.join(format!("{fixture}.o"));
    let executable = temp.0.join(fixture);

    let assemble = run_tool(
        "ca65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            source.as_os_str(),
            OsStr::new("-o"),
            object.as_os_str(),
        ],
        &format!("assembling {fixture}"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    require_success("ca65", &assemble, &format!("assembling {fixture}"));

    let link = run_tool(
        "ld65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            OsStr::new("-o"),
            executable.as_os_str(),
            object.as_os_str(),
            OsStr::new("sim6502.lib"),
        ],
        &format!("linking {fixture} for sim6502"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    require_success("ld65", &link, &format!("linking {fixture} for sim6502"));

    let run = run_tool(
        "sim65",
        &[
            OsStr::new("-x"),
            OsStr::new(cycle_limit),
            executable.as_os_str(),
        ],
        &format!("running {fixture}"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    run
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn sim65_assembly_smoke_fixtures() {
    // Check cycle exhaustion by its diagnostic because cc65 releases may map
    // the simulator's internal timeout result to different process statuses.
    let zero = build_and_run("exit_zero", CYCLE_LIMIT);
    assert_eq!(status_description(zero.status), "0");

    let seven = build_and_run("exit_seven", CYCLE_LIMIT);
    assert_eq!(status_description(seven.status), "7");

    let timeout = build_and_run("infinite_loop", CYCLE_LIMIT);
    assert!(!timeout.status.success(), "cycle exhaustion must fail");
    assert!(
        String::from_utf8_lossy(&timeout.stderr).contains("Maximum number of cycles reached."),
        "sim65 did not report cycle exhaustion (status {}):\n{}",
        status_description(timeout.status),
        String::from_utf8_lossy(&timeout.stderr)
    );
}

#[test]
fn missing_executable_is_reported_as_a_process_start_failure() {
    let error = run_tool(
        "tbx-next-tool-that-does-not-exist",
        &[],
        "checking the missing-tool diagnostic",
    )
    .expect_err("the deliberately missing tool must not start");
    assert!(error.contains("could not start tbx-next-tool-that-does-not-exist"));
    assert!(error.contains("checking the missing-tool diagnostic"));
}
