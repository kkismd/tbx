use super::*;

#[test]
fn sttr1_navigation_interpolates_course_and_applies_small_warp_time_rules() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET STARDATE = 100\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("15".to_owned())), Ok(Some("2".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [1, 1, 5, 4, 104, 100]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_uses_original_y_sign_and_zero_warp_cost() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET STARDATE = 100\n\
LET DEADLINE = 99\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("15".to_owned())), Ok(Some("0".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [4, 4, 105, 100]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_warp_eight_uses_all_sixty_four_steps() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET STARDATE = 100\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("80".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [9, 1, 4, 4, 41, 101]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_crossing_uses_final_quadrant_without_intermediate_obstacles() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 8\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
LET @GALAXY[2] = 108\n\
INIT_QUADRANT\n\
LET STARDATE = 100\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [2, 1, 8, 4, 97, 101]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_stops_before_obstacle_and_charges_planned_steps() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET @SECTOR[29] = 4\n\
LET STARDATE = 100\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [4, 4, 97, 101]
    );
    assert!(writer
        .text()
        .contains("WARP ENGINES SHUTDOWN AT SECTOR 5,4 DUE TO OBSTACLE"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_combat_quadrant_reports_dangerously_low_shields() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
LET SHIELDS = 200\n\
INIT_QUADRANT\n\
PRINT \"LOW_SHIELDS_STATE \", KLINGONS_HERE, \" \", SHIELDS\n\
CR\n\
PRINT \"SAFE_SHIELDS_START\"\n\
CR\n\
LET SHIELDS = 201\n\
INIT_QUADRANT\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        30,
    ));

    assert_eq!(output_values(writer.text(), "LOW_SHIELDS_STATE "), [1, 200]);
    assert!(writer
        .text()
        .contains("COMBAT AREA: SHIELDS DANGEROUSLY LOW"));
    let safe_state_output = writer
        .text()
        .split("SAFE_SHIELDS_START")
        .nth(1)
        .expect("safe-shield marker should be present");
    assert!(!safe_state_output.contains("COMBAT AREA: SHIELDS DANGEROUSLY LOW"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_handles_galaxy_edge_and_returns_to_known_quadrant() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 8\n\
LET ENT_QY = 1\n\
LET ENT_SX = 8\n\
LET ENT_SY = 4\n\
LET @GALAXY[8] = 0\n\
INIT_QUADRANT\n\
LET STARDATE = 100\n\
LET ENERGY = 100\n\
NAVIGATE\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([
        Ok(Some("10".to_owned())),
        Ok(Some("10".to_owned())),
        Ok(Some("50".to_owned())),
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

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [8, 1, 8, 4, 94, 102]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_retries_from_course_after_invalid_warp_and_cancel_preserves_state() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET @DAMAGE[1] = -1\n\
LET STARDATE = 100\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([
        Ok(Some("7".to_owned())),
        Ok(Some("10".to_owned())),
        Ok(Some("3".to_owned())),
        Ok(Some("0".to_owned())),
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

    assert!(writer.text().contains("COURSE OUT OF RANGE"));
    assert!(writer
        .text()
        .contains("WARP ENGINES ARE DAMAGED, MAXIMUM INPUT = 2 (WARP 0.2)"));
    assert!(writer.text().contains("COURSE SCALE: INPUT 10=1.0"));
    assert!(writer
        .text()
        .contains("DIRECTIONS: 10 E, 20 NE, 30 N, 40 NW, 50 W, 60 SW, 70 S, 80 SE"));
    assert!(writer
        .text()
        .contains("WARP SCALE: INPUT 0-80 = WARP 0.0-8.0 (10=1.0)"));
    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [1, 1, 4, 4, 100, 100]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_repairs_devices_after_valid_input_before_moving() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET @DAMAGE[1] = -100\n\
NAVIGATE\n\
PRINT \"DEVICE_AFTER_NAVIGATION \"\n\
PRINT @DAMAGE[1]\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("0".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_ne!(
        output_values(writer.text(), "DEVICE_AFTER_NAVIGATION "),
        [-100]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_retaliates_before_movement() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
INIT_QUADRANT\n\
LET SECTOR_INDEX = 1\n\
WHILE SECTOR_INDEX <= 64\n\
  LET @SECTOR[SECTOR_INDEX] = 0\n\
  LET SECTOR_INDEX = SECTOR_INDEX + 1\n\
ENDWH\n\
LET @SECTOR[28] = 0\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 8\n\
LET @KLINGON_Y[1] = 8\n\
LET @KLINGON_E[1] = 200\n\
LET SHIELDS = 1000\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_ATTACK_STATE \"\n\
PRINT ENT_SX, \" \", ENT_SY, \" \", SHIELDS, \" \", ENERGY\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("15".to_owned())), Ok(Some("2".to_owned()))]);
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
    assert!(output.contains("KLINGON ATTACK FROM SECTOR 8,8: DAMAGE"));
    assert_eq!(
        output_values(output, "NAVIGATION_ATTACK_STATE "),
        [5, 4, 998, 104]
    );
    assert!(
        output.find("KLINGON ATTACK FROM").unwrap()
            < output.find("NAVIGATION_ATTACK_STATE").unwrap()
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_stops_before_events_and_movement_when_retaliation_destroys_ship() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
INIT_QUADRANT\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
LET SHIELDS = 1\n\
LET ENERGY = 100\n\
LET STARDATE = 100\n\
LET @DAMAGE[1] = -100\n\
NAVIGATE\n\
CHECK_ENDGAME\n\
PRINT \"NAVIGATION_DEFEAT_STATE \"\n\
PRINT ENT_SX, \" \", ENT_SY, \" \", SHIELDS, \" \", ENERGY, \" \", STARDATE, \" \", @DAMAGE[1]\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("2".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_DEFEAT_STATE "),
        [4, 4, -15, 100, 100, -100]
    );
    assert!(writer.text().contains("REASON DESTROYED"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_without_klingons_consumes_no_retaliation_rng() {
    let setup = "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET DOCKED = 0\n\
LET DEVICE_INDEX = 1\n\
WHILE DEVICE_INDEX <= 8\n\
  LET @DAMAGE[DEVICE_INDEX] = 0\n\
  LET DEVICE_INDEX = DEVICE_INDEX + 1\n\
ENDWH\n\
";
    let mut navigation_source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    navigation_source.push_str(setup);
    navigation_source.push_str(
        "NAVIGATE\n\
PRINT \"RNG_AFTER_EMPTY_NAVIGATION \"\n\
PRINT RND(100), \" \", RND(100), \" \", RND(100)\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &navigation_source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("0".to_owned()))]);
    let mut writer = RecordingWriter::default();
    let navigation_result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let mut control_source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    control_source.push_str(setup);
    control_source.push_str(
        "NAVIGATION_DEVICE_EVENTS\n\
PRINT \"RNG_EMPTY_NAVIGATION_CONTROL \"\n\
PRINT RND(100), \" \", RND(100), \" \", RND(100)\n\
CR\n",
    );
    let (control_sources, control_standard_library_id, control_source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &control_source);
    let mut control_writer = RecordingWriter::default();
    let control_result = success(execute_registered_sources_with_filesystem_and_seed(
        control_sources,
        control_standard_library_id,
        control_source_id,
        &mut control_writer,
        None,
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "RNG_AFTER_EMPTY_NAVIGATION "),
        output_values(control_writer.text(), "RNG_EMPTY_NAVIGATION_CONTROL ")
    );
    assert_eq!(navigation_result.data_stack(), []);
    assert_eq!(control_result.data_stack(), []);
}

#[test]
fn sttr1_docked_navigation_preserves_shields_and_retaliation_rng_while_moving() {
    let setup = "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET SECTOR_INDEX = 1\n\
WHILE SECTOR_INDEX <= 64\n\
  LET @SECTOR[SECTOR_INDEX] = 0\n\
  LET SECTOR_INDEX = SECTOR_INDEX + 1\n\
ENDWH\n\
LET @SECTOR[64] = 2\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 8\n\
LET @KLINGON_Y[1] = 8\n\
LET @KLINGON_E[1] = 200\n\
LET DOCKED = 1\n\
LET SHIELDS = 77\n\
LET DEVICE_INDEX = 1\n\
WHILE DEVICE_INDEX <= 8\n\
  LET @DAMAGE[DEVICE_INDEX] = 0\n\
  LET DEVICE_INDEX = DEVICE_INDEX + 1\n\
ENDWH\n\
";
    let mut navigation_source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    navigation_source.push_str(setup);
    navigation_source.push_str(
        "NAVIGATE\n\
PRINT \"DOCKED_NAVIGATION_STATE \"\n\
PRINT ENT_SX, \" \", ENT_SY, \" \", SHIELDS\n\
CR\n\
PRINT \"DOCKED_NAVIGATION_RNG \"\n\
PRINT RND(100), \" \", RND(100), \" \", RND(100)\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &navigation_source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("2".to_owned()))]);
    let mut writer = RecordingWriter::default();
    let navigation_result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let mut control_source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    control_source.push_str(setup);
    control_source.push_str(
        "NAVIGATION_DEVICE_EVENTS\n\
PRINT \"DOCKED_NAVIGATION_CONTROL_RNG \"\n\
PRINT RND(100), \" \", RND(100), \" \", RND(100)\n\
CR\n",
    );
    let (control_sources, control_standard_library_id, control_source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &control_source);
    let mut control_writer = RecordingWriter::default();
    let control_result = success(execute_registered_sources_with_filesystem_and_seed(
        control_sources,
        control_standard_library_id,
        control_source_id,
        &mut control_writer,
        None,
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "DOCKED_NAVIGATION_STATE "),
        [5, 4, 77]
    );
    assert!(!writer.text().contains("KLINGON ATTACK FROM"));
    assert_eq!(
        output_values(writer.text(), "DOCKED_NAVIGATION_RNG "),
        output_values(control_writer.text(), "DOCKED_NAVIGATION_CONTROL_RNG ")
    );
    assert_eq!(navigation_result.data_stack(), []);
    assert_eq!(control_result.data_stack(), []);
}

#[test]
fn sttr1_navigation_updates_condition_after_entering_klingon_quadrant_without_scan() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 8\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
LET @GALAXY[2] = 100\n\
LET @GALAXY[3] = 0\n\
INIT_QUADRANT\n\
LET ENERGY = 3000\n\
LET SHIELDS = 100\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", KLINGONS_HERE, \" \", CONDITION\n\
CR\n\
LET ENERGY = 100\n\
NAVIGATE\n\
PRINT \"SAFE_NAVIGATION_STATE \", ENT_QX, \" \", ENT_QY, \" \", KLINGONS_HERE, \" \", CONDITION\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([
        Ok(Some("10".to_owned())),
        Ok(Some("10".to_owned())),
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

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [2, 1, 1, 2]
    );
    assert_eq!(
        output_values(writer.text(), "SAFE_NAVIGATION_STATE "),
        [3, 1, 0, 1]
    );
    assert_eq!(writer.text().matches("SHORT RANGE SCAN").count(), 1);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_navigation_updates_condition_and_docking_after_same_quadrant_move() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET SECTOR_INDEX = 1\n\
WHILE SECTOR_INDEX <= 64\n\
  LET @SECTOR[SECTOR_INDEX] = 0\n\
  LET SECTOR_INDEX = SECTOR_INDEX + 1\n\
ENDWH\n\
LET @SECTOR[28] = 1\n\
LET @SECTOR[30] = 3\n\
LET KLINGONS_HERE = 0\n\
LET ENERGY = 123\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 77\n\
NAVIGATE\n\
PRINT \"NAVIGATION_STATE \", ENT_SX, \" \", ENT_SY, \" \", DOCKED, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS, \" \", CONDITION\n\
CR\n\
LET @SECTOR[37] = 2\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 5\n\
LET @KLINGON_E[1] = 200\n\
LET KLINGONS_HERE = 1\n\
KLINGON_ATTACK\n\
PRINT \"DOCKED_ATTACK_STATE \", DOCKED, \" \", SHIELDS, \" \", @KLINGON_E[1]\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned())), Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "NAVIGATION_STATE "),
        [5, 4, 1, 3000, 10, 0, 3]
    );
    assert_eq!(
        output_values(writer.text(), "DOCKED_ATTACK_STATE "),
        [1, 0, 200]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_invalid_and_cancel_navigation_preserve_damage_and_rng() {
    let setup = "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 0\n\
INIT_QUADRANT\n\
LET SECTOR_INDEX = 1\n\
WHILE SECTOR_INDEX <= 64\n\
  LET @SECTOR[SECTOR_INDEX] = 0\n\
  LET SECTOR_INDEX = SECTOR_INDEX + 1\n\
ENDWH\n\
LET @SECTOR[28] = 1\n\
LET @SECTOR[29] = 3\n\
LET DOCKED = 0\n\
LET CONDITION = 2\n\
LET ENERGY = 123\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 77\n\
LET @DAMAGE[1] = -100\n\
";
    let mut navigation_source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    navigation_source.push_str(setup);
    navigation_source.push_str(
        "NAVIGATE\n\
PRINT \"DAMAGE_AFTER_INVALID_CANCEL \"\n\
PRINT @DAMAGE[1]\n\
CR\n\
PRINT \"SHIP_STATE_AFTER_INVALID_CANCEL \"\n\
PRINT DOCKED, \" \", CONDITION, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS\n\
CR\n\
PRINT \"RNG_AFTER_INVALID_CANCEL \"\n\
PRINT RND(100)\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &navigation_source);
    let mut input = TestInput::new([Ok(Some("7".to_owned())), Ok(Some("0".to_owned()))]);
    let mut writer = RecordingWriter::default();
    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let mut control_source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    control_source.push_str(setup);
    control_source.push_str(
        "PRINT \"DAMAGE_CONTROL \"\n\
PRINT @DAMAGE[1]\n\
CR\n\
PRINT \"SHIP_STATE_CONTROL \"\n\
PRINT DOCKED, \" \", CONDITION, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS\n\
CR\n\
PRINT \"RNG_CONTROL \"\n\
PRINT RND(100)\n\
CR\n",
    );
    let (control_sources, control_standard_library_id, control_source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &control_source);
    let mut control_writer = RecordingWriter::default();
    let control_result = success(execute_registered_sources_with_filesystem_and_seed(
        control_sources,
        control_standard_library_id,
        control_source_id,
        &mut control_writer,
        None,
        30,
    ));

    assert_eq!(
        output_values(writer.text(), "DAMAGE_AFTER_INVALID_CANCEL "),
        [-100]
    );
    assert_eq!(
        output_values(writer.text(), "SHIP_STATE_AFTER_INVALID_CANCEL "),
        [0, 2, 123, 2, 77]
    );
    assert_eq!(
        output_values(control_writer.text(), "SHIP_STATE_CONTROL "),
        [0, 2, 123, 2, 77]
    );
    assert_eq!(
        output_values(writer.text(), "RNG_AFTER_INVALID_CANCEL "),
        output_values(control_writer.text(), "RNG_CONTROL ")
    );
    assert_eq!(result.data_stack(), []);
    assert_eq!(control_result.data_stack(), []);
}
