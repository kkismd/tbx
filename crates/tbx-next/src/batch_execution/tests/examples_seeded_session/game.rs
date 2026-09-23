use super::*;

#[test]
fn sttr1_game_briefing_and_invalid_command_dispatch_preserve_state() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
PRINT_BRIEFING
LET COMMAND = 99
DISPATCH_COMMAND
IF INPUT?()
  DROP
ELSE
  DROP
  PRINT "COMMAND INPUT ERROR"
  CR
ENDIF
LET COMMAND = 6
DISPATCH_COMMAND
LET DEADLINE = 0
CHECK_ENDGAME
"#,
            1,
        );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("not a command".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let output = writer.text();
    assert!(output.contains("DEADLINE "));
    assert!(output.contains("KLINGONS "));
    assert!(output.contains("STARBASES "));
    for command in [
        "0 NAVIGATE",
        "1 SHORT RANGE SCAN",
        "2 LONG RANGE SCAN",
        "3 PHASER",
        "4 PHOTON TORPEDO",
        "5 SHIELD CONTROL",
        "6 DAMAGE CONTROL",
        "7 COMPUTER",
    ] {
        assert!(output.contains(command), "missing command help: {command}");
    }
    assert_eq!(output.matches("COMMAND MUST BE 0-7").count(), 1);
    assert_eq!(output.matches("COMMAND INPUT ERROR").count(), 1);
    assert!(output.contains("DAMAGE REPORT"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_game_loop_checks_endgame_and_stops_after_terminal_command() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
PRINT_BRIEFING
LET KLINGONS_LEFT = 0
GAME_LOOP
"#,
            1,
        );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("99".to_owned())), Ok(Some("6".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let output = writer.text();
    assert!(output.contains("DAMAGE REPORT"));
    assert!(output.contains("MISSION SUMMARY"));
    assert!(output.contains("RESULT VICTORY"));
    assert_eq!(output.matches("COMMANDS (0-7)").count(), 1);
    assert_eq!(output.matches("COMMAND (0-7):").count(), 2);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_game_loop_reaches_victory_after_destroying_the_last_klingon() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
LET GAME_RESULT = 0
LET ENT_QX = 1
LET ENT_QY = 1
LET ENT_SX = 4
LET ENT_SY = 4
LET SECTOR_INDEX = 1
WHILE SECTOR_INDEX <= 64
  LET @SECTOR[SECTOR_INDEX] = 0
  LET SECTOR_INDEX = SECTOR_INDEX + 1
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
        );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::strict([Ok(Some("4".to_owned())), Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let output = writer.text();
    assert!(output.contains("PHOTON TORPEDO FIRED"));
    assert!(output.contains("PHOTON TORPEDO HIT KLINGON AT SECTOR 5,4"));
    assert!(output.contains("KLINGON AT SECTOR 5,4 DESTROYED"));
    assert!(output.contains("MISSION SUMMARY"));
    assert!(output.contains("RESULT VICTORY"));
    assert!(output.contains("ELAPSED 1"));
    assert!(output.contains("EFFICIENCY 1000"));
    assert_eq!(output_values(output, "VICTORY_GAME_LOOP_STATE "), [0, 1]);
    assert_eq!(output.matches("COMMAND (0-7):").count(), 1);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_game_loop_reaches_timeout_after_navigation_crosses_the_deadline() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
LET GAME_RESULT = 0
LET ENT_QX = 1
LET ENT_QY = 1
LET ENT_SX = 4
LET ENT_SY = 4
LET @GALAXY[1] = 0
LET @GALAXY[2] = 0
INIT_QUADRANT
LET KLINGONS_LEFT = 1
LET ENERGY = 100
LET SHIELDS = 0
LET STARDATE = 100
LET DEADLINE = 100
LET @DAMAGE[1] = 0
GAME_LOOP
PRINT "TIMEOUT_GAME_LOOP_STATE ", STARDATE, " ", DEADLINE, " ", GAME_RESULT, " ", END_REASON
CR
"#,
            1,
        );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::strict([
        Ok(Some("0".to_owned())),
        Ok(Some("10".to_owned())),
        Ok(Some("10".to_owned())),
    ]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let output = writer.text();
    assert!(output.contains("COURSE (0 CANCEL, 10-89):"));
    assert!(output.contains("WARP (0-80):"));
    assert!(output.contains("MISSION SUMMARY"));
    assert!(output.contains("RESULT DEFEAT"));
    assert!(output.contains("REASON TIMEOUT"));
    assert!(output.contains("KLINGONS REMAINING 1"));
    assert_eq!(
        output_values(output, "TIMEOUT_GAME_LOOP_STATE "),
        [101, 100, 2, 1]
    );
    assert_eq!(output.matches("COMMAND (0-7):").count(), 1);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_game_loop_navigation_retaliation_defeat_stops_the_mission() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
LET ENT_QX = 1
LET ENT_QY = 1
LET ENT_SX = 4
LET ENT_SY = 4
LET @GALAXY[1] = 100
INIT_QUADRANT
LET SECTOR_INDEX = 1
WHILE SECTOR_INDEX <= 64
  LET @SECTOR[SECTOR_INDEX] = 0
  LET SECTOR_INDEX = SECTOR_INDEX + 1
ENDWH
LET KLINGONS_HERE = 1
LET KLINGONS_LEFT = 1
LET @KLINGON_X[1] = 5
LET @KLINGON_Y[1] = 4
LET @KLINGON_E[1] = 200
LET SHIELDS = 1
LET ENERGY = 100
LET STARDATE = 100
LET DEADLINE = 130
LET @DAMAGE[1] = -100
GAME_LOOP
"#,
            1,
        );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([
        Ok(Some("0".to_owned())),
        Ok(Some("10".to_owned())),
        Ok(Some("2".to_owned())),
    ]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let output = writer.text();
    assert!(output.contains("KLINGON ATTACK FROM SECTOR 5,4: DAMAGE"));
    assert!(output.contains("REASON DESTROYED"));
    assert_eq!(output.matches("COMMAND (0-7):").count(), 1);
    assert!(output.contains("MISSION SUMMARY"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_game_loop_reprints_commands_after_command_input_error() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable");
    let source = source.replacen(
        "START_GAME",
        "INIT_MISSION\nPRINT_BRIEFING\nLET KLINGONS_LEFT = 0\nGAME_LOOP\n",
        1,
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([
        Ok(Some("not a command".to_owned())),
        Ok(Some("6".to_owned())),
    ]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let output = writer.text();
    assert_eq!(output.matches("COMMANDS (0-7)").count(), 2);
    assert_eq!(output.matches("COMMAND (0-7):").count(), 2);
    assert_eq!(output.matches("COMMAND INPUT ERROR").count(), 1);
    assert_eq!(result.data_stack(), []);
}
