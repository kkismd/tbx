use super::*;

fn example_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs")
        .join("next")
        .join("examples")
        .join(name)
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
