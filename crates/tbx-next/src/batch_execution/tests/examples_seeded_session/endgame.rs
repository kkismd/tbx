use super::*;

fn run_endgame(source_suffix: &str, input_lines: &[&str]) -> String {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(source_suffix);
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new(input_lines.iter().map(|line| Ok(Some((*line).to_owned()))));
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
fn sttr1_phaser_last_klingon_sets_victory_and_prints_summary() {
    let output = run_endgame(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET KLINGONS_HERE = 1\n\
LET KLINGONS_LEFT = 1\n\
LET KLINGONS_INITIAL = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 1\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[29] = 2\n\
LET ENERGY = 1000\n\
LET SHIELDS = 10000\n\
LET DOCKED = 0\n\
LET @DAMAGE[4] = 0\n\
LET @DAMAGE[8] = 0\n\
PHASER\n\
CHECK_ENDGAME\n\
PRINT \"ENDGAME_STATE \", GAME_RESULT, \" \", KLINGONS_LEFT, \" \", EFFICIENCY\n\
CR\n",
        &["1000"],
    );
    assert!(output.contains("RESULT VICTORY\n"));
    assert_eq!(output_values(&output, "ENDGAME_STATE "), [1, 0, 1000]);
}

#[test]
fn sttr1_photon_last_klingon_sets_victory() {
    let output = run_endgame(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET KLINGONS_HERE = 1\n\
LET KLINGONS_LEFT = 1\n\
LET KLINGONS_INITIAL = 3\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
LET @SECTOR[29] = 2\n\
LET TORPEDOES = 1\n\
LET SHIELDS = 10000\n\
LET DOCKED = 0\n\
LET @DAMAGE[5] = 0\n\
PHOTON_TORPEDO\n\
CHECK_ENDGAME\n\
PRINT \"ENDGAME_STATE \", GAME_RESULT, \" \", KLINGONS_LEFT\n\
CR\n",
        &["10"],
    );
    assert!(output.contains("RESULT VICTORY\n"));
    assert_eq!(output_values(&output, "ENDGAME_STATE "), [1, 0]);
}

#[test]
fn sttr1_endgame_detects_timeout() {
    let output = run_endgame(
        "LET GAME_RESULT = 0\n\
LET KLINGONS_LEFT = 2\n\
LET KLINGONS_HERE = 1\n\
LET STARDATE = 131\n\
LET DEADLINE = 130\n\
CHECK_ENDGAME\n\
PRINT \"ENDGAME_STATE \", GAME_RESULT, \" \", END_REASON\n\
CR\n",
        &[],
    );
    assert!(output.contains("REASON TIMEOUT\n"));
    assert_eq!(output_values(&output, "ENDGAME_STATE "), [2, 1]);
}

#[test]
fn sttr1_endgame_detects_shield_defeat() {
    let output = run_endgame(
        "LET GAME_RESULT = 0\n\
LET KLINGONS_LEFT = 2\n\
LET KLINGONS_HERE = 1\n\
LET SHIELDS = -1\n\
LET ENERGY = 100\n\
CHECK_ENDGAME\n\
PRINT \"ENDGAME_STATE \", GAME_RESULT, \" \", END_REASON\n\
CR\n",
        &[],
    );
    assert!(output.contains("REASON DESTROYED\n"));
    assert_eq!(output_values(&output, "ENDGAME_STATE "), [2, 2]);
}

#[test]
fn sttr1_endgame_detects_dead_in_space_and_avoids_zero_efficiency_division() {
    let output = run_endgame(
        "LET GAME_RESULT = 0\n\
LET KLINGONS_LEFT = 3\n\
LET KLINGONS_HERE = 0\n\
LET KLINGONS_INITIAL = 3\n\
LET STARDATE = START_STARDATE\n\
LET ENERGY = 0\n\
LET SHIELDS = 0\n\
CHECK_ENDGAME\n\
PRINT \"ENDGAME_STATE \", GAME_RESULT, \" \", END_REASON, \" \", ELAPSED\n\
CR\n",
        &[],
    );
    assert!(output.contains("REASON DEAD-IN-SPACE\n"));
    assert_eq!(output_values(&output, "ENDGAME_STATE "), [2, 3, 1]);
}

#[test]
fn sttr1_endgame_is_noop_after_result_is_already_set() {
    let output = run_endgame(
        "LET GAME_RESULT = 1\n\
LET ELAPSED = 77\n\
LET EFFICIENCY = 88\n\
CHECK_ENDGAME\n\
PRINT \"REPEATED_ENDGAME_STATE \", GAME_RESULT, \" \", ELAPSED, \" \", EFFICIENCY\n\
CR\n",
        &[],
    );
    assert_eq!(
        output_values(&output, "REPEATED_ENDGAME_STATE "),
        [1, 77, 88]
    );
    assert!(!output.contains("MISSION SUMMARY"));
}

#[test]
fn sttr1_victory_efficiency_avoids_intermediate_overflow_and_caps_result() {
    let output = run_endgame(
        "LET GAME_RESULT = 0\n\
LET KLINGONS_LEFT = 0\n\
LET KLINGONS_HERE = 0\n\
LET KLINGONS_INITIAL = 33\n\
LET STARDATE = START_STARDATE + 2\n\
CHECK_ENDGAME\n\
PRINT \"EFFICIENCY_ELAPSED_2 \", EFFICIENCY\n\
CR\n\
LET GAME_RESULT = 0\n\
LET STARDATE = START_STARDATE + 1\n\
CHECK_ENDGAME\n\
PRINT \"EFFICIENCY_ELAPSED_1 \", EFFICIENCY\n\
CR\n",
        &[],
    );
    assert_eq!(output_values(&output, "EFFICIENCY_ELAPSED_2 "), [16500]);
    assert_eq!(output_values(&output, "EFFICIENCY_ELAPSED_1 "), [32767]);
}
