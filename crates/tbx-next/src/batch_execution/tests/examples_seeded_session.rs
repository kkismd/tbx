use super::*;

fn example_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs")
        .join("next")
        .join("examples")
        .join(name)
}

fn output_values(output: &str, label: &str) -> Vec<i16> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .unwrap_or_else(|| panic!("missing {label:?} output line"))
        .split_whitespace()
        .map(|value| value.parse().expect("state output should contain integers"))
        .collect()
}

#[test]
fn seeded_processing_session_continues_rnd_across_top_level_forms() {
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, "PUTDEC RND(10)\nPUTDEC RND(10)");
    let mut first_output = RecordingWriter::default();
    let first = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut first_output,
        None,
        123,
    ));

    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, "PUTDEC RND(10)\nPUTDEC RND(10)");
    let mut second_output = RecordingWriter::default();
    let second = success(execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut second_output,
        None,
        123,
    ));

    let mut expected_random = RandomState::seeded(123);
    let expected_output = format!(
        "{}{}",
        expected_random
            .next_inclusive(10)
            .expect("positive bound should succeed"),
        expected_random
            .next_inclusive(10)
            .expect("positive bound should succeed")
    );
    assert_eq!(first_output.text(), second_output.text());
    assert_eq!(first_output.text(), expected_output);
    assert_eq!(first.data_stack(), second.data_stack());
}

#[test]
fn sttr1_initialization_builds_consistent_mission_state() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 initialization example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
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
        [1, 1, 0, -1, -1, -1, 0, 1, 1]
    );
    assert_eq!(
        output_values(writer.text(), "COURSE_DY "),
        [0, -1, -1, -1, 0, 1, 1, 1, 0]
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn sttr1_quadrant_setup_places_unique_sector_objects_and_klingon_state() {
    let source = std::fs::read_to_string(example_path("sttr1/main.tbx"))
        .expect("STTR1 example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
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

    let klingon_state = output_values(writer.text(), "KLINGON_STATE ");
    assert_eq!(klingon_state.len(), 9);
    assert_eq!(klingon_state.chunks_exact(3).len(), 3);
    for (index, state) in klingon_state.chunks_exact(3).enumerate() {
        if index < quadrant[2] as usize {
            assert!((1..=8).contains(&state[0]));
            assert!((1..=8).contains(&state[1]));
            assert_eq!(state[2], 200);
        } else {
            assert_eq!(state, [0, 0, 0]);
        }
    }
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
        sources_with_standard_library(STDLIB_SOURCE, &source);
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

#[test]
fn guess_example_covers_ordering_branches_with_one_generated_answer() {
    let source = std::fs::read_to_string(example_path("guess.tbx"))
        .expect("guess example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut expected_random = RandomState::seeded(123);
    let answer = expected_random
        .next_inclusive(100)
        .expect("the sample uses a positive random bound");
    assert!((2..=99).contains(&answer));
    let mut input = TestInput::new([
        Ok(Some("not a number".to_owned())),
        Ok(Some((answer - 1).to_string())),
        Ok(Some((answer + 1).to_string())),
        Ok(Some(answer.to_string())),
    ]);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        123,
    );

    let result = success(result);
    assert_eq!(
        writer.text(),
        "Guess a number from 1 to 100: Please enter a number.\n\
Guess a number from 1 to 100: Too low.\n\
Guess a number from 1 to 100: Too high.\n\
Guess a number from 1 to 100: Correct!\n"
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn guess_example_keeps_answer_after_invalid_input() {
    let source = std::fs::read_to_string(example_path("guess.tbx"))
        .expect("guess example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut expected_random = RandomState::seeded(456);
    let answer = expected_random
        .next_inclusive(100)
        .expect("the sample uses a positive random bound");
    let mut input = TestInput::new([
        Ok(Some("not a number".to_owned())),
        Ok(Some(answer.to_string())),
    ]);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        Some(&mut input),
        456,
    );

    let result = success(result);
    assert_eq!(
        writer.text(),
        "Guess a number from 1 to 100: Please enter a number.\n\
Guess a number from 1 to 100: Correct!\n"
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn prime_example_leaves_the_data_stack_empty() {
    let source = std::fs::read_to_string(example_path("prime.tbx"))
        .expect("prime example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn grades_example_leaves_the_data_stack_empty() {
    let source = std::fs::read_to_string(example_path("grades.tbx"))
        .expect("grades example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn mandelbrot_example_leaves_the_data_stack_empty() {
    let source = std::fs::read_to_string(example_path("mandelbrot.tbx"))
        .expect("Mandelbrot example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn squares_example_leaves_the_data_stack_empty() {
    let source = std::fs::read_to_string(example_path("squares.tbx"))
        .expect("squares example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn eightqueen_example_leaves_the_data_stack_empty() {
    let source = std::fs::read_to_string(example_path("eightqueen.tbx"))
        .expect("eightqueen example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(writer.text(), "92\n");
    assert_eq!(result.data_stack(), []);
}

#[test]
fn maze_example_leaves_the_data_stack_empty() {
    let source =
        std::fs::read_to_string(example_path("maze.tbx")).expect("maze example should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(
        writer.text(),
        "########\n#S***G##\n#+######\n#++#####\n########\nMAZE SOLVED\n"
    );
    assert_eq!(result.data_stack(), []);
}

#[test]
fn unreachable_maze_exhausts_the_search_stack_without_runtime_failure() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maze_unreachable.tbx"),
    )
    .expect("unreachable maze fixture should be readable");
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(STDLIB_SOURCE, &source);
    let mut writer = RecordingWriter::default();

    let result = execute_registered_sources_with_filesystem_and_seed(
        sources,
        standard_library_id,
        source_id,
        &mut writer,
        None,
        123,
    );

    let result = success(result);
    assert_eq!(
        writer.text(),
        "#####\n#S#G#\n#+###\n#++##\n#####\nNO PATH\n"
    );
    assert_eq!(result.data_stack(), []);
}
