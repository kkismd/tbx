use super::*;
#[test]
fn sttr1_initialization_builds_consistent_mission_state() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 initialization example should be readable");
    let (sources, standard_library_id, source_id) =
        sttr1_sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        30,
    );

    let result = success(result);
    let mission = output_values(writer.text(), "MISSION ");
    assert_eq!(mission.len(), 14);
    assert!((2000..=3900).contains(&mission[0]));
    assert_eq!(mission[0] % 100, 0);
    assert_eq!(mission[1], mission[0]);
    assert_eq!(mission[2], 30);
    assert_eq!(mission[3], mission[0] + mission[2]);
    assert_eq!(&mission[4..10], &[3000, 3000, 10, 10, 0, 200]);
    assert!(mission[10..14].iter().all(|value| (1..=8).contains(value)));

    let totals = output_values(writer.text(), "TOTALS ");
    assert_eq!(totals.len(), 4);
    assert!(totals[0] > 0);
    assert_eq!(totals[1], totals[0]);
    assert!(totals[2] > 0);
    assert_eq!(totals[3], 2);

    let galaxy = output_values(writer.text(), "GALAXY ");
    assert_eq!(galaxy.len(), 64);
    let (mut klingons, mut bases) = (0, 0);
    for summary in galaxy {
        let quadrant_klingons = summary / 100;
        let quadrant_bases = summary / 10 % 10;
        let quadrant_stars = summary % 10;
        assert!((0..=3).contains(&quadrant_klingons));
        assert!((0..=1).contains(&quadrant_bases));
        assert!((1..=8).contains(&quadrant_stars));
        klingons += quadrant_klingons;
        bases += quadrant_bases;
    }
    assert_eq!(totals[0], klingons);
    assert_eq!(totals[2], bases);

    let chart = output_values(writer.text(), "CHART ");
    assert_eq!(chart.len(), 64);
    let observed = chart.iter().filter(|value| **value != 0).count();
    assert!((4..=9).contains(&observed));
    assert_eq!(chart[40], 8);
    assert_eq!(chart[41], 4);
    assert_eq!(chart[48], 302);
    assert_eq!(chart[49], 7);
    assert_eq!(chart[56], 1);
    assert_eq!(chart[57], 5);
    assert_eq!(output_values(writer.text(), "DAMAGE "), vec![0; 8]);
    assert_eq!(
        output_values(writer.text(), "COURSE_DX "),
        [10, 7, 0, -7, -10, -7, 0, 7, 10]
    );
    assert_eq!(
        output_values(writer.text(), "COURSE_DY "),
        [0, -7, -10, -7, 0, 7, 10, 7, 0]
    );
    assert_eq!(result.data_stack(), []);
}
