use super::evaluate;
use crate::static_image::bytecode_6502::BytecodeArtifact;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CYCLE_LIMIT: &str = "50000000";
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TempArtifacts(PathBuf);

impl TempArtifacts {
    fn new(crate_root: &Path) -> Result<Self, String> {
        let root = crate_root.join(".tmp");
        fs::create_dir_all(&root)
            .map_err(|error| format!("write temporary artifact directory: {error}"))?;
        loop {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("prime-sim65-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("write temporary artifact directory: {error}")),
            }
        }
    }
}

impl Drop for TempArtifacts {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wrapper(artifact: &BytecodeArtifact) -> String {
    format!(
        ".setcpu \"6502\"\n\
         .export _tbx_code_start, _tbx_code_end, _tbx_entry_offset, _tbx_global_count\n\
         .export _tbx_before_init, _tbx_error_probe\n\
         .segment \"RODATA\"\n\
         _tbx_code_start:\n\
             .incbin \"program.bin\"\n\
         _tbx_code_end:\n\
         _tbx_entry_offset: .word {}\n\
         _tbx_global_count: .word {}\n\
         .segment \"CODE\"\n\
         _tbx_before_init:\n\
         _tbx_error_probe:\n\
             rts\n",
        artifact.entry_offset(),
        artifact.global_slot_count()
    )
}

fn run_tool(tool: &str, args: &[&OsStr], directory: &Path, stage: &str) -> Result<Output, String> {
    Command::new(tool)
        .args(args)
        .current_dir(directory)
        .output()
        .map_err(|error| format!("{stage}: could not start {tool}: {error}"))
}

fn require_success(output: &Output, stage: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{stage}: exit status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn require_matching_stdout(actual: &[u8], expected: &[u8]) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "stdout mismatch: host {:?}, sim65 {:?}",
            String::from_utf8_lossy(expected),
            String::from_utf8_lossy(actual)
        ))
    }
}

fn build_and_run(artifact: &BytecodeArtifact, expected_stdout: &[u8]) -> Result<(), String> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = TempArtifacts::new(crate_root)?;
    let program = temp.0.join("program.bin");
    let wrapper_source = temp.0.join("wrapper.s");
    let runtime_source = crate_root.join("6502/vm.s");
    let runtime_object = temp.0.join("vm.o");
    let wrapper_object = temp.0.join("wrapper.o");
    let executable = temp.0.join("program");

    fs::write(&program, artifact.code())
        .map_err(|error| format!("write temporary artifact {}: {error}", program.display()))?;
    fs::write(&wrapper_source, wrapper(artifact)).map_err(|error| {
        format!(
            "write temporary artifact {}: {error}",
            wrapper_source.display()
        )
    })?;

    for (source, object, stage) in [
        (&runtime_source, &runtime_object, "assemble runtime"),
        (&wrapper_source, &wrapper_object, "assemble wrapper"),
    ] {
        let output = run_tool(
            "ca65",
            &[
                OsStr::new("-t"),
                OsStr::new("sim6502"),
                source.as_os_str(),
                OsStr::new("-o"),
                object.as_os_str(),
            ],
            &temp.0,
            stage,
        )?;
        require_success(&output, stage)?;
    }

    let link = run_tool(
        "ld65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            OsStr::new("-o"),
            executable.as_os_str(),
            runtime_object.as_os_str(),
            wrapper_object.as_os_str(),
            OsStr::new("sim6502.lib"),
        ],
        &temp.0,
        "link",
    )?;
    require_success(&link, "link")?;

    let target = run_tool(
        "sim65",
        &[
            OsStr::new("-x"),
            OsStr::new(CYCLE_LIMIT),
            executable.as_os_str(),
        ],
        &temp.0,
        "sim65 execute",
    )?;
    require_success(&target, "sim65 exit status or cycle limit")?;
    require_matching_stdout(&target.stdout, expected_stdout)
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn prime_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/prime.tbx"),
        "prime.tbx",
        true,
    );
    let artifact = result.artifact.expect("encode prime source");
    let host_output = result.host_output.expect("host executes prime source");
    build_and_run(&artifact, &host_output).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn e2e_failures_identify_the_stage() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for stage in [
        "assemble runtime",
        "assemble wrapper",
        "link",
        "sim65 execute",
    ] {
        let error = run_tool("tbx-next-missing-tool", &[], root, stage)
            .expect_err("missing tool must fail");
        assert!(error.contains(&format!("{stage}: could not start")));
    }

    let failed = Command::new("sh")
        .args(["-c", "echo broken >&2; exit 7"])
        .output()
        .expect("shell starts");
    for stage in [
        "assemble runtime",
        "assemble wrapper",
        "link",
        "sim65 exit status or cycle limit",
    ] {
        let error = require_success(&failed, stage).expect_err("nonzero status must fail");
        assert!(error.contains(stage));
        assert!(error.contains("exit status Some(7)"));
        assert!(error.contains("broken"));
    }

    let error = require_matching_stdout(b"target", b"host").expect_err("output differs");
    assert!(error.contains("stdout mismatch"));
}
