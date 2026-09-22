use super::*;
#[test]
fn sttr1_combat_helpers_use_ten_times_distance_and_checked_damage_scaling() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_SX = 2\n\
LET ENT_SY = 1\n\
LET @KLINGON_X[1] = 3\n\
LET @KLINGON_Y[1] = 1\n\
PUTDEC DIST_TO_KLINGON(1)\n\
CR\n\
LET ENT_SX = 2\n\
LET ENT_SY = 2\n\
LET @KLINGON_X[1] = 3\n\
LET @KLINGON_Y[1] = 3\n\
PUTDEC DIST_TO_KLINGON(1)\n\
CR\n\
LET ENT_SX = 1\n\
LET ENT_SY = 1\n\
LET @KLINGON_X[1] = 8\n\
LET @KLINGON_Y[1] = 8\n\
PUTDEC DIST_TO_KLINGON(1)\n\
CR\n\
PUTDEC COMBAT_DAMAGE(3000, 199, 10, 1)\n\
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

    let values = writer
        .text()
        .lines()
        .filter_map(|line| line.parse::<i16>().ok())
        .collect::<Vec<_>>();
    assert!(
        values.ends_with(&[10, 14, 98, 5970]),
        "values={values:?}\noutput={}",
        writer.text()
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_phaser_deducts_energy_then_retaliates_and_updates_klingon_state() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
LET BASES_HERE = 0\n\
LET STARS_HERE = 0\n\
LET KLINGONS_HERE = 1\n\
LET KLINGONS_LEFT = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[36] = 0\n\
LET @SECTOR[29] = 2\n\
LET ENERGY = 1000\n\
LET SHIELDS = 1000\n\
LET DOCKED = 0\n\
LET @DAMAGE[4] = 0\n\
LET @DAMAGE[7] = 0\n\
PHASER\n\
PRINT \"COMBAT_STATE \", ENERGY, \" \", SHIELDS, \" \", @KLINGON_E[1], \" \", KLINGONS_HERE, \" \", KLINGONS_LEFT, \" \", @GALAXY[1]\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("1100".to_owned())), Ok(Some("100".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "COMBAT_STATE ");
    assert_eq!(state.len(), 6, "state={state:?}\noutput={}", writer.text());
    assert_eq!(state[0], 900);
    assert!((602..=1000).contains(&state[1]));
    assert!((0..200).contains(&state[2]));
    assert_eq!(state[3], if state[2] == 0 { 0 } else { 1 });
    assert_eq!(state[4], state[3]);
    assert_eq!(state[5], state[3] * 100);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_damaged_computer_scales_phaser_once_without_overflow() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
LET BASES_HERE = 0\n\
LET STARS_HERE = 0\n\
LET KLINGONS_HERE = 1\n\
LET KLINGONS_LEFT = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[29] = 2\n\
LET ENERGY = 3000\n\
LET SHIELDS = 10000\n\
LET DOCKED = 0\n\
LET @DAMAGE[4] = 0\n\
LET @DAMAGE[7] = -1\n\
PHASER\n\
PRINT \"DAMAGED_COMPUTER_STATE \", ENERGY, \" \", @KLINGON_E[1], \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("3000".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    );
    assert!(
        matches!(result, BatchExecutionResult::Success(_)),
        "output={}",
        writer.text()
    );
    let result = success(result);

    let state = output_values(writer.text(), "DAMAGED_COMPUTER_STATE ");
    assert_eq!(state, [0, 0, 9710]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_phaser_uses_reduced_klingon_count_after_first_kill_in_same_volley() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 200\n\
LET BASES_HERE = 0\n\
LET STARS_HERE = 0\n\
LET KLINGONS_HERE = 2\n\
LET KLINGONS_LEFT = 2\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 1\n\
LET @KLINGON_X[2] = 3\n\
LET @KLINGON_Y[2] = 4\n\
LET @KLINGON_E[2] = 3000\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[29] = 2\n\
LET @SECTOR[27] = 2\n\
LET ENERGY = 1000\n\
LET SHIELDS = 10000\n\
LET DOCKED = 0\n\
LET @DAMAGE[4] = 0\n\
LET @DAMAGE[7] = 0\n\
PHASER\n\
PRINT \"VOLLEY_STATE \", @KLINGON_E[1], \" \", @KLINGON_E[2], \" \", KLINGONS_HERE, \" \", @GALAXY[1]\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("1000".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "VOLLEY_STATE ");
    // The second Klingon receives the full current-count divisor after the
    // first one is destroyed; keeping the initial count would leave 2265.
    assert_eq!(state, [0, 1530, 1, 100]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_shield_defeat_skips_phaser_damage_and_preserves_negative_state() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
LET BASES_HERE = 0\n\
LET STARS_HERE = 0\n\
LET KLINGONS_HERE = 1\n\
LET KLINGONS_LEFT = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 3000\n\
LET @SECTOR[29] = 2\n\
LET ENERGY = 1000\n\
LET SHIELDS = 1\n\
LET DOCKED = 0\n\
LET @DAMAGE[4] = 0\n\
LET @DAMAGE[7] = 0\n\
PHASER\n\
PRINT \"SHIELD_DEFEAT_STATE \", ENERGY, \" \", @KLINGON_E[1], \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("1000".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "SHIELD_DEFEAT_STATE ");
    assert_eq!(state[0], 0);
    assert_eq!(state[1], 3000);
    assert!(state[2] < 0);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_klingon_attack_accumulates_survivors_and_skips_destroyed_slots() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET SHIELDS = 1000\n\
LET DOCKED = 0\n\
LET KLINGONS_HERE = 2\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
LET @KLINGON_X[2] = 6\n\
LET @KLINGON_Y[2] = 4\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_X[3] = 3\n\
LET @KLINGON_Y[3] = 4\n\
LET @KLINGON_E[3] = 200\n\
KLINGON_ATTACK\n\
PRINT \"MULTI_ATTACK_STATE \", SHIELDS, \" \", @KLINGON_E[1], \" \", @KLINGON_E[2], \" \", @KLINGON_E[3]\n\
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

    let state = output_values(writer.text(), "MULTI_ATTACK_STATE ");
    assert!(state[0] < 1000);
    assert_eq!(state[1], 200);
    assert_eq!(state[2], 0);
    assert_eq!(state[3], 200);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_phaser_zero_cancel_preserves_state() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET KLINGONS_HERE = 1\n\
LET ENERGY = 777\n\
LET SHIELDS = 222\n\
LET @DAMAGE[4] = 0\n\
PHASER\n\
PRINT \"CANCEL_STATE \", ENERGY, \" \", SHIELDS\n\
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

    assert_eq!(output_values(writer.text(), "CANCEL_STATE "), [777, 222]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_docked_klingon_attack_preserves_state_and_rng() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET SHIELDS = 321\n\
LET DOCKED = 1\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 5\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
KLINGON_ATTACK\n\
PRINT \"DOCKED_STATE \", SHIELDS, \" \", @KLINGON_E[1], \" \", RND(200)\n\
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

    let docked_state = output_values(writer.text(), "DOCKED_STATE ");
    assert_eq!(docked_state[0..2], [321, 200]);

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
    let control_state = output_values(control_writer.text(), "DOCKED_STATE ");
    assert_eq!(docked_state[2], control_state[2]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_phaser_rejects_missing_target_and_damaged_control_without_state_changes() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENERGY = 777\n\
LET SHIELDS = 222\n\
LET KLINGONS_HERE = 0\n\
PHASER\n\
PRINT \"NO_TARGET_STATE \", ENERGY, \" \", SHIELDS\n\
CR\n\
LET KLINGONS_HERE = 1\n\
LET @DAMAGE[4] = -1\n\
PHASER\n\
PRINT \"DAMAGED_STATE \", ENERGY, \" \", SHIELDS\n\
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

    assert_eq!(output_values(writer.text(), "NO_TARGET_STATE "), [777, 222]);
    assert_eq!(output_values(writer.text(), "DAMAGED_STATE "), [777, 222]);
    assert!(writer.text().contains("NO KLINGONS IN THIS QUADRANT"));
    assert!(writer.text().contains("PHASER CONTROL IS NON-OPERATIONAL"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_destroy_klingon_keeps_sector_counts_and_galaxy_summary_consistent() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET BASES_HERE = 1\n\
LET STARS_HERE = 2\n\
LET KLINGONS_HERE = 2\n\
LET KLINGONS_LEFT = 5\n\
LET @KLINGON_X[1] = 2\n\
LET @KLINGON_Y[1] = 2\n\
LET @KLINGON_E[1] = 50\n\
LET @KLINGON_X[2] = 3\n\
LET @KLINGON_Y[2] = 3\n\
LET @KLINGON_E[2] = 50\n\
LET @SECTOR[10] = 2\n\
LET @SECTOR[19] = 2\n\
DESTROY_KLINGON(1)\n\
PRINT \"ONE_DESTROYED \", @KLINGON_E[1], \" \", @KLINGON_E[2], \" \", @SECTOR[10], \" \", @SECTOR[19], \" \", KLINGONS_HERE, \" \", KLINGONS_LEFT, \" \", @GALAXY[1]\n\
CR\n\
DESTROY_KLINGON(2)\n\
PRINT \"ALL_DESTROYED \", @KLINGON_E[1], \" \", @KLINGON_E[2], \" \", @SECTOR[10], \" \", @SECTOR[19], \" \", KLINGONS_HERE, \" \", KLINGONS_LEFT, \" \", @GALAXY[1]\n\
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
        output_values(writer.text(), "ONE_DESTROYED "),
        [0, 50, 0, 2, 1, 4, 112]
    );
    assert_eq!(
        output_values(writer.text(), "ALL_DESTROYED "),
        [0, 0, 0, 0, 0, 3, 12]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_photon_torpedo_rejects_gate_failures_and_cancel_without_consumption() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET TORPEDOES = 2\n\
LET @DAMAGE[5] = -1\n\
PHOTON_TORPEDO\n\
LET @DAMAGE[5] = 0\n\
LET TORPEDOES = 0\n\
PHOTON_TORPEDO\n\
LET TORPEDOES = 2\n\
LET KLINGONS_HERE = 0\n\
LET DOCKED = 1\n\
PHOTON_TORPEDO\n\
PRINT \"TORPEDO_GATE_STATE \", TORPEDOES\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("7".to_owned())), Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    assert_eq!(output_values(writer.text(), "TORPEDO_GATE_STATE "), [1]);
    assert!(writer.text().contains("PHOTON TUBES ARE DAMAGED"));
    assert!(writer.text().contains("NO PHOTON TORPEDOES LEFT"));
    assert!(writer.text().contains("COURSE OUT OF RANGE"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_photon_torpedo_hits_first_non_empty_klingon_and_updates_counts() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 100\n\
LET BASES_HERE = 0\n\
LET STARS_HERE = 0\n\
LET KLINGONS_HERE = 1\n\
LET KLINGONS_LEFT = 1\n\
LET @KLINGON_X[1] = 6\n\
LET @KLINGON_Y[1] = 4\n\
LET @KLINGON_E[1] = 200\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[30] = 2\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 500\n\
LET DOCKED = 0\n\
PHOTON_TORPEDO\n\
PRINT \"TORPEDO_KLINGON_STATE \", TORPEDOES, \" \", @KLINGON_E[1], \" \", @SECTOR[30], \" \", KLINGONS_HERE, \" \", KLINGONS_LEFT, \" \", @GALAXY[1], \" \", SHIELDS, \" \", RND(200)\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "TORPEDO_KLINGON_STATE ");
    assert_eq!(state[0..7], [1, 0, 0, 0, 0, 0, 500]);
    let control_source = source.replace("\nPHOTON_TORPEDO\n", "\nREM SKIP_TORPEDO\n");
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
    assert_eq!(
        state[7],
        output_values(control_writer.text(), "TORPEDO_KLINGON_STATE ")[7]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_photon_torpedo_hits_starbase_without_changing_other_objects() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 111\n\
LET BASES_HERE = 1\n\
LET BASES_LEFT = 1\n\
LET STARS_HERE = 1\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 8\n\
LET @KLINGON_Y[1] = 8\n\
LET @KLINGON_E[1] = 200\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[30] = 3\n\
LET @SECTOR[64] = 2\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 500\n\
LET DOCKED = 0\n\
PHOTON_TORPEDO\n\
PRINT \"TORPEDO_BASE_STATE \", TORPEDOES, \" \", @SECTOR[30], \" \", BASES_HERE, \" \", BASES_LEFT, \" \", @GALAXY[1], \" \", @SECTOR[64], \" \", @KLINGON_E[1], \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "TORPEDO_BASE_STATE ");
    assert_eq!(state[0..7], [1, 0, 0, 0, 101, 2, 200]);
    assert!(state[7] < 500);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_photon_torpedo_uses_interpolated_course_and_leaves_star_unchanged() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 1\n\
LET ENT_QY = 1\n\
LET ENT_SX = 4\n\
LET ENT_SY = 4\n\
LET @GALAXY[1] = 101\n\
LET BASES_HERE = 0\n\
LET STARS_HERE = 1\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 8\n\
LET @KLINGON_Y[1] = 8\n\
LET @KLINGON_E[1] = 200\n\
LET @KLINGON_E[2] = 0\n\
LET @KLINGON_E[3] = 0\n\
LET @SECTOR[22] = 4\n\
LET @SECTOR[64] = 2\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 1\n\
LET DOCKED = 0\n\
PHOTON_TORPEDO\n\
PRINT \"TORPEDO_STAR_STATE \", TORPEDOES, \" \", @SECTOR[22], \" \", STARS_HERE, \" \", @GALAXY[1], \" \", DIRECTION_X, \" \", DIRECTION_Y, \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("15".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "TORPEDO_STAR_STATE ");
    assert_eq!(state[0..6], [1, 4, 1, 101, 8, -3]);
    assert!(state[6] < 0);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_photon_torpedo_miss_calls_retaliation_and_does_not_cross_quadrant() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET ENT_QX = 8\n\
LET ENT_QY = 1\n\
LET ENT_SX = 8\n\
LET ENT_SY = 4\n\
LET @GALAXY[8] = 100\n\
LET @GALAXY[9] = 200\n\
LET KLINGONS_HERE = 1\n\
LET @KLINGON_X[1] = 1\n\
LET @KLINGON_Y[1] = 1\n\
LET @KLINGON_E[1] = 200\n\
LET @SECTOR[32] = 2\n\
LET TORPEDOES = 2\n\
LET SHIELDS = 500\n\
LET DOCKED = 0\n\
PHOTON_TORPEDO\n\
PRINT \"TORPEDO_MISS_STATE \", ENT_QX, \" \", ENT_QY, \" \", TORPEDOES, \" \", @GALAXY[8], \" \", @GALAXY[9], \" \", SHIELDS\n\
CR\n",
    );
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut input = TestInput::new([Ok(Some("10".to_owned()))]);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        30,
    ));

    let state = output_values(writer.text(), "TORPEDO_MISS_STATE ");
    assert_eq!(state[0..5], [8, 1, 1, 100, 200]);
    assert!(state[5] < 500);
    assert_eq!(result.data_stack(), []);
}
