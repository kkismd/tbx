use super::{evaluate, evaluate_with_seed};
use crate::batch_execution::{BatchEnvironment, SourceAcquisitionStates, SourceProcessingSession};
use crate::source::{SourceAcquisition, SourceTexts};
use crate::source_processor::SourceFormCursor;
use crate::source_word::AdditionalSourceRequest;
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
// sim65 measured 932,429,359 cycles; this limit adds about 34% headroom.
const EIGHTQUEEN_CYCLE_LIMIT: &str = "1250000000";
// sim65 measured 1,343,363 cycles; this dedicated limit adds about 34% headroom.
const MAZE_CYCLE_LIMIT: &str = "1800000";
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
const MINIMAL_ARRAY_SOURCE: &str = "DIM @VALUES[3]\nPRINT @VALUES[2]\nCR\nLET @VALUES[1] = 7\nLET @VALUES[3] = -2\nPRINT @VALUES[1]\nCR\nPRINT @VALUES[3]\nCR\n";
const MINIMAL_FIXED_TEXT_SOURCE: &str = "PRINT \"A\"\n";
const SEEDED_RND_SOURCE: &str = "PUTDEC RND(10)\nCR\nPUTDEC RND(97)\nCR\nPUTDEC RND(100)\nCR\nPUTDEC RND(32767)\nCR\nPUTDEC RND(10)\nCR\n";
const SCRIPTED_INPUT_SOURCE: &str = "VAR VALUE\nPRINT \"?\"\nIF_LET VALUE = TRY_INPUT()\nPUTDEC VALUE\nCR\nLET_ELSE\nPUTDEC 0\nCR\nENDLET\nPRINT \"?\"\nIF_LET VALUE = TRY_INPUT()\nPUTDEC VALUE\nCR\nLET_ELSE\nPUTDEC 0\nCR\nENDLET\nPRINT \"?\"\nIF_LET VALUE = TRY_INPUT()\nPUTDEC VALUE\nCR\nLET_ELSE\nPUTDEC 0\nCR\nENDLET\nPRINT \"?\"\nIF_LET VALUE = TRY_INPUT()\nPUTDEC VALUE\nCR\nLET_ELSE\nPUTDEC 0\nCR\nENDLET\n";

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

fn wrapper(artifact: &BytecodeArtifact, seed: Option<u64>) -> Result<String, String> {
    wrapper_with_input(artifact, seed, None)
}

fn wrapper_with_input(
    artifact: &BytecodeArtifact,
    seed: Option<u64>,
    input: Option<(&[u8], bool)>,
) -> Result<String, String> {
    let count = u16::try_from(artifact.array_count())
        .map_err(|_| "array count exceeds wrapper metadata".to_owned())?;
    let text_count = u16::try_from(artifact.text_count())
        .map_err(|_| "text count exceeds wrapper metadata".to_owned())?;
    let mut source = format!(
        ".setcpu \"6502\"\n\
         .export _tbx_code_start, _tbx_code_end, _tbx_entry_offset, _tbx_global_count\n\
         .export _tbx_array_count, _tbx_array_descriptors\n\
         .export _tbx_text_count, _tbx_text_descriptors\n\
         .export _tbx_before_init, _tbx_error_probe\n\
         .segment \"RODATA\"\n\
         _tbx_code_start:\n\
             .incbin \"program.bin\"\n\
         _tbx_code_end:\n\
         _tbx_entry_offset: .word {}\n\
         _tbx_global_count: .word {}\n\
         _tbx_array_count: .word {count}\n\
         _tbx_array_descriptors:\n",
        artifact.entry_offset(),
        artifact.global_slot_count()
    );
    if count == 0 {
        source.push_str(
            "    .word _tbx_empty_array_descriptor\n_tbx_empty_array_descriptor: .word 0\n",
        );
    } else {
        source.push_str("    .word _tbx_array_descriptor_table\n_tbx_array_descriptor_table:\n");
        let mut storage_bytes = 0usize;
        let mut descriptor_bytes = 0usize;
        for (slot, &length) in artifact.array_lengths().iter().enumerate() {
            let bytes = usize::from(length)
                .checked_mul(2)
                .ok_or_else(|| "array storage size overflows".to_owned())?;
            storage_bytes = storage_bytes
                .checked_add(bytes)
                .ok_or_else(|| "total array storage size overflows".to_owned())?;
            descriptor_bytes = descriptor_bytes
                .checked_add(4)
                .ok_or_else(|| "array descriptor size overflows".to_owned())?;
            source.push_str(&format!("    .word _tbx_array_{slot}, {length}\n"));
        }
        if Some(storage_bytes) != artifact.array_storage_bytes()
            || Some(descriptor_bytes) != artifact.array_descriptor_bytes()
        {
            return Err("array wrapper sizes disagree with artifact".to_owned());
        }
        source.push_str(".segment \"BSS\"\n");
        for (slot, &length) in artifact.array_lengths().iter().enumerate() {
            let bytes = usize::from(length)
                .checked_mul(2)
                .ok_or_else(|| "array storage size overflows".to_owned())?;
            source.push_str(&format!("_tbx_array_{slot}: .res {bytes}\n"));
        }
    }
    source.push_str(&format!(
        ".segment \"RODATA\"\n_tbx_text_count: .word {text_count}\n_tbx_text_descriptors:\n"
    ));
    let mut storage_bytes = 0usize;
    let mut descriptor_bytes = 0usize;
    if text_count == 0 {
        source.push_str(
            "    .word _tbx_empty_text_descriptor\n_tbx_empty_text_descriptor: .word 0\n",
        );
    } else {
        source.push_str("    .word _tbx_text_descriptor_table\n_tbx_text_descriptor_table:\n");
        for (slot, text) in artifact.texts().iter().enumerate() {
            storage_bytes = storage_bytes
                .checked_add(text.bytes().len())
                .ok_or_else(|| "total text storage size overflows".to_owned())?;
            descriptor_bytes = descriptor_bytes
                .checked_add(4)
                .ok_or_else(|| "text descriptor size overflows".to_owned())?;
            source.push_str(&format!(
                "    .word _tbx_text_{slot}, {}\n",
                text.byte_length()
            ));
        }
        for (slot, text) in artifact.texts().iter().enumerate() {
            source.push_str(&format!("_tbx_text_{slot}:\n"));
            source.push_str(&text_bytes_assembly(text.bytes()));
        }
    }
    if Some(storage_bytes) != artifact.text_storage_bytes()
        || Some(descriptor_bytes) != artifact.text_descriptor_bytes()
    {
        return Err("text wrapper sizes disagree with artifact".to_owned());
    }
    if let Some(seed) = seed {
        source.push_str(".import tbx_rng_state\n.segment \"CODE\"\n_tbx_before_init:\n");
        for (index, byte) in seed.to_le_bytes().iter().enumerate() {
            source.push_str(&format!(
                "    lda #${byte:02X}\n    sta tbx_rng_state+{index}\n"
            ));
        }
        source.push_str("    rts\n_tbx_error_probe:\n    rts\n");
    } else {
        source.push_str(".segment \"CODE\"\n_tbx_before_init:\n_tbx_error_probe:\n    rts\n");
    }
    if let Some((bytes, strict)) = input {
        if bytes.len() > u8::MAX as usize {
            return Err("scripted input exceeds 255 bytes".to_owned());
        }
        source.push_str(".export _tbx_read_byte\n.segment \"BSS\"\n_tbx_input_position: .res 1\n.segment \"CODE\"\n_tbx_read_byte:\n");
        source.push_str("    ldx _tbx_input_position\n    cpx #");
        source.push_str(&bytes.len().to_string());
        source.push_str("\n    bcs _tbx_input_exhausted\n    lda _tbx_input_script,x\n    pha\n    inc _tbx_input_position\n    pla\n    tax\n    lda #0\n    rts\n_tbx_input_exhausted:\n    lda #");
        source.push_str(if strict { "2\n" } else { "1\n" });
        source.push_str("    rts\n.segment \"RODATA\"\n_tbx_input_script:\n");
        if bytes.is_empty() {
            source.push_str("    .byte 0\n");
        } else {
            for chunk in bytes.chunks(16) {
                source.push_str("    .byte ");
                source.push_str(
                    &chunk
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                source.push('\n');
            }
        }
    } else {
        // Existing generated wrappers report capability failure only if input executes.
        source.push_str(
            ".export _tbx_read_byte\n.segment \"CODE\"\n_tbx_read_byte:\n    lda #2\n    rts\n",
        );
    }
    Ok(source)
}

fn text_bytes_assembly(bytes: &[u8]) -> String {
    let mut source = String::new();
    for chunk in bytes.chunks(16) {
        let literals = chunk
            .iter()
            .map(|byte| format!("${byte:02X}"))
            .collect::<Vec<_>>()
            .join(", ");
        source.push_str(&format!("    .byte {literals}\n"));
    }
    source
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
    seed: Option<u64>,
    expected_stdout: &[u8],
    cycle_limit: &str,
    sample: &str,
    test_name: &str,
) -> Result<(), String> {
    build_and_run_with_input(
        artifact,
        seed,
        expected_stdout,
        cycle_limit,
        sample,
        test_name,
        (None, None),
    )
}

fn build_and_run_with_input(
    artifact: &BytecodeArtifact,
    seed: Option<u64>,
    expected_stdout: &[u8],
    cycle_limit: &str,
    sample: &str,
    test_name: &str,
    input_and_expected_exit_code: (Option<(&[u8], bool)>, Option<i32>),
) -> Result<(), String> {
    let (input, expected_exit_code) = input_and_expected_exit_code;
    let expect_failure = expected_exit_code.is_some();
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

    fs::write(&program, artifact.code())
        .map_err(|error| format!("write temporary artifact {}: {error}", program.display()))?;
    fs::write(&wrapper_source, wrapper_with_input(artifact, seed, input)?).map_err(|error| {
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
        if size("ZEROPAGE") != 31 || size("BSS") != 1054 {
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
    if let Some(expected_exit_code) = expected_exit_code {
        if target.status.code() != Some(expected_exit_code) {
            return Err(format_nonzero_status(
                "sim65 exit status or cycle limit",
                target.status.code(),
                &target.stderr,
            ));
        }
    } else if !target.status.success() {
        return Err(format_nonzero_status(
            "sim65 exit status or cycle limit",
            target.status.code(),
            &target.stderr,
        ));
    }
    require_matching_stdout(&target.stdout, expected_stdout)?;
    if !expect_failure && std::env::var_os("TBX_SHOW_SIM65_STDOUT").is_some() {
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
    if let Some((segments, map, labels, ca65_version, ld65_version, sim65_version)) =
        measurement.filter(|_| !expect_failure)
    {
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
        let vm_bss_size = segments
            .iter()
            .find(|(name, _)| name == "BSS")
            .map(|(_, size)| *size)
            .ok_or_else(|| "VM object has no BSS segment".to_owned())?;
        let linked_bss_delta = bss_size
            .checked_sub(vm_bss_size)
            .ok_or_else(|| "linked BSS is smaller than VM BSS".to_owned())?;
        let array_storage_bytes = artifact
            .array_storage_bytes()
            .ok_or_else(|| "array storage size overflows".to_owned())?;
        let text_storage_bytes = artifact
            .text_storage_bytes()
            .ok_or_else(|| "text storage size overflows".to_owned())?;
        let text_descriptor_bytes = artifact
            .text_descriptor_bytes()
            .ok_or_else(|| "text descriptor size overflows".to_owned())?;
        let input_adapter_cursor_bytes = usize::from(input.is_some());
        let expected_linked_bss_delta = array_storage_bytes + input_adapter_cursor_bytes;
        if linked_bss_delta != expected_linked_bss_delta {
            return Err(format!(
                "linked BSS delta {linked_bss_delta} differs from expected wrapper storage {expected_linked_bss_delta} (arrays={array_storage_bytes}, input cursor={input_adapter_cursor_bytes})"
            ));
        }
        let input_adapter_script_bytes = input.map(|(bytes, _)| bytes.len().max(1)).unwrap_or(0);
        let input_adapter_bytes = input_adapter_script_bytes + input_adapter_cursor_bytes;
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
            "sim65 resources: sample={sample} revision={} ca65=\"{ca65_version}\" ld65=\"{ld65_version}\" sim65=\"{sim65_version}\" bytecode={} VM[{segment_sizes}] array_storage_bytes={array_storage_bytes} array_descriptor_bytes={} text_storage_bytes={text_storage_bytes} text_descriptor_bytes={text_descriptor_bytes} input_adapter_script_bytes={input_adapter_script_bytes} input_adapter_cursor_bytes={input_adapter_cursor_bytes} input_adapter_total_bytes={input_adapter_bytes} linked_bss_delta={linked_bss_delta} ZEROPAGE={zp_start:#06x}..={zp_end:#06x}({zp_size}) BSS={bss_start:#06x}..={bss_end:#06x}({bss_size}) __MAIN_START__={main_start:#06x} __MAIN_SIZE__={main_size} software_stack_boundary={stack_boundary:#06x} bss_to_stack_headroom={headroom} cycles={cycles} command=\"TBX_MEASURE_SIM65_RESOURCES=1 cargo test -p tbx-next --lib {test_name} -- --ignored --nocapture\"",
            git_revision(crate_root)?,
            artifact.code().len(),
            artifact.array_descriptor_bytes().ok_or_else(|| "array descriptor size overflows".to_owned())?
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn multisource_runtime_sequence_matches_host_reference_and_sim65() {
    let mut sources = SourceTexts::new();
    let stdlib_id = crate::cli_source::register_embedded_standard_library(&mut sources);
    let main_id = sources.register(
        "PUSH 42\nPRINT 1\nUSE \"lib.tbx\"\nPUTDEC\nCROSS\nPRINT SHARED\nPRINT @VALUES[1]\nPRINT 5\n",
        "main.tbx",
    );
    let mut environment = BatchEnvironment::new().expect("batch environment builds");
    environment
        .compile(&sources, stdlib_id)
        .expect("embedded standard library compiles without runtime execution");
    let forms = SourceFormCursor::new(sources.view(), main_id).expect("main source tokenizes");
    let mut session =
        SourceProcessingSession::with_environment_and_cursor(sources, environment, main_id, forms);
    let mut host_output = Vec::new();
    let mut hook = |sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        let (name, text) = match request.specification.as_ref() {
            "lib.tbx" => (
                "lib.tbx",
                "PRINT 2\nPRINT \"B\"\nUSE \"nested.tbx\"\nPRINT 4\nVAR SHARED\nLET SHARED = 7\nDIM @VALUES[1]\nLET @VALUES[1] = 8\nDEF CROSS\nPRINT 6\nEND\n",
            ),
            "nested.tbx" => ("nested.tbx", "PRINT 3\n"),
            other => panic!("unexpected source request: {other}"),
        };
        Ok(Some(sources.register(text, name)))
    };
    session
        .run_with_hook(&mut host_output, &mut hook, None)
        .expect("source graph executes in depth-first order");

    let units = session.runtime_units();
    let mut owners = units
        .iter()
        .map(|unit| unit.instructions())
        .collect::<Vec<_>>();
    let entries = units
        .iter()
        .map(|unit| unit.entry_location())
        .collect::<Vec<_>>();
    owners.push(session.environment.published_code.instruction_view());
    let primitive_words = super::Fixture::new().primitive_words;
    let mut reference_output = Vec::new();
    let mut reference_runtime_output =
        crate::runtime_output::WriteRuntimeOutput::new(&mut reference_output);
    let mut random = crate::random::RandomState::seeded(0x5442_582D_4E45_5854);
    let reference = crate::static_image::test_lower_and_run_sequence(
        &owners,
        &entries,
        units.len(),
        &session.environment.words,
        primitive_words,
        &session.environment.globals,
        &session.environment.arrays,
        (&mut reference_runtime_output, &mut random, None),
    )
    .expect("composed source sequence runs in ReferenceVm");
    let expected = b"12B34426785";
    assert_eq!(host_output, expected);
    assert_eq!(reference_output, expected);
    assert!(reference.halted);

    let artifact = crate::static_image::test_lower_and_encode_sequence(
        &owners,
        &entries,
        units.len(),
        &session.environment.words,
        primitive_words,
        &session.environment.globals,
        &session.environment.arrays,
    )
    .expect("composed source sequence encodes");
    build_and_run(
        &artifact,
        None,
        expected,
        "50000000",
        "multisource",
        "multisource_runtime_sequence_matches_host_reference_and_sim65",
    )
    .expect("sim65 output matches host and ReferenceVm");
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn sttr1_all_sources_encode_and_measure_sim65_resources() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let main_path = crate_root.join("../../docs/next/examples/sttr1/main.tbx");
    let canonical_path = fs::canonicalize(&main_path).expect("STTR1 main file exists");
    let original = fs::read_to_string(&canonical_path).expect("STTR1 main file is readable");
    let main_source = original
        .strip_suffix("START_GAME\n")
        .expect("only the final START_GAME call is removed");
    let mut sources = SourceTexts::new();
    let stdlib_id = crate::cli_source::register_embedded_standard_library(&mut sources);
    let main_id = sources.register_with_acquisition(
        main_source,
        "docs/next/examples/sttr1/main.tbx",
        SourceAcquisition::FileSystem { canonical_path },
    );
    let mut environment = BatchEnvironment::new().expect("batch environment builds");
    environment
        .compile(&sources, stdlib_id)
        .expect("embedded standard library compiles without runtime execution");
    let forms = SourceFormCursor::new(sources.view(), main_id).expect("STTR1 main tokenizes");
    let mut session =
        SourceProcessingSession::with_environment_and_cursor(sources, environment, main_id, forms);
    let mut acquired = Vec::new();
    let mut hook = |sources: &mut SourceTexts,
                    states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        let source_id = crate::batch_execution::acquire_filesystem_source_with_states(
            sources, states, request,
        )?;
        if let Some(source_id) = source_id {
            acquired.push(source_id);
        }
        Ok(source_id)
    };
    let mut host_output = Vec::new();
    session
        .run_with_hook(&mut host_output, &mut hook, None)
        .expect("all STTR1 sources process without entering the game loop");
    assert!(host_output.is_empty());
    assert_eq!(acquired.len(), 9);
    let source_view = session.sources().view();
    let mut actual_sources = acquired
        .iter()
        .map(|&id| {
            let SourceAcquisition::FileSystem { canonical_path } =
                source_view.acquisition(id).expect("acquired source exists")
            else {
                panic!("USE source must be acquired from the filesystem");
            };
            assert_eq!(
                source_view.source(id).expect("acquired source has text"),
                fs::read_to_string(canonical_path).expect("acquired file is readable")
            );
            canonical_path
                .file_name()
                .expect("USE source has a filename")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    actual_sources.sort();
    assert_eq!(
        actual_sources,
        [
            "combat.tbx",
            "computer.tbx",
            "device.tbx",
            "endgame.tbx",
            "galaxy.tbx",
            "game.tbx",
            "navigation.tbx",
            "ship.tbx",
            "state.tbx",
        ]
    );

    assert_eq!(session.environment.globals.len(), 40);
    assert_eq!(session.environment.arrays.len(), 9);
    let units = session.runtime_units();
    let mut owners = units
        .iter()
        .map(|unit| unit.instructions())
        .collect::<Vec<_>>();
    let entries = units
        .iter()
        .map(|unit| unit.entry_location())
        .collect::<Vec<_>>();
    let published = session.environment.published_code.instruction_view();
    assert!(
        !published.is_empty(),
        "STTR1 compiled definitions are published"
    );
    owners.push(published);
    let artifact = crate::static_image::test_lower_and_encode_sequence(
        &owners,
        &entries,
        units.len(),
        &session.environment.words,
        super::Fixture::new().primitive_words,
        &session.environment.globals,
        &session.environment.arrays,
    );
    let artifact = match artifact {
        Ok(artifact) => artifact,
        Err(error)
            if error.starts_with("UnsupportedInstruction(")
                && error.contains(": StoreScratch(N);") =>
        {
            eprintln!(
                "STTR1 artifact blocker: {error}; revision={}; command=\"TBX_MEASURE_SIM65_RESOURCES=1 cargo test -p tbx-next --lib sttr1_all_sources_encode_and_measure_sim65_resources -- --ignored --nocapture\"",
                git_revision(crate_root).unwrap_or_else(|_| "unavailable".to_owned())
            );
            if std::env::var_os("TBX_MEASURE_SIM65_RESOURCES").is_some() {
                eprintln!(
                    "STTR1 tool versions: ca65=\"{}\" ld65=\"{}\" sim65=\"{}\"",
                    tool_version("ca65").expect("ca65 version is available"),
                    tool_version("ld65").expect("ld65 version is available"),
                    tool_version("sim65").expect("sim65 version is available")
                );
            }
            return;
        }
        Err(error) => panic!("all STTR1 definitions lower and encode: {error}"),
    };
    assert_eq!(artifact.global_slot_count(), 40);
    assert_eq!(artifact.array_count(), 9);
    assert_eq!(
        artifact
            .array_lengths()
            .iter()
            .map(|&n| usize::from(n))
            .sum::<usize>(),
        227
    );
    assert_eq!(artifact.array_storage_bytes(), Some(454));
    if std::env::var_os("TBX_MEASURE_SIM65_RESOURCES").is_some() {
        eprintln!(
            "STTR1 artifact metadata: globals={} arrays={} array_cells=227 fixed_texts={} published_instructions={} bytecode={}",
            artifact.global_slot_count(),
            artifact.array_count(),
            artifact.text_count(),
            published.len(),
            artifact.code().len()
        );
    }
    build_and_run(
        &artifact,
        None,
        b"",
        "50000000",
        "sttr1-all-sources",
        "sttr1_all_sources_encode_and_measure_sim65_resources",
    )
    .expect("STTR1 artifact links and its harmless entry completes in sim65");
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn multisource_runtime_failure_keeps_prefix_and_stops_later_forms() {
    let mut sources = SourceTexts::new();
    let stdlib_id = crate::cli_source::register_embedded_standard_library(&mut sources);
    let main_id = sources.register(
        "PRINT \"A\"\nUSE \"lib.tbx\"\nTRY_INPUT\nPRINT \"BAD\"\n",
        "main.tbx",
    );
    let mut environment = BatchEnvironment::new().expect("batch environment builds");
    environment
        .compile(&sources, stdlib_id)
        .expect("embedded standard library compiles");
    let forms = SourceFormCursor::new(sources.view(), main_id).expect("main source tokenizes");
    let mut session =
        SourceProcessingSession::with_environment_and_cursor(sources, environment, main_id, forms);
    let mut host_output = Vec::new();
    let mut hook = |sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        assert_eq!(request.specification.as_ref(), "lib.tbx");
        Ok(Some(sources.register("PRINT \"B\"\n", "lib.tbx")))
    };
    let mut host_input = crate::runtime_input::TestInput::strict(std::iter::empty());
    let error = session
        .run_with_hook(&mut host_output, &mut hook, Some(&mut host_input))
        .expect_err("strict input exhaustion fails the source run");
    assert!(matches!(
        error,
        crate::source_processor::SourceProcessorError::Runtime(ref error)
            if error.is_input_failure()
    ));
    assert_eq!(host_output, b"AB");

    // Include a later compiled form in the artifact to prove failure prevents
    // the runtime sequence from reaching it.
    let suffix_id = session
        .sources_mut()
        .register("PRINT \"BAD\"\n", "after-failure.tbx");
    let suffix = {
        let crate::batch_execution::SourceProcessingSession {
            sources,
            environment,
            ..
        } = &mut session;
        environment
            .compile(sources, suffix_id)
            .expect("later form compiles for the target stop check")
    };
    let units = session.runtime_units();
    let mut owners = units
        .iter()
        .map(|unit| unit.instructions())
        .collect::<Vec<_>>();
    let entries = units
        .iter()
        .map(|unit| unit.entry_location())
        .chain(std::iter::once(suffix.entry_location()))
        .collect::<Vec<_>>();
    owners.push(suffix.instructions());
    owners.push(session.environment.published_code.instruction_view());
    let runtime_owner_count = entries.len();
    let primitive_words = super::Fixture::new().primitive_words;
    let mut reference_output = Vec::new();
    let mut reference_runtime_output =
        crate::runtime_output::WriteRuntimeOutput::new(&mut reference_output);
    let mut random = crate::random::RandomState::seeded(0x5442_582D_4E45_5854);
    let mut reference_input = crate::runtime_input::TestInput::strict(std::iter::empty());
    let result = crate::static_image::test_lower_and_run_sequence(
        &owners,
        &entries,
        runtime_owner_count,
        &session.environment.words,
        primitive_words,
        &session.environment.globals,
        &session.environment.arrays,
        (
            &mut reference_runtime_output,
            &mut random,
            Some(&mut reference_input),
        ),
    );
    assert_eq!(reference_output, host_output);
    assert_eq!(
        result,
        Err(crate::static_image::TestReferenceError::InputFailed)
    );

    let artifact = crate::static_image::test_lower_and_encode_sequence(
        &owners,
        &entries,
        runtime_owner_count,
        &session.environment.words,
        primitive_words,
        &session.environment.globals,
        &session.environment.arrays,
    )
    .expect("failing source sequence encodes");
    build_and_run_with_input(
        &artifact,
        None,
        b"AB",
        "10000000",
        "multisource-failure",
        "multisource_runtime_failure_keeps_prefix_and_stops_later_forms",
        (Some((&[], true)), Some(26)),
    )
    .expect("sim65 preserves the output prefix and exits before BAD");
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
    build_and_run(
        &artifact,
        None,
        &host_output,
        PRIME_CYCLE_LIMIT,
        "prime",
        "prime_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn scripted_try_input_source_matches_host_execution() {
    let lines = ["42", "invalid", "-7"];
    let result = super::evaluate_with_seed_and_input(
        SCRIPTED_INPUT_SOURCE,
        "scripted-input.tbx",
        true,
        true,
        0x5442_582D_4E45_5854,
        Some(&lines),
    );
    let artifact = result.artifact.expect("encode scripted input source");
    let host_output = result
        .host_output
        .expect("host executes scripted input source");
    assert_eq!(host_output, b"?42\n?0\n?-7\n?0\n");
    let mut bytes = lines.join("\n").into_bytes();
    bytes.push(b'\n');
    build_and_run_with_input(
        &artifact,
        None,
        &host_output,
        "10000000",
        "scripted-input",
        "scripted_try_input_source_matches_host_execution",
        (Some((&bytes, false)), None),
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn strict_scripted_try_input_fails_after_stdout_prefix() {
    let (host_prefix, reference_prefix, artifact) =
        super::evaluate_strict_input_failure("PRINT \"?\"\nTRY_INPUT\n");
    assert_eq!(host_prefix, b"?");
    assert_eq!(reference_prefix, host_prefix);
    build_and_run_with_input(
        &artifact,
        None,
        b"?",
        "10000000",
        "strict-input",
        "strict_scripted_try_input_fails_after_stdout_prefix",
        (Some((&[], true)), Some(26)),
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn guess_source_matches_sim65_execution() {
    let lines = ["invalid", "0", "101", "1"];
    let result = super::evaluate_with_seed_and_strict_input(
        include_str!("../../../../../../docs/next/examples/guess.tbx"),
        "guess.tbx",
        true,
        true,
        42,
        &lines,
    );
    let artifact = result.artifact.expect("encode guess source");
    let host_output = result.host_output.expect("host executes guess source");
    let output = String::from_utf8(host_output.clone()).expect("guess output is UTF-8");
    let mut search_from = 0;
    for marker in [
        "Please enter a number.",
        "Too low.",
        "Too high.",
        "Correct!",
    ] {
        let relative = output[search_from..]
            .find(marker)
            .unwrap_or_else(|| panic!("host output is missing {marker:?}: {output:?}"));
        search_from += relative + marker.len();
    }

    let mut input_bytes = lines.join("\n").into_bytes();
    input_bytes.push(b'\n');
    build_and_run_with_input(
        &artifact,
        Some(42),
        &host_output,
        "10000000",
        "guess",
        "guess_source_matches_sim65_execution",
        (Some((&input_bytes, true)), None),
    )
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
    build_and_run(
        &artifact,
        None,
        &host_output,
        MANDELBROT_CYCLE_LIMIT,
        "mandelbrot",
        "mandelbrot_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn wrapper_array_metadata_matches_artifact() {
    let no_arrays = evaluate("PUTDEC 1\nCR\n", "no-arrays.tbx", false, true)
        .artifact
        .expect("encode no-array source");
    let empty = wrapper(&no_arrays, None).expect("generate empty wrapper");
    assert!(empty.contains("_tbx_array_count: .word 0"));
    assert!(empty.contains("_tbx_array_descriptors:"));
    assert!(!empty.contains(".segment \"BSS\""));
    assert_eq!(no_arrays.array_storage_bytes(), Some(0));
    assert_eq!(no_arrays.array_descriptor_bytes(), Some(0));

    let one = evaluate(MINIMAL_ARRAY_SOURCE, "array-minimal.tbx", false, true)
        .artifact
        .expect("encode one-array source");
    let one_wrapper = wrapper(&one, None).expect("generate one-array wrapper");
    assert_eq!(one.array_lengths(), &[3]);
    assert!(one_wrapper.contains("_tbx_array_count: .word 1"));
    assert!(one_wrapper.contains("_tbx_array_descriptors:\n    .word _tbx_array_descriptor_table\n_tbx_array_descriptor_table:\n    .word _tbx_array_0, 3"));
    assert!(one_wrapper.contains("_tbx_array_0: .res 6"));
    assert_eq!(one.array_storage_bytes(), Some(6));
    assert_eq!(one.array_descriptor_bytes(), Some(4));

    let many = evaluate(
        "DIM @FIRST[2]\nDIM @SECOND[5]\nPRINT @FIRST[1]\nCR\nPRINT @SECOND[1]\nCR\n",
        "two-arrays.tbx",
        false,
        true,
    )
    .artifact
    .expect("encode two-array source");
    let many_wrapper = wrapper(&many, None).expect("generate two-array wrapper");
    assert_eq!(many.array_lengths(), &[2, 5]);
    assert!(many_wrapper.contains("_tbx_array_count: .word 2"));
    assert!(many_wrapper.contains("_tbx_array_descriptors:\n    .word _tbx_array_descriptor_table\n_tbx_array_descriptor_table:\n    .word _tbx_array_0, 2\n    .word _tbx_array_1, 5"));
    assert!(many_wrapper.contains("_tbx_array_0: .res 4\n_tbx_array_1: .res 10"));
    assert_eq!(many.array_storage_bytes(), Some(14));
    assert_eq!(many.array_descriptor_bytes(), Some(8));
}

#[test]
fn wrapper_text_metadata_matches_artifact() {
    let no_texts = evaluate("PUTDEC 1\nCR\n", "no-texts.tbx", false, true)
        .artifact
        .expect("encode no-text source");
    let empty = wrapper(&no_texts, None).expect("generate empty wrapper");
    assert!(empty.contains("_tbx_text_count: .word 0"));
    assert!(empty.contains("_tbx_text_descriptors:\n    .word _tbx_empty_text_descriptor\n_tbx_empty_text_descriptor: .word 0"));
    assert!(!empty.contains("_tbx_text_0:"));
    assert_eq!(no_texts.text_storage_bytes(), Some(0));
    assert_eq!(no_texts.text_descriptor_bytes(), Some(0));

    let one = evaluate("PRINT \"A\"\n", "one-text.tbx", false, true)
        .artifact
        .expect("encode one text");
    let one_wrapper = wrapper(&one, None).expect("generate one-text wrapper");
    assert!(one_wrapper.contains("_tbx_text_count: .word 1"));
    assert!(one_wrapper.contains("_tbx_text_descriptors:\n    .word _tbx_text_descriptor_table\n_tbx_text_descriptor_table:\n    .word _tbx_text_0, 1"));
    assert!(one_wrapper.contains("_tbx_text_0:\n    .byte $41\n"));
    assert_eq!(one.text_storage_bytes(), Some(1));
    assert_eq!(one.text_descriptor_bytes(), Some(4));

    let many = evaluate(
        "PRINT \"A\"\nPRINT \"BC\"\nPRINT \"\"\nPRINT \"A\"\n",
        "many-texts.tbx",
        false,
        true,
    )
    .artifact
    .expect("encode multiple texts");
    let many_wrapper = wrapper(&many, None).expect("generate multiple-text wrapper");
    assert_eq!(many.text_count(), 4);
    assert!(many_wrapper.contains("_tbx_text_count: .word 4"));
    assert!(many_wrapper.contains("_tbx_text_descriptor_table:\n    .word _tbx_text_0, 1\n    .word _tbx_text_1, 2\n    .word _tbx_text_2, 0\n    .word _tbx_text_3, 1\n"));
    assert!(many_wrapper.contains("_tbx_text_0:\n    .byte $41\n_tbx_text_1:\n    .byte $42, $43\n_tbx_text_2:\n_tbx_text_3:\n    .byte $41\n"));
    assert_eq!(many.text_storage_bytes(), Some(4));
    assert_eq!(many.text_descriptor_bytes(), Some(16));
}

#[test]
fn wrapper_text_bytes_use_numeric_assembly_literals() {
    assert_eq!(
        text_bytes_assembly(&[0, b'"', b'\\', 0xc3, 0xa9]),
        "    .byte $00, $22, $5C, $C3, $A9\n"
    );
    assert_eq!(text_bytes_assembly(&[]), "");
}

#[test]
fn wrapper_seed_is_little_endian_and_unseeded_hook_stays_empty() {
    let artifact = evaluate("PUTDEC 1\n", "seed-wrapper.tbx", false, true)
        .artifact
        .expect("encode seed wrapper source");
    let unseeded = wrapper(&artifact, None).expect("generate unseeded wrapper");
    assert!(unseeded.contains("_tbx_before_init:\n_tbx_error_probe:\n    rts"));
    assert!(!unseeded.contains("tbx_rng_state"));

    let seeded = wrapper(&artifact, Some(0x0102_0304_0506_0708)).expect("generate seeded wrapper");
    assert!(seeded.contains(".import tbx_rng_state"));
    assert!(
        seeded.contains("lda #$08\n    sta tbx_rng_state+0\n    lda #$07\n    sta tbx_rng_state+1")
    );
    assert!(seeded.contains("lda #$01\n    sta tbx_rng_state+7"));
}

#[test]
fn wrapper_input_adapter_maps_script_exhaustion_to_eof_or_failure() {
    let input_lines = ["12"];
    let artifact = super::evaluate_with_seed_and_input(
        "TRY_INPUT\n",
        "input-wrapper.tbx",
        false,
        true,
        0x5442_582D_4E45_5854,
        Some(&input_lines),
    )
    .artifact
    .expect("encode input source with input-aware reference run");
    let eof = wrapper_with_input(&artifact, None, Some((b"12\n", false)))
        .expect("generate EOF input wrapper");
    assert!(eof.contains("_tbx_input_script:\n    .byte 49, 50, 10"));
    assert!(eof.contains("_tbx_input_exhausted:\n    lda #1\n    rts"));
    let strict = wrapper_with_input(&artifact, None, Some((b"12\n", true)))
        .expect("generate strict input wrapper");
    assert!(strict.contains("_tbx_input_exhausted:\n    lda #2\n    rts"));
    assert!(wrapper_with_input(&artifact, None, Some((&[0; 256], false))).is_err());
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn seeded_rnd_source_matches_sim65_for_representative_seeds() {
    let mut outputs = Vec::new();
    for seed in [0, 1, 42, u64::MAX] {
        let result = evaluate_with_seed(SEEDED_RND_SOURCE, "seeded-rnd.tbx", true, true, seed);
        let artifact = result.artifact.expect("encode seeded RND source");
        let host_output = result.host_output.expect("host executes seeded RND source");
        outputs.push(host_output.clone());
        build_and_run(
            &artifact,
            Some(seed),
            &host_output,
            PRIME_CYCLE_LIMIT,
            "seeded-rnd",
            "seeded_rnd_source_matches_sim65_for_representative_seeds",
        )
        .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
    }
    assert!(outputs.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
#[ignore = "requires ca65; run with --ignored"]
fn wrapper_text_bytes_assemble_with_special_and_utf8_bytes() {
    let artifact = evaluate("PRINT \"A\"\n", "assemble-text.tbx", false, true)
        .artifact
        .expect("encode text source");
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = TempArtifacts::new(crate_root).expect("create temporary artifact directory");
    fs::write(temp.0.join("program.bin"), artifact.code()).expect("write program bytes");
    let mut source = wrapper(&artifact, None).expect("generate wrapper");
    source.push_str(".segment \"RODATA\"\n_tbx_special_bytes:\n");
    source.push_str(&text_bytes_assembly(&[0, b'"', b'\\', 0xc3, 0xa9]));
    fs::write(temp.0.join("wrapper.s"), source).expect("write wrapper source");
    let output = run_tool(
        "ca65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            OsStr::new("wrapper.s"),
            OsStr::new("-o"),
            OsStr::new("wrapper.o"),
        ],
        &temp.0,
        "assemble wrapper",
    )
    .expect("start ca65");
    require_success(&output, "assemble wrapper").expect("assemble wrapper");
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn minimal_array_source_matches_sim65_execution() {
    let result = evaluate(MINIMAL_ARRAY_SOURCE, "array-minimal.tbx", true, true);
    let artifact = result.artifact.expect("encode array source");
    let host_output = result.host_output.expect("host executes array source");
    assert_eq!(host_output, b"0\n7\n-2\n");
    build_and_run(
        &artifact,
        None,
        &host_output,
        PRIME_CYCLE_LIMIT,
        "array-minimal",
        "minimal_array_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn minimal_fixed_text_source_matches_sim65_execution() {
    let result = evaluate(
        MINIMAL_FIXED_TEXT_SOURCE,
        "fixed-text-minimal.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode fixed-text source");
    let host_output = result.host_output.expect("host executes fixed-text source");
    assert_eq!(artifact.text_count(), 1);
    assert_eq!(artifact.text_storage_bytes(), Some(1));
    assert_eq!(artifact.text_descriptor_bytes(), Some(4));
    assert_eq!(host_output, b"A");
    build_and_run(
        &artifact,
        None,
        &host_output,
        PRIME_CYCLE_LIMIT,
        "fixed-text-minimal",
        "minimal_fixed_text_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn squares_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/squares.tbx"),
        "squares.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode squares source");
    let host_output = result.host_output.expect("host executes squares source");
    assert_eq!(artifact.array_lengths(), &[10]);
    assert_eq!(artifact.text_count(), 1);
    assert_eq!(artifact.text_storage_bytes(), Some(1));
    assert_eq!(artifact.text_descriptor_bytes(), Some(4));
    build_and_run(
        &artifact,
        None,
        &host_output,
        PRIME_CYCLE_LIMIT,
        "squares",
        "squares_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn grades_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/grades.tbx"),
        "grades.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode grades source");
    let host_output = result.host_output.expect("host executes grades source");
    assert_eq!(artifact.array_lengths(), &[6]);
    assert_eq!(artifact.text_count(), 7);
    assert_eq!(artifact.text_storage_bytes(), Some(38));
    assert_eq!(artifact.text_descriptor_bytes(), Some(28));
    build_and_run(
        &artifact,
        None,
        &host_output,
        PRIME_CYCLE_LIMIT,
        "grades",
        "grades_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn eightqueen_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/eightqueen.tbx"),
        "eightqueen.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode eightqueen source");
    let host_output = result.host_output.expect("host executes eightqueen source");
    assert_eq!(artifact.array_lengths(), &[8, 8]);
    assert_eq!(artifact.array_storage_bytes(), Some(32));
    assert_eq!(artifact.array_descriptor_bytes(), Some(8));
    assert_eq!(artifact.text_count(), 0);
    assert_eq!(artifact.text_storage_bytes(), Some(0));
    assert_eq!(artifact.text_descriptor_bytes(), Some(0));
    assert_eq!(host_output, b"92\n");
    build_and_run(
        &artifact,
        None,
        &host_output,
        EIGHTQUEEN_CYCLE_LIMIT,
        "eightqueen",
        "eightqueen_source_matches_sim65_execution",
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn maze_source_matches_sim65_execution() {
    let result = evaluate(
        include_str!("../../../../../../docs/next/examples/maze.tbx"),
        "maze.tbx",
        true,
        true,
    );
    let artifact = result.artifact.expect("encode maze source");
    let host_output = result.host_output.expect("host executes maze source");
    assert_eq!(artifact.array_lengths(), &[40, 40, 40, 40, 40, 40]);
    assert_eq!(artifact.array_storage_bytes(), Some(480));
    assert_eq!(artifact.array_descriptor_bytes(), Some(24));
    assert_eq!(artifact.text_count(), 2);
    assert_eq!(artifact.text_storage_bytes(), Some(18));
    assert_eq!(artifact.text_descriptor_bytes(), Some(8));
    build_and_run(
        &artifact,
        None,
        &host_output,
        MAZE_CYCLE_LIMIT,
        "maze",
        "maze_source_matches_sim65_execution",
    )
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
        vm_object_segments("Name: \"CODE\"\nSize: 0x0012\nName: \"BSS\"\nSize: 0x03e6\n")
            .expect("parse object segment sizes");
    assert_eq!(segments, [("CODE".to_owned(), 18), ("BSS".to_owned(), 998)]);

    let map = "Modules list:\nBSS Offs=000000 Size=0003E6\nSegment list:\nName Start End Size Align\nZEROPAGE 000000 000038 000039 00001\nBSS 000D40 001125 0003E6 00001\n";
    assert_eq!(linked_segment(map, "ZEROPAGE").unwrap(), (0, 0x38, 0x39));
    assert_eq!(linked_segment(map, "BSS").unwrap(), (0xD40, 0x1125, 0x3E6));
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
