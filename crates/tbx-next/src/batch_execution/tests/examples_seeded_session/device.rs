use super::*;

fn device_source(
    source: &str,
) -> (
    SourceTexts,
    crate::source::SourceId,
    crate::source::SourceId,
) {
    sttr1_sources_with_standard_library(
        STDLIB_SOURCE,
        &format!("USE \"state.tbx\"\nUSE \"device.tbx\"\n{source}"),
    )
}

#[test]
fn sttr1_device_registry_reports_all_eight_slots() {
    let (sources, standard_library_id, source_id) =
        device_source("PACK @DAMAGE = -1, -2, -3, -4, -5, 0, -7, -8\nDAMAGE_CONTROL\n");
    let mut writer = RecordingWriter::default();
    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        30,
    ));

    let output = writer.text();
    for name in [
        "WARP ENGINES -1",
        "SHORT RANGE SENSORS -2",
        "LONG RANGE SENSORS -3",
        "PHASER CONTROL -4",
        "PHOTON TUBES -5",
        "DAMAGE CONTROL 0",
        "SHIELD CONTROL -7",
        "LIBRARY COMPUTER -8",
    ] {
        assert!(output.contains(name), "missing {name:?} in {output}");
    }
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_damaged_control_rejects_damage_report() {
    let (sources, standard_library_id, source_id) =
        device_source("LET @DAMAGE[6] = -1\nDAMAGE_CONTROL\n");
    let mut writer = RecordingWriter::default();
    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        30,
    ));

    assert!(writer.text().contains("DAMAGE CONTROL IS NON-OPERATIONAL"));
    assert!(!writer.text().contains("DAMAGE REPORT"));
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_repair_tick_only_advances_negative_damage() {
    let (sources, standard_library_id, source_id) = device_source(
        "PACK @DAMAGE = -1, -2, 0, 3, -5, 0, 2, -8\nREPAIR_DEVICES\nPRINT \"REPAIRED \"\nLET DEVICE_INDEX = 1\nWHILE DEVICE_INDEX <= 8\nPRINT @DAMAGE[DEVICE_INDEX], \" \"\nLET DEVICE_INDEX = DEVICE_INDEX + 1\nENDWH\nCR\n",
    );
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
        output_values(writer.text(), "REPAIRED "),
        [0, -1, 0, 3, -4, 0, 2, -7]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_random_device_event_changes_one_slot_without_clamping() {
    let (sources, standard_library_id, source_id) = device_source(
        "LET @DAMAGE[1] = -1\nLET @DAMAGE[2] = 0\nLET @DAMAGE[3] = 0\nLET @DAMAGE[4] = 0\nLET @DAMAGE[5] = 0\nLET @DAMAGE[6] = 0\nLET @DAMAGE[7] = 0\nLET @DAMAGE[8] = 0\nRANDOM_DEVICE_EVENT\nPRINT \"EVENT \"\nLET DEVICE_INDEX = 1\nWHILE DEVICE_INDEX <= 8\nPRINT @DAMAGE[DEVICE_INDEX], \" \"\nLET DEVICE_INDEX = DEVICE_INDEX + 1\nENDWH\nCR\n",
    );
    let mut writer = RecordingWriter::default();
    let result = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        1,
    ));

    let values = output_values(writer.text(), "EVENT ");
    assert_eq!(values, [-1, 0, 0, 0, 0, 4, 0, 0]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_random_device_event_can_worsen_a_positive_slot() {
    let mut found_worsening = false;
    for seed in 1..=100 {
        let (sources, standard_library_id, source_id) = device_source(
            "PACK @DAMAGE = 10, 10, 10, 10, 10, 10, 10, 10\nRANDOM_DEVICE_EVENT\nPRINT \"EVENT \"\nLET DEVICE_INDEX = 1\nWHILE DEVICE_INDEX <= 8\nPRINT @DAMAGE[DEVICE_INDEX], \" \"\nLET DEVICE_INDEX = DEVICE_INDEX + 1\nENDWH\nCR\n",
        );
        let mut writer = RecordingWriter::default();
        let result = success(execute_registered_sources_with_filesystem_and_seed(
            sources,
            standard_library_id,
            source_id,
            &mut writer,
            None,
            seed,
        ));
        let values = output_values(writer.text(), "EVENT ");
        assert_eq!(result.data_stack(), []);
        if values.iter().any(|value| (5..10).contains(value)) {
            found_worsening = true;
            break;
        }
    }
    assert!(
        found_worsening,
        "seed range should include a worsening event"
    );
}
