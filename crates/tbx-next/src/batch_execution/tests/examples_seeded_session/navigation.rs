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
PRINT \"NAVIGATION_STATE \", ENT_SX, \" \", ENT_SY, \" \", ENERGY, \" \", STARDATE, \" \", DIRECTION_X, \" \", DIRECTION_Y\n\
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
        [4, 4, 105, 100, 8, -3]
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
        .contains("WARP ENGINES ARE DAMAGED, MAXIMUM SPEED = WARP .2"));
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
