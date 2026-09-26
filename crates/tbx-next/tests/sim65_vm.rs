use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const CYCLE_LIMIT: &str = "5000000";

struct TempDir(PathBuf);

impl TempDir {
    fn new(root: &Path) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after UNIX epoch")
            .as_nanos();
        let path = root
            .join(".tmp")
            .join(format!("sim65-vm-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("create repository temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(tool: &str, args: &[&OsStr], context: &str) -> Output {
    Command::new(tool)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("could not start {tool} while {context}: {error}"))
}

fn require_success(tool: &str, context: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{tool} failed while {context} (status {:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct FixtureRun {
    output: Output,
    map: String,
    labels: String,
}

fn build_and_run(fixture: &str, error_probe: bool) -> FixtureRun {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = crate_root.join("tests/fixtures/sim65/vm");
    let temp = TempDir::new(crate_root);
    let runtime = crate_root.join("6502/vm.s");
    let fixture_source = source_root.join(format!("{fixture}.s"));
    let probe_source = source_root.join(if error_probe {
        "error_probe.s"
    } else {
        "success_probe.s"
    });
    let hook_source = source_root.join(if fixture == "poison_init" {
        "poison_init_hook.s"
    } else {
        "before_init.s"
    });
    let runtime_object = temp.0.join("vm.o");
    let fixture_object = temp.0.join("fixture.o");
    let probe_object = temp.0.join("probe.o");
    let hook_object = temp.0.join("hook.o");
    for (source, object) in [
        (&runtime, &runtime_object),
        (&fixture_source, &fixture_object),
        (&probe_source, &probe_object),
        (&hook_source, &hook_object),
    ] {
        let output = run(
            "ca65",
            &[
                OsStr::new("-t"),
                OsStr::new("sim6502"),
                OsStr::new("-I"),
                source_root.as_os_str(),
                source.as_os_str(),
                OsStr::new("-o"),
                object.as_os_str(),
            ],
            &format!("assembling {fixture}"),
        );
        require_success("ca65", &format!("assembling {}", source.display()), &output);
    }

    let executable = temp.0.join("program");
    let map_path = temp.0.join("program.map");
    let labels_path = temp.0.join("program.lbl");
    let link = run(
        "ld65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            OsStr::new("-o"),
            executable.as_os_str(),
            OsStr::new("-m"),
            map_path.as_os_str(),
            OsStr::new("-Ln"),
            labels_path.as_os_str(),
            runtime_object.as_os_str(),
            fixture_object.as_os_str(),
            probe_object.as_os_str(),
            hook_object.as_os_str(),
            OsStr::new("sim6502.lib"),
        ],
        &format!("linking {fixture}"),
    );
    require_success("ld65", &format!("linking {fixture}"), &link);
    assert!(
        link.stderr.is_empty(),
        "ld65 warnings while linking {fixture}: {}",
        String::from_utf8_lossy(&link.stderr)
    );

    let output = run(
        "sim65",
        &[
            OsStr::new("-x"),
            OsStr::new(CYCLE_LIMIT),
            executable.as_os_str(),
        ],
        &format!("running {fixture}"),
    );
    FixtureRun {
        output,
        map: fs::read_to_string(map_path).expect("read linker map"),
        labels: fs::read_to_string(labels_path).expect("read linker labels"),
    }
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn vm_success_fixtures() {
    let mut failures = Vec::new();
    for (fixture, stdout) in [
        (
            "arithmetic",
            "0\n-1\n-32768\n-1\n1\n0\n1\n0\n1\n0\n1\n0\n-32768\n",
        ),
        ("control", "0\n5\n4\n3\n2\n1\n0\n"),
        ("call", "44\n22\n"),
        ("encoder_contract", "4658\n"),
        ("encoder_immediates", ""),
        ("nonzero_entry", "0\n99\n"),
        ("arithmetic_edges", "32761\n-2\n1\n1\n1\n0\n1\n1\n"),
        ("jz_invalid_untaken", "42\n"),
        ("terminal_jz_taken", ""),
        ("terminal_jump", ""),
    ] {
        let result = build_and_run(fixture, false);
        if result.output.status.code() != Some(0) || result.output.stdout != stdout.as_bytes() {
            failures.push(format!(
                "{fixture}: status {:?}, stdout {:?}, stderr {:?}",
                result.output.status.code(),
                String::from_utf8_lossy(&result.output.stdout),
                String::from_utf8_lossy(&result.output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn vm_failure_fixtures_observe_atomic_state() {
    for (fixture, exit) in [
        ("add_overflow", 17),
        ("multiply_overflow", 17),
        ("remainder_zero", 17),
        ("remainder_overflow", 17),
        ("data_underflow", 12),
        ("data_overflow", 13),
        ("invalid_global", 16),
        ("truncated", 11),
        ("invalid_target", 11),
        ("jz_invalid_taken", 11),
        ("terminal_push", 11),
        ("terminal_putdec", 11),
        ("terminal_call", 11),
        ("terminal_jz_untaken", 11),
        ("unsupported_zero", 10),
        ("unsupported_unassigned", 10),
        ("return_underflow", 14),
        ("call_overflow", 15),
        ("nested_frames", 10),
        ("invalid_call_target", 11),
        ("invalid_call_base", 19),
        ("poison_init", 10),
        ("output_failure", 18),
    ] {
        let result = build_and_run(fixture, true);
        assert_eq!(
            result.output.status.code(),
            Some(exit),
            "{fixture}: error probe reported an invariant failure or sim65 failed: {}",
            String::from_utf8_lossy(&result.output.stderr)
        );
        assert!(result.output.stdout.is_empty(), "{fixture} produced output");
    }
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn vm_rejects_invalid_startup_metadata() {
    for (fixture, exit) in [("invalid_entry", 11), ("invalid_global_count", 16)] {
        let result = build_and_run(fixture, false);
        assert_eq!(result.output.status.code(), Some(exit), "{fixture}");
    }
}

fn symbol(labels: &str, name: &str) -> usize {
    let qualified = format!(".{name}");
    labels
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            match (fields.next(), fields.next(), fields.next()) {
                (Some("al"), Some(value), Some(found)) if found == qualified => {
                    Some(usize::from_str_radix(value, 16).expect("hexadecimal label address"))
                }
                _ => None,
            }
        })
        .next()
        .unwrap_or_else(|| panic!("missing linker symbol {name}"))
}

fn segment(map: &str, name: &str) -> (usize, usize) {
    let row = map
        .split("Segment list:")
        .nth(1)
        .expect("linker map segment list")
        .lines()
        .find(|line| line.split_whitespace().next() == Some(name))
        .unwrap_or_else(|| panic!("missing segment {name}"));
    let mut fields = row.split_whitespace();
    assert_eq!(fields.next(), Some(name));
    let start = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
    let end = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
    (start, end + 1)
}

#[test]
#[ignore = "requires ca65, ld65, and sim65; run with --ignored"]
fn vm_linker_layout_obeys_m32_segments_and_capacity() {
    let result = build_and_run("arithmetic", false);
    assert_eq!(result.output.status.code(), Some(0));
    let zp = segment(&result.map, "ZEROPAGE");
    let code = segment(&result.map, "CODE");
    let rodata = segment(&result.map, "RODATA");
    let bss = segment(&result.map, "BSS");
    assert!(zp.1 <= 0x100);
    assert!(code.0 >= 0x200);
    assert!(bss.1 <= 0xffc0);
    let main_end =
        symbol(&result.labels, "__MAIN_START__") + symbol(&result.labels, "__MAIN_SIZE__");
    assert!(bss.1 <= main_end, "VM BSS overlaps the cc65 software stack");
    for address in [
        symbol(&result.labels, "_main"),
        symbol(&result.labels, "_tbx_error_probe"),
    ] {
        assert!((code.0..code.1).contains(&address));
    }
    for name in ["_tbx_code_start", "_tbx_entry_offset", "_tbx_global_count"] {
        assert!((rodata.0..rodata.1).contains(&symbol(&result.labels, name)));
    }
    assert!((rodata.0..=rodata.1).contains(&symbol(&result.labels, "_tbx_code_end")));
    for (name, offset) in [
        ("tbx_pc", 0),
        ("tbx_base", 2),
        ("tbx_end", 4),
        ("tbx_data_depth", 6),
        ("tbx_control_depth", 7),
        ("tbx_call_depth", 8),
        ("tbx_global_count", 9),
        ("tbx_last_error", 11),
    ] {
        assert_eq!(
            symbol(&result.labels, name),
            symbol(&result.labels, "tbx_pc") + offset
        );
    }
    assert!((zp.0..zp.1).contains(&symbol(&result.labels, "tbx_pc")));
    let data = symbol(&result.labels, "tbx_data_stack");
    let frames = symbol(&result.labels, "tbx_frames");
    let globals = symbol(&result.labels, "tbx_globals");
    assert_eq!(frames - data, 64 * 2);
    assert_eq!(globals - frames, 16 * 20);
    assert!(data >= bss.0 && globals + 256 * 2 <= bss.1);
}
