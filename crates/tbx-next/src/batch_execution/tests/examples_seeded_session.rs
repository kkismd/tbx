use super::*;

mod combat;
mod computer;
mod device;
mod endgame;
mod examples;
mod game;
mod initialization;
mod navigation;
mod quadrant_scan;
mod seeded_session;
mod ship;

fn example_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs")
        .join("next")
        .join("examples")
        .join(name)
}

pub(super) fn sttr1_sources_with_standard_library(
    standard_library: &str,
    source: &str,
) -> (
    SourceTexts,
    crate::source::SourceId,
    crate::source::SourceId,
) {
    let mut sources = SourceTexts::new();
    let standard_library_id = sources.register(standard_library, "<tbx-next-stdlib>");
    let source = source.replace(
        "\nSTART_GAME\n",
        r#"
INIT_MISSION
PRINT "QUADRANT ", ENT_QX, " ", ENT_QY, " ", KLINGONS_HERE, " ", BASES_HERE, " ", STARS_HERE
CR
PRINT "KLINGON_STATE "
LET TEST_INDEX = 1
WHILE TEST_INDEX <= 3
  PRINT @KLINGON_X[TEST_INDEX], " ", @KLINGON_Y[TEST_INDEX], " ", @KLINGON_E[TEST_INDEX], " "
  LET TEST_INDEX = TEST_INDEX + 1
ENDWH
CR
PRINT_SHORT_SCAN
PRINT_LONG_SCAN
"#,
    );
    let source = format!("VAR TEST_INDEX\n{source}");
    let main_path = example_path("sttr1/main.tbx");
    let canonical_path = std::fs::canonicalize(main_path)
        .expect("STTR1 entry point should have a canonical filesystem path");
    let source_id = sources.register_with_acquisition(
        source.as_str(),
        "docs/next/examples/sttr1/main.tbx",
        crate::source::SourceAcquisition::FileSystem { canonical_path },
    );
    (sources, standard_library_id, source_id)
}

pub(super) const STTR1_VICTORY_SEED: u64 = 30;
pub(super) const STTR1_VICTORY_INPUT: [&str; 2] = ["4", "10"];
pub(super) const STTR1_VICTORY_INPUT_BYTES: &[u8] = b"4\n10\n";

pub(super) fn sttr1_victory_source() -> String {
    std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
LET GAME_RESULT = 0
LET ENT_QX = 1
LET ENT_QY = 1
LET ENT_SX = 4
LET ENT_SY = 4
LET TEST_INDEX = 1
WHILE TEST_INDEX <= 64
  LET @SECTOR[TEST_INDEX] = 0
  LET TEST_INDEX = TEST_INDEX + 1
ENDWH
LET @SECTOR[28] = 1
LET @SECTOR[29] = 2
LET KLINGONS_HERE = 1
LET KLINGONS_LEFT = 1
LET KLINGONS_INITIAL = 1
LET @KLINGON_X[1] = 5
LET @KLINGON_Y[1] = 4
LET @KLINGON_E[1] = 200
LET @KLINGON_E[2] = 0
LET @KLINGON_E[3] = 0
LET TORPEDOES = 1
LET @DAMAGE[5] = 0
LET DOCKED = 0
LET STARDATE = START_STARDATE + 1
GAME_LOOP
PRINT "VICTORY_GAME_LOOP_STATE ", KLINGONS_LEFT, " ", GAME_RESULT
CR
"#,
            1,
        )
}

fn output_values(output: &str, label: &str) -> Vec<i16> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .unwrap_or_else(|| panic!("missing {label:?} output line"))
        .split_whitespace()
        .map(|value| value.parse().expect("state output should contain integers"))
        .collect()
}
