use super::evaluate;
use crate::static_image::bytecode_6502::BytecodeArtifact;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const PRIME_CYCLE_LIMIT: &str = "50000000";
// sim65 measured 2,315,181,850 cycles; this limit adds about 30% headroom.
const MANDELBROT_CYCLE_LIMIT: &str = "3000000000";
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TempArtifacts(PathBuf);

impl TempArtifacts {
    fn new(crate_root: &Path) -> Result<Self, String> {
        let root = crate_root.join(".tmp");
        fs::create_dir_all(&root)
            .map_err(|error| format!("write temporary artifact directory: {error}"))?;
        loop {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                "static-image-sim65-{}-{sequence}",
                std::process::id()
            ));
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
         .export _tbx_array_count, _tbx_array_descriptors\n\
         .export _tbx_before_init, _tbx_error_probe\n\
         .segment \"RODATA\"\n\
         _tbx_code_start:\n\
             .incbin \"program.bin\"\n\
         _tbx_code_end:\n\
         _tbx_entry_offset: .word {}\n\
         _tbx_global_count: .word {}\n\
         _tbx_array_count: .word 0\n\
         _tbx_array_descriptors: .word _tbx_empty_array_descriptor\n\
         _tbx_empty_array_descriptor: .word 0\n\
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
        Err(format_nonzero_status(
            stage,
            output.status.code(),
            &output.stderr,
        ))
    }
}

fn format_nonzero_status(stage: &str, status_code: Option<i32>, stderr: &[u8]) -> String {
    format!(
        "{stage}: exit status {status_code:?}: {}",
        String::from_utf8_lossy(stderr)
    )
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

fn tool_version(tool: &str) -> Result<String, String> {
    let output = run_tool(
        tool,
        &[OsStr::new("--version")],
        Path::new("."),
        "tool version",
    )?;
    require_success(&output, "tool version")?;
    let version = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    Ok(String::from_utf8_lossy(version).trim().to_owned())
}

fn vm_object_segments(output: &str) -> Result<Vec<(String, usize)>, String> {
    let mut segments = Vec::new();
    let mut name = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("Name:") {
            name = Some(value.trim().trim_matches('"').to_owned());
        } else if let Some(value) = line.strip_prefix("Size:") {
            let name = name
                .take()
                .ok_or_else(|| "od65 segment size has no preceding name".to_owned())?;
            let value = value.trim();
            let size = if let Some(hex) = value.strip_prefix("0x") {
                usize::from_str_radix(hex, 16).ok()
            } else {
                value.parse().ok()
            }
            .ok_or_else(|| format!("invalid od65 segment size: {value}"))?;
            segments.push((name, size));
        }
    }
    if segments.is_empty() {
        return Err("od65 reported no object segments".to_owned());
    }
    Ok(segments)
}

fn git_revision(crate_root: &Path) -> Result<String, String> {
    let output = run_tool(
        "git",
        &[OsStr::new("rev-parse"), OsStr::new("HEAD")],
        crate_root,
        "read source revision",
    )?;
    require_success(&output, "read source revision")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn linked_segment(output: &str, wanted: &str) -> Result<(usize, usize, usize), String> {
    let mut in_segment_list = false;
    for line in output.lines() {
        if line.trim() == "Segment list:" {
            in_segment_list = true;
            continue;
        }
        if !in_segment_list {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first() == Some(&wanted) && fields.len() >= 4 {
            let parse = |field: &str| {
                usize::from_str_radix(field.trim_start_matches('$'), 16)
                    .map_err(|_| format!("invalid {wanted} linker map value: {field}"))
            };
            let start = parse(fields[1])?;
            let end = parse(fields[2])?;
            let size = parse(fields[3])?;
            return Ok((start, end, size));
        }
    }
    Err(format!("linker map has no {wanted} segment"))
}

fn label_value(output: &str, wanted: &str) -> Result<usize, String> {
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if let Some(index) = fields
            .iter()
            .position(|name| name.trim_start_matches('.') == wanted)
        {
            let value = if index == 0 {
                fields.get(1)
            } else {
                fields.get(index - 1)
            }
            .ok_or_else(|| format!("linker label {wanted} has no value"))?;
            return usize::from_str_radix(value.trim_start_matches('$'), 16)
                .map_err(|_| format!("invalid linker label {wanted}: {value}"));
        }
    }
    Err(format!("linker labels have no {wanted}"))
}

fn cycle_count(report: &[u8], expected_stdout: &[u8]) -> Result<String, String> {
    let report = report
        .strip_prefix(expected_stdout)
        .ok_or_else(|| "sim65 cycle measurement changed program stdout".to_owned())?;
    let report = String::from_utf8_lossy(report);
    let count = report
        .split_whitespace()
        .find(|field| {
            field
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<u64>()
                .is_ok()
        })
        .ok_or_else(|| format!("could not parse sim65 cycle report: {}", report.trim()))?;
    Ok(count.trim_matches(|c: char| !c.is_ascii_digit()).to_owned())
}

fn build_and_run(
    artifact: &BytecodeArtifact,
    expected_stdout: &[u8],
    cycle_limit: &str,
) -> Result<(), String> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = TempArtifacts::new(crate_root)?;
    let program = temp.0.join("program.bin");
    let wrapper_source = temp.0.join("wrapper.s");
    let runtime_source = crate_root.join("6502/vm.s");
    let runtime_object = temp.0.join("vm.o");
    let wrapper_object = temp.0.join("wrapper.o");
    let executable = temp.0.join("program");
    let map_path = temp.0.join("program.map");
    let labels_path = temp.0.join("program.lbl");
    let sample = if cycle_limit == PRIME_CYCLE_LIMIT {
        "prime"
    } else {
        "mandelbrot"
    };

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

    let measure_resources = std::env::var_os("TBX_MEASURE_SIM65_RESOURCES").is_some();
    let mut link_args = vec![
        OsStr::new("-t"),
        OsStr::new("sim6502"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ];
    if measure_resources {
        link_args.extend([
            OsStr::new("-m"),
            map_path.as_os_str(),
            OsStr::new("-Ln"),
            labels_path.as_os_str(),
        ]);
    }
    link_args.extend([
        runtime_object.as_os_str(),
        wrapper_object.as_os_str(),
        OsStr::new("sim6502.lib"),
    ]);
    let link = run_tool("ld65", &link_args, &temp.0, "link")?;
    require_success(&link, "link")?;

    let mut measurement = None;
    if measure_resources {
        let output = run_tool(
            "od65",
            &[OsStr::new("--dump-segments"), runtime_object.as_os_str()],
            &temp.0,
            "inspect VM object",
        )?;
        require_success(&output, "inspect VM object")?;
        let segments = vm_object_segments(&String::from_utf8_lossy(&output.stdout))?;
        let size = |name: &str| {
            segments
                .iter()
                .find(|(segment, _)| segment == name)
                .map(|(_, size)| *size)
                .unwrap_or(0)
        };
        if size("ZEROPAGE") != 29 || size("BSS") != 966 {
            return Err(format!(
                "unexpected VM RAM segment sizes: ZEROPAGE={} BSS={}",
                size("ZEROPAGE"),
                size("BSS")
            ));
        }
        measurement = Some((
            segments,
            fs::read_to_string(&map_path).map_err(|error| format!("read linker map: {error}"))?,
            fs::read_to_string(&labels_path)
                .map_err(|error| format!("read linker labels: {error}"))?,
            tool_version("ca65")?,
            tool_version("ld65")?,
            tool_version("sim65")?,
        ));
    }

    let target = run_tool(
        "sim65",
        &[
            OsStr::new("-x"),
            OsStr::new(cycle_limit),
            executable.as_os_str(),
        ],
        &temp.0,
        "sim65 execute",
    )?;
    require_success(&target, "sim65 exit status or cycle limit")?;
    require_matching_stdout(&target.stdout, expected_stdout)?;
    if std::env::var_os("TBX_SHOW_SIM65_STDOUT").is_some() {
        std::io::stdout()
            .write_all(&target.stdout)
            .map_err(|error| format!("write sim65 stdout: {error}"))?;
        let measurement = run_tool(
            "sim65",
            &[
                OsStr::new("-c"),
                OsStr::new("-x"),
                OsStr::new(cycle_limit),
                executable.as_os_str(),
            ],
            &temp.0,
            "sim65 cycle measurement",
        )?;
        require_success(&measurement, "sim65 cycle measurement")?;
        let report = measurement
            .stdout
            .strip_prefix(expected_stdout)
            .ok_or_else(|| "sim65 cycle measurement changed program stdout".to_owned())?;
        eprintln!(
            "sim65 cycle report: {}",
            String::from_utf8_lossy(report).trim()
        );
    }
    if let Some((segments, map, labels, ca65_version, ld65_version, sim65_version)) = measurement {
        let output = run_tool(
            "sim65",
            &[
                OsStr::new("-c"),
                OsStr::new("-x"),
                OsStr::new(cycle_limit),
                executable.as_os_str(),
            ],
            &temp.0,
            "sim65 cycle measurement",
        )?;
        require_success(&output, "sim65 cycle measurement")?;
        let cycles = cycle_count(&output.stdout, expected_stdout)?;
        let (zp_start, zp_end, zp_size) = linked_segment(&map, "ZEROPAGE")?;
        let (bss_start, bss_end, bss_size) = linked_segment(&map, "BSS")?;
        let main_start = label_value(&labels, "__MAIN_START__")?;
        let main_size = label_value(&labels, "__MAIN_SIZE__")?;
        let stack_boundary = main_start + main_size;
        let headroom = stack_boundary.checked_sub(bss_end + 1).ok_or_else(|| {
            format!("BSS ends beyond software stack boundary: BSS end={bss_end:#06x}, boundary={stack_boundary:#06x}")
        })?;
        let segment_sizes = ["CODE", "RODATA", "BSS", "ZEROPAGE"]
            .iter()
            .map(|name| {
                let size = segments
                    .iter()
                    .find(|(segment, _)| segment == name)
                    .map(|(_, size)| *size)
                    .unwrap_or(0);
                format!("{name}={size}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "sim65 resources: sample={sample} revision={} ca65=\"{ca65_version}\" ld65=\"{ld65_version}\" sim65=\"{sim65_version}\" bytecode={} VM[{segment_sizes}] ZEROPAGE={zp_start:#06x}..={zp_end:#06x}({zp_size}) BSS={bss_start:#06x}..={bss_end:#06x}({bss_size}) __MAIN_START__={main_start:#06x} __MAIN_SIZE__={main_size} software_stack_boundary={stack_boundary:#06x} bss_to_stack_headroom={headroom} cycles={cycles} command=\"TBX_MEASURE_SIM65_RESOURCES=1 cargo test -p tbx-next --lib prime_source_matches_sim65_execution -- --ignored --nocapture\"",
            git_revision(crate_root)?,
            artifact.code().len()
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn prime_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/prime.tbx"),
        "prime.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode prime source");
    let host_output = result.host_output.expect("host executes prime source");
    build_and_run(&artifact, &host_output, PRIME_CYCLE_LIMIT)
        .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn mandelbrot_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/mandelbrot.tbx"),
        "mandelbrot.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode mandelbrot source");
    let host_output = result.host_output.expect("host executes mandelbrot source");
    build_and_run(&artifact, &host_output, MANDELBROT_CYCLE_LIMIT)
        .unwrap_or_else(|error| panic!("{error}"));
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

    for stage in [
        "assemble runtime",
        "assemble wrapper",
        "link",
        "sim65 exit status or cycle limit",
    ] {
        let error = format_nonzero_status(stage, Some(7), b"broken\n");
        assert!(error.contains(stage));
        assert!(error.contains("exit status Some(7)"));
        assert!(error.contains("broken"));
    }

    let error = require_matching_stdout(b"target", b"host").expect_err("output differs");
    assert!(error.contains("stdout mismatch"));

    let segments =
        vm_object_segments("Name: \"CODE\"\nSize: 0x0012\nName: \"BSS\"\nSize: 0x03c6\n")
            .expect("parse object segment sizes");
    assert_eq!(segments, [("CODE".to_owned(), 18), ("BSS".to_owned(), 966)]);

    let map = "Modules list:\nBSS Offs=000000 Size=0003C6\nSegment list:\nName Start End Size Align\nZEROPAGE 000000 000038 000039 00001\nBSS 000D40 001105 0003C6 00001\n";
    assert_eq!(linked_segment(map, "ZEROPAGE").unwrap(), (0, 0x38, 0x39));
    assert_eq!(linked_segment(map, "BSS").unwrap(), (0xD40, 0x1105, 0x3C6));
    assert_eq!(
        label_value(
            "al 000200 .__MAIN_START__\nal 00F5F0 .__MAIN_SIZE__\n",
            "__MAIN_START__"
        )
        .unwrap(),
        0x200
    );
    assert_eq!(
        label_value("__MAIN_SIZE__ 00F5F0 RLA\n", "__MAIN_SIZE__").unwrap(),
        0xF5F0
    );
}
