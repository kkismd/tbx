use super::*;
use std::collections::HashSet;

#[test]
fn sttr1_quadrant_setup_places_unique_sector_objects_and_klingon_state() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
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

    let quadrant = output_values(writer.text(), "QUADRANT ");
    let short_scan = writer
        .text()
        .lines()
        .skip_while(|line| *line != "SHORT RANGE SCAN")
        .skip(1)
        .take(8)
        .collect::<Vec<_>>();
    assert_eq!(short_scan.len(), 8);
    assert!(short_scan.iter().all(|line| line.chars().count() == 8));
    let counts = short_scan.iter().flat_map(|line| line.chars()).fold(
        (0, 0, 0, 0, 0),
        |(empty, player, klingon, base, star), cell| match cell {
            '.' => (empty + 1, player, klingon, base, star),
            'P' => (empty, player + 1, klingon, base, star),
            'K' => (empty, player, klingon + 1, base, star),
            'B' => (empty, player, klingon, base + 1, star),
            '*' => (empty, player, klingon, base, star + 1),
            _ => panic!("unexpected sector cell {cell:?}"),
        },
    );
    assert_eq!(counts.1, 1);
    assert_eq!(counts.2, quadrant[2]);
    assert_eq!(counts.3, quadrant[3]);
    assert_eq!(counts.4, quadrant[4]);
    assert_eq!(counts.0 + counts.1 + counts.2 + counts.3 + counts.4, 64);

    let scanned_klingons = short_scan
        .iter()
        .enumerate()
        .flat_map(|(y, line)| {
            line.chars()
                .enumerate()
                .filter(|(_, cell)| *cell == 'K')
                .map(move |(x, _)| (x + 1, y + 1))
        })
        .collect::<HashSet<_>>();
    assert_eq!(scanned_klingons.len(), quadrant[2] as usize);

    let klingon_state = output_values(writer.text(), "KLINGON_STATE ");
    assert_eq!(klingon_state.len(), 9);
    assert_eq!(klingon_state.chunks_exact(3).len(), 3);
    let mut state_klingons = HashSet::new();
    for (index, state) in klingon_state.chunks_exact(3).enumerate() {
        if index < quadrant[2] as usize {
            assert!((1..=8).contains(&state[0]));
            assert!((1..=8).contains(&state[1]));
            assert_eq!(state[2], 200);
            assert!(state_klingons.insert((state[0] as usize, state[1] as usize)));
        } else {
            assert_eq!(state, [0, 0, 0]);
        }
    }
    assert_eq!(state_klingons, scanned_klingons);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_scan_commands_respect_sensor_and_computer_damage_gates() {
    let mut source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    source.push_str(
        "LET @DAMAGE[2] = -1\nPRINT_SHORT_SCAN\nLET @DAMAGE[3] = -1\nPRINT_LONG_SCAN\nLET SECTOR_INDEX = 1\nWHILE SECTOR_INDEX <= 64\nLET @CHART[SECTOR_INDEX] = 0\nLET SECTOR_INDEX = SECTOR_INDEX + 1\nENDWH\nLET @DAMAGE[2] = 0\nLET @DAMAGE[3] = 0\nLET @DAMAGE[7] = -1\nPRINT_LONG_SCAN\nPRINT \"CHART_AFTER \"\nLET SECTOR_INDEX = 1\nWHILE SECTOR_INDEX <= 64\nPRINT @CHART[SECTOR_INDEX], \" \"\nLET SECTOR_INDEX = SECTOR_INDEX + 1\nENDWH\nCR\n",
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
    let output = writer.text();
    assert!(output.contains("SHORT RANGE SENSORS INOPERABLE"));
    assert!(output.contains("LONG RANGE SENSORS INOPERABLE"));
    let long_scan_count = output.matches("LONG RANGE SCAN\n").count();
    assert_eq!(long_scan_count, 2);
    let chart = output_values(output, "CHART_AFTER ");
    assert!(chart.iter().all(|value| *value == 0));
    assert_eq!(result.data_stack(), []);
}
