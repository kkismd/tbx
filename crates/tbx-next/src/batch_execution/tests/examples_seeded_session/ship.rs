use super::*;

#[test]
fn sttr1_shield_control_preserves_total_power_and_rejects_excess() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENERGY = 1000\n\
LET SHIELDS = 500\n\
LET @DAMAGE[7] = 0\n\
SHIELD_CONTROL\n\
PRINT \"SHIELD_TRANSFER_STATE \", ENERGY, \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("2000".to_owned())), Ok(Some("700".to_owned()))]);
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
        output_values(writer.text(), "SHIELD_TRANSFER_STATE "),
        [800, 700]
    );
    assert!(writer.text().contains("ENERGY EXCEEDED"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_shield_control_cancel_preserves_state() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENERGY = 1000\n\
LET SHIELDS = 500\n\
LET @DAMAGE[7] = 0\n\
SHIELD_CONTROL\n\
PRINT \"SHIELD_CANCEL_STATE \", ENERGY, \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("0".to_owned()))]);
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
        output_values(writer.text(), "SHIELD_CANCEL_STATE "),
        [1000, 500]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_docking_replenishes_ship_and_updates_condition_only_when_adjacent() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET ENERGY = 123\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 77\n\
LET KLINGONS_HERE = 0\n\
LET ENERGY = 3000\n\
CHECK_DOCKING\n\
PRINT_SHORT_SCAN\n\
LET @SECTOR[27] = 3\n\
CHECK_DOCKING\n\
PRINT_SHORT_SCAN\n\
PRINT \"DOCKED_STATE \", DOCKED, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS, \" \", CONDITION\n\
CR\n\
LET SHIELDS = 321\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
KLINGON_ATTACK\n\
PRINT \"DOCKED_ATTACK_STATE \", DOCKED, \" \", SHIELDS, \" \", @KLINGON_E[1], \" \", RND(200)\n\
CR\n\
LET @SECTOR[27] = 0\n\
LET @SECTOR[64] = 3\n\
LET KLINGONS_HERE = 0\n\
CHECK_DOCKING\n\
PRINT \"UNDOCKED_STATE \", DOCKED, \" \", ENERGY, \" \", TORPEDOES, \" \", SHIELDS, \" \", CONDITION\n\
CR\n\
LET KLINGONS_HERE = 1\n\
CHECK_DOCKING\n\
PRINT_SHORT_SCAN\n\
PRINT \"RED_STATE \", CONDITION\n\
CR\n\
LET KLINGONS_HERE = 0\n\
LET ENERGY = 100\n\
CHECK_DOCKING\n\
PRINT_SHORT_SCAN\n\
PRINT \"YELLOW_STATE \", CONDITION\n\
CR\n",
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

    assert_eq!(
        output_values(writer.text(), "DOCKED_STATE "),
        [1, 3000, 10, 0, 3]
    );
    assert!(writer.text().contains("CONDITION DOCKED"));
    assert!(writer.text().contains("CONDITION GREEN"));
    assert!(writer.text().contains("CONDITION RED"));
    assert!(writer.text().contains("CONDITION YELLOW"));
    assert!(writer.text().contains("STARDATE "));
    assert!(writer.text().contains("QUADRANT "));
    assert!(writer.text().contains("SECTOR "));
    assert!(writer.text().contains("ENERGY 3000"));
    assert!(writer.text().contains("TORPEDOES 10"));
    assert!(writer.text().contains("SHIELDS 0"));
    assert!(writer
        .text()
        .contains("DOCKED: ENERGY AND TORPEDOES REPLENISHED; SHIELDS RESET"));
    assert_eq!(
        output_values(writer.text(), "UNDOCKED_STATE "),
        [0, 3000, 10, 321, 0]
    );
    let docked_attack = output_values(writer.text(), "DOCKED_ATTACK_STATE ");
    assert_eq!(docked_attack[0..3], [1, 321, 200]);

    let control_source = source.replace("\nKLINGON_ATTACK\n", "\nREM SKIP_ATTACK\n");
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &control_source);
    let mut control_writer = RecordingWriter::default();
    success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut control_writer,
        None,
        30,
    ));
    let control_attack = output_values(control_writer.text(), "DOCKED_ATTACK_STATE ");
    assert_eq!(docked_attack[3], control_attack[3]);
    assert_eq!(output_values(writer.text(), "RED_STATE "), [2]);
    assert_eq!(output_values(writer.text(), "YELLOW_STATE "), [1]);
    assert_eq!(result.data_stack(), []);
}
