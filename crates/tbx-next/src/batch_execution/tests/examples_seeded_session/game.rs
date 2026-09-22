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
fn sttr1_game_dispatches_each_command_and_stops_after_endgame() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 entry point should be readable")
        .replacen(
            "START_GAME",
            r#"INIT_MISSION
LET COMMAND = 0
DISPATCH_COMMAND
LET COMMAND = 1
DISPATCH_COMMAND
LET COMMAND = 2
DISPATCH_COMMAND
LET COMMAND = 3
DISPATCH_COMMAND
LET COMMAND = 4
DISPATCH_COMMAND
LET COMMAND = 5
DISPATCH_COMMAND
LET COMMAND = 6
DISPATCH_COMMAND
LET COMMAND = 7
DISPATCH_COMMAND
"#,
            1,
        );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([
        Ok(Some("0".to_owned())),
        Ok(Some("0".to_owned())),
        Ok(Some("20".to_owned())),
        Ok(Some("0".to_owned())),
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
    assert!(output.contains("SHORT RANGE SCAN"));
    assert!(output.contains("LONG RANGE SCAN"));
    assert!(output.contains("PHASER ENERGY:"));
    assert!(output.contains("COURSE (0 CANCEL, 10-89):"));
    assert!(output.contains("ENERGY AVAILABLE ="));
    assert!(output.contains("DAMAGE REPORT"));
    assert!(output.contains("COMPUTER OPTION"));
    assert_eq!(result.data_stack(), []);
}
