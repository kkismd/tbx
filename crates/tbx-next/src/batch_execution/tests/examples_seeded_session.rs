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

fn sttr1_sources_with_standard_library(
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
LET KLINGON_INDEX = 1
WHILE KLINGON_INDEX <= 3
  PRINT @KLINGON_X[KLINGON_INDEX], " ", @KLINGON_Y[KLINGON_INDEX], " ", @KLINGON_E[KLINGON_INDEX], " "
  LET KLINGON_INDEX = KLINGON_INDEX + 1
ENDWH
CR
PRINT_SHORT_SCAN
PRINT_LONG_SCAN
"#,
    );
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

fn output_values(output: &str, label: &str) -> Vec<i16> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .unwrap_or_else(|| panic!("missing {label:?} output line"))
        .split_whitespace()
        .map(|value| value.parse().expect("state output should contain integers"))
        .collect()
}
