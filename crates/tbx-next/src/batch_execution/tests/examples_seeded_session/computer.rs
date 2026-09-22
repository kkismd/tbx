use super::*;

fn computer_source(source: &str) -> (SourceTexts, SourceId, SourceId) {
    sttr1_sources_with_standard_library(
        STDLIB_SOURCE,
        &format!(
            "USE \"state.tbx\"\nUSE \"galaxy.tbx\"\nUSE \"device.tbx\"\nUSE \"ship.tbx\"\nUSE \"navigation.tbx\"\nUSE \"combat.tbx\"\nUSE \"computer.tbx\"\n{source}"
        ),
    )
}

fn run_computer(source: &str, input_lines: impl IntoIterator<Item = &'static str>) -> String {
    let (sources, standard_library_id, source_id) = computer_source(source);
    let mut input = TestInput::new(
        input_lines
            .into_iter()
            .map(|line| Ok(Some(line.to_owned()))),
    );
    let mut writer = RecordingWriter::default();
    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));
    assert_eq!(result.data_stack(), []);
    writer.text().to_owned()
}

#[test]
fn sttr1_computer_rejects_a_damaged_library_computer() {
    let output = run_computer("LET @DAMAGE[8] = -1\nCOMPUTER\n", []);
    assert_eq!(output, "LIBRARY COMPUTER IS NON-OPERATIONAL\n");
}

#[test]
fn sttr1_computer_prints_unknown_and_known_chart_cells_in_row_order() {
    let output = run_computer(
        "LET ENT_QX = 3\nLET ENT_QY = 4\nLET @CHART[1] = 0\nLET @CHART[2] = 5\nLET @CHART[3] = 206\nCOMPUTER\n",
        ["0"],
    );
    let chart = output
        .lines()
        .skip_while(|line| *line != "GALACTIC CHART")
        .skip(4)
        .take(8)
        .collect::<Vec<_>>();
    assert_eq!(chart.len(), 8);
    assert!(chart[0].contains("--- 005 206 "));
    assert!(chart[0].starts_with("QY 1 "));
    assert_eq!(chart[0].split_whitespace().count(), 10);
    assert!(output.contains("KBS = KLINGONS / BASES / STARS\n"));
    assert!(output.contains("CURRENT QUADRANT 3,4\n"));
    assert!(output.contains("QX     1   2   3   4   5   6   7   8\n"));
    assert!(chart[7].starts_with("QY 8 "));
}

#[test]
fn sttr1_computer_prints_status_and_damage_report() {
    let output = run_computer(
        "LET KLINGONS_LEFT = 4\nLET DEADLINE = 80\nLET STARDATE = 50\nLET BASES_LEFT = 2\nPACK @DAMAGE = -1, -2, 0, 0, 0, 0, 0, 0\nCOMPUTER\n",
        ["1"],
    );
    assert!(output.contains("KLINGONS_LEFT 4\n"));
    assert!(output.contains("DEADLINE - STARDATE 30\n"));
    assert!(output.contains("BASES_LEFT 2\n"));
    assert!(output.contains("DAMAGE REPORT\n"));
    assert!(output.contains("WARP ENGINES -1\n"));
    assert!(output.contains("LIBRARY COMPUTER 0\n"));
}

#[test]
fn sttr1_computer_reuses_course_and_distance_for_klingon_report() {
    let output = run_computer(
        "LET ENT_SX = 4\nLET ENT_SY = 4\nLET KLINGONS_HERE = 2\nLET @KLINGON_X[1] = 6\nLET @KLINGON_Y[1] = 4\nLET @KLINGON_E[1] = 200\nLET @KLINGON_X[2] = 2\nLET @KLINGON_Y[2] = 2\nLET @KLINGON_E[2] = 200\nCOMPUTER\n",
        ["2"],
    );
    assert!(output.contains("KLINGON 1 COURSE 10 DISTANCE 20\n"));
    assert!(output.contains("KLINGON 2 COURSE 40 DISTANCE 28\n"));
}

#[test]
fn sttr1_computer_interpolates_non_diagonal_klingon_course() {
    let output = run_computer(
        "LET ENT_SX = 4\nLET ENT_SY = 4\nLET KLINGONS_HERE = 1\nLET @KLINGON_X[1] = 6\nLET @KLINGON_Y[1] = 3\nLET @KLINGON_E[1] = 200\nCOMPUTER\n",
        ["2"],
    );
    assert!(output.contains("KLINGON 1 COURSE 15 DISTANCE 22\n"));
}

#[test]
fn sttr1_computer_handles_zero_klingons_and_recovers_from_invalid_input() {
    let output = run_computer(
        "LET KLINGONS_HERE = 0\nCOMPUTER\n",
        ["not a number", "9", "2"],
    );
    assert!(output.contains("COMPUTER INPUT ERROR\n"));
    assert!(output.contains("COMPUTER OPTION OUT OF RANGE\n"));
    assert!(output.ends_with("NO KLINGONS IN THIS QUADRANT\n"));
}

#[test]
fn sttr1_computer_status_keeps_damage_control_gate() {
    let output = run_computer("LET @DAMAGE[6] = -1\nCOMPUTER\n", ["1"]);
    assert!(output.contains("KLINGONS_LEFT "));
    assert!(output.contains("DAMAGE CONTROL IS NON-OPERATIONAL\n"));
    assert!(!output.contains("DAMAGE REPORT\n"));
}
