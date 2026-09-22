use super::*;
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
