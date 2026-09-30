use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new(root: &Path) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after UNIX epoch")
            .as_nanos();
        let path = root
            .join(".tmp")
            .join(format!("sim65-rng-poc-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("create repository temporary directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(tool: &str, args: &[&OsStr], cwd: &Path) -> Output {
    Command::new(tool)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| panic!("could not start {tool}: {error}"))
}

fn success(tool: &str, stage: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{tool} failed during {stage}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sim_output(source: &str, root: &Path, dir: &Path, name: &str, cycles: bool) -> Output {
    let source_path = dir.join(format!("{name}.s"));
    let object = dir.join(format!("{name}.o"));
    let executable = dir.join(name);
    fs::write(&source_path, source).expect("write PoC assembly");
    let assembled = run(
        "ca65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            source_path.as_os_str(),
            OsStr::new("-o"),
            object.as_os_str(),
        ],
        root,
    );
    success("ca65", "assemble PoC", &assembled);
    let linked = run(
        "ld65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            OsStr::new("-o"),
            executable.as_os_str(),
            object.as_os_str(),
            OsStr::new("sim6502.lib"),
        ],
        root,
    );
    success("ld65", "link PoC", &linked);
    let mut args = vec![OsStr::new("-x"), OsStr::new("500000000")];
    if cycles {
        args.insert(0, OsStr::new("-c"));
    }
    args.push(executable.as_os_str());
    let output = run("sim65", &args, root);
    success("sim65", "execute PoC", &output);
    output
}

fn tool_version(name: &str) -> String {
    let output = Command::new(name)
        .arg("--version")
        .output()
        .unwrap_or_else(|error| panic!("could not start {name}: {error}"));
    success(name, "read version", &output);
    String::from_utf8_lossy(if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    })
    .trim()
    .to_owned()
}

#[test]
#[ignore = "requires ca65, ld65, od65, sim65, and sim6502.lib; run with --ignored"]
fn explicit_seed_rng_matches_host_golden_vectors_and_measures_resources() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root.parent().unwrap().parent().unwrap();
    let source = fs::read_to_string(crate_root.join("6502/poc/explicit_seed_rng.s"))
        .expect("read independent RNG PoC source");
    let temp = TempDir::new(crate_root);
    let seeds = [0_u64, 1, 42, u64::MAX];
    let expected: [[u16; 5]; 4] = [
        [1, 88, 1, 30_210, 8],
        [6, 18, 43, 18_428, 9],
        [1, 99, 38, 12_313, 9],
        [7, 80, 94, 16_256, 8],
    ];
    let bounds = [10_u16, 100, 97, 32_767, 10];
    let seed_marker = "seed_bytes: .byte $00,$00,$00,$00,$00,$00,$00,$00";
    for (seed_index, seed) in seeds.into_iter().enumerate() {
        let bytes = (0..8)
            .map(|index| format!("${:02x}", (seed >> (index * 8)) & 0xff))
            .collect::<Vec<_>>()
            .join(",");
        let seeded = source.replace(seed_marker, &format!("seed_bytes: .byte {bytes}"));
        let output = sim_output(
            &seeded,
            repo_root,
            &temp.0,
            &format!("seed-{seed_index}"),
            false,
        );
        let expected_bytes = expected[seed_index]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(output.stdout, expected_bytes, "seed {seed} PoC output");
    }

    let object = temp.0.join("measurement.o");
    let assembled = run(
        "ca65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            crate_root.join("6502/poc/explicit_seed_rng.s").as_os_str(),
            OsStr::new("-o"),
            object.as_os_str(),
        ],
        repo_root,
    );
    success("ca65", "assemble resource object", &assembled);
    let segments = run(
        "od65",
        &[OsStr::new("--dump-segments"), object.as_os_str()],
        repo_root,
    );
    success("od65", "inspect resource object", &segments);
    let segment_text = String::from_utf8_lossy(&segments.stdout);
    let mut segment_name = String::new();
    let mut sizes = Vec::new();
    for line in segment_text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("Name:") {
            segment_name = name.trim().trim_matches('"').to_owned();
        } else if let Some(size) = line.strip_prefix("Size:") {
            sizes.push((segment_name.clone(), size.trim().to_owned()));
        }
    }
    let measured_binary = temp.0.join("resource-measurement");
    let map_path = temp.0.join("resource-measurement.map");
    let labels_path = temp.0.join("resource-measurement.lbl");
    let linked = run(
        "ld65",
        &[
            OsStr::new("-t"),
            OsStr::new("sim6502"),
            OsStr::new("-o"),
            measured_binary.as_os_str(),
            OsStr::new("-m"),
            map_path.as_os_str(),
            OsStr::new("-Ln"),
            labels_path.as_os_str(),
            object.as_os_str(),
            OsStr::new("sim6502.lib"),
        ],
        repo_root,
    );
    success("ld65", "write resource map and labels", &linked);
    let map = fs::read_to_string(map_path).expect("read linker map");
    let labels = fs::read_to_string(labels_path).expect("read linker labels");

    let mut costs = Vec::new();
    for bound in bounds {
        let repeated = format!("bounds: .word {bound}, {bound}, {bound}, {bound}, {bound}");
        let measured_source = source.replace("bounds: .word 10, 100, 97, 32767, 10", &repeated);
        let baseline_source = measured_source.replace("jsr next_random", "jsr baseline_random");
        let baseline_source = baseline_source.replace(
            "; ADR #1889 defines xorshift updates",
            "baseline_random:\n    lda #0\n    sta result\n    sta result+1\n    rts\n\n; ADR #1889 defines xorshift updates",
        );
        let measured = sim_output(&measured_source, repo_root, &temp.0, "rng-cycles", true);
        let baseline = sim_output(
            &baseline_source,
            repo_root,
            &temp.0,
            "baseline-cycles",
            true,
        );
        let measured_text = format!(
            "{}{}",
            String::from_utf8_lossy(&measured.stdout),
            String::from_utf8_lossy(&measured.stderr)
        );
        let baseline_text = format!(
            "{}{}",
            String::from_utf8_lossy(&baseline.stdout),
            String::from_utf8_lossy(&baseline.stderr)
        );
        let parse = |text: &str| -> u64 {
            text.rsplit_once(" cycles")
                .and_then(|(prefix, _)| {
                    prefix
                        .chars()
                        .rev()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                        .parse()
                        .ok()
                })
                .unwrap_or_else(|| panic!("parse sim65 -c cycle report: {text:?}"))
        };
        let per_call = (parse(&measured_text) - parse(&baseline_text)) / 5;
        costs.push(format!("{bound}:{per_call}"));
    }

    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .expect("read source revision");
    success("git", "read source revision", &revision);
    eprintln!(
        "6502 RNG PoC resources: revision={} ca65=\"{}\" ld65=\"{}\" od65=\"{}\" sim65=\"{}\" object_segments={sizes:?} linker_map_segments={} linker_labels={} state_bytes=8 cycles_per_call[bound:cycles]={}",
        String::from_utf8_lossy(&revision.stdout).trim(),
        tool_version("ca65"),
        tool_version("ld65"),
        tool_version("od65"),
        tool_version("sim65"),
        map.lines().filter(|line| !line.trim().is_empty()).count(),
        labels.lines().filter(|line| !line.trim().is_empty()).count(),
        costs.join(" ")
    );
}
