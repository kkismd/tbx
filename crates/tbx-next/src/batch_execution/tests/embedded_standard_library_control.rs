use super::*;

#[test]
fn standard_library_source_word_is_available_to_the_user_source() {
    let standard_library =
        "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS";
    let source = "VAR COUNT\nLET COUNT = 0\nSLET COUNT = 7\nEVAL COUNT";
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(standard_library, source);
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(7)]);
}

#[test]
fn standard_library_runtime_definition_keeps_its_source_mapping() {
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library("DEF FAIL\nEVAL 1 / 0\nEND", "FAIL");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
    assert_eq!(
        failure
            .diagnostic()
            .primary()
            .map(|primary| primary.display_name()),
        Some("<tbx-next-stdlib>")
    );
}

#[test]
fn standard_library_block_source_word_and_marker_are_available_to_user_source() {
    let standard_library = "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS";
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(standard_library, "WRAP\nENDWRAP\nEVAL 9");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(9)]);
}

#[test]
fn standard_library_marker_reservation_rejects_user_binding_with_same_name() {
    let standard_library = "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS";
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library(standard_library, "DEF ENDWRAP\nEND");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
    assert!(failure
        .diagnostic()
        .primary()
        .is_some_and(|primary| primary.source_line() == "DEF ENDWRAP"));
}

#[test]
fn standard_library_marker_reservation_keeps_the_published_source_word_owner() {
    let standard_library = "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS";
    let mut sources = SourceTexts::new();
    let standard_library_id = sources.register(standard_library, "<tbx-next-stdlib>");
    let mut environment = BatchEnvironment::new().expect("batch environment should build");

    environment
        .compile(&sources, standard_library_id)
        .expect("standard library should compile");

    let owner = match environment.bindings.get(&name("WRAP")) {
        Some(Binding::SourceWord(owner)) => *owner,
        other => panic!("expected published source word, got {other:?}"),
    };
    assert_eq!(
        environment
            .bindings
            .syntax_marker_reservation(&name("ENDWRAP"))
            .map(|reservation| reservation.owner()),
        Some(owner)
    );
}

#[test]
fn standard_library_top_level_unit_is_not_executed() {
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library("EVAL 99\nPUTDEC\nCR", "EVAL 1\nPUTDEC\nCR");
    let mut writer = RecordingWriter::default();

    success(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(writer.text(), "1\n");
}

#[test]
fn standard_library_failure_short_circuits_user_source_and_is_environment_failure() {
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library("UNKNOWN", "EVAL 7\nPUTDEC");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert!(failure
        .diagnostic()
        .primary()
        .is_some_and(|primary| primary.display_name() == "<tbx-next-stdlib>"));
    assert_eq!(writer.text(), "");
}

#[test]
fn standard_library_lex_failure_short_circuits_user_source() {
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library("?", "EVAL 7\nPUTDEC");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert!(failure
        .diagnostic()
        .primary()
        .is_some_and(|primary| primary.display_name() == "<tbx-next-stdlib>"));
    assert_eq!(writer.text(), "");
}

#[test]
fn standard_library_publication_failure_short_circuits_user_source() {
    let (sources, standard_library_id, source_id) =
        sources_with_standard_library("SYNTAX A\nSTATEMENT\nENDS", "EVAL 7\nPUTDEC");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_sources(
        &sources,
        standard_library_id,
        source_id,
        &mut writer,
    ));

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert!(failure
        .diagnostic()
        .primary()
        .is_some_and(|primary| primary.display_name() == "<tbx-next-stdlib>"));
    assert_eq!(writer.text(), "");
}

#[test]
fn embedded_standard_library_test_entry_uses_the_production_source() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "EVAL 3 + 4",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(7)]);
}

#[test]
fn embedded_standard_library_while_repeats_until_condition_is_false() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nWHILE COUNT < 3\nLET COUNT = COUNT + 1\nENDWH\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(3)]);

    for (source, expected) in [
        ("VAR COUNT\nWHILE 0\nLET COUNT = 1\nENDWH\nEVAL 0", 0),
        (
            "VAR COUNT\nLET COUNT = 0\nWHILE COUNT < 1\nLET COUNT = COUNT + 1\nENDWH\nEVAL COUNT",
            1,
        ),
    ] {
        let mut writer = RecordingWriter::default();
        let result = success(execute_with_embedded_standard_library(
            source,
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(expected)]);
    }
}

#[test]
fn embedded_standard_library_break_exits_while_do_and_for_without_running_terminators() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nWHILE 1\nLET COUNT = COUNT + 1\nBREAK\nLET COUNT = COUNT + 100\nENDWH\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(1)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nDO\nLET COUNT = COUNT + 1\nBREAK\nLET COUNT = COUNT + 100\nUNTIL 1\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(1)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = 1 TO 3\nLET COUNT = COUNT + LOOP_INDEX\nBREAK\nLET COUNT = COUNT + 100\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(1), Value::integer(1)]);
}

#[test]
fn embedded_standard_library_break_transparently_exits_through_if_and_select() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nWHILE COUNT < 1\nIF 1\nBREAK\nENDIF\nLET COUNT = COUNT + 100\nENDWH\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(0)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR LOOP_INDEX\nFOR LOOP_INDEX = 1 TO 1\nSELECT LOOP_INDEX\nCASE 1\nBREAK\nENDSEL\nNEXT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(1)]);
}

#[test]
fn embedded_standard_library_break_cleans_nested_control_values() {
    let mut writer = RecordingWriter::default();
    let failure = failure(execute_with_embedded_standard_library(
        "VAR LOOP_INDEX\nSYNTAX COPY_CONTROL\nSTATEMENT\nEXPECT_END\nEMIT_CONTROL_COPY\nENDS\nFOR LOOP_INDEX = 1 TO 1\nSELECT LOOP_INDEX\nCASE 1\nIF 1\nBREAK\nENDIF\nENDSEL\nNEXT\nCOPY_CONTROL",
        "program.tbx",
        &mut writer,
    ));
    let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
        panic!("control-value cleanup probe should fail in user runtime code");
    };
    let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
        panic!("cleanup probe should preserve the runtime error");
    };
    assert!(matches!(
        error.vm().kind(),
        crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
    ));

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR INNER_COUNT\nLET COUNT = 0\nWHILE COUNT < 2\nLET INNER_COUNT = 0\nWHILE INNER_COUNT < 1\nBREAK\nENDWH\nLET COUNT = COUNT + 1\nENDWH\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(2)]);
}

#[test]
fn embedded_standard_library_break_requires_a_loop_and_rejects_trailing_tokens() {
    for (source, expected_column) in [("BREAK", 1), ("BREAK X", 7)] {
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_with_embedded_standard_library(
            source,
            "program.tbx",
            &mut writer,
        ));
        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        let primary = failure
            .diagnostic
            .primary()
            .expect("BREAK diagnostic should have a primary span");
        assert_eq!(primary.display_name(), "program.tbx");
        assert_eq!(primary.line_number(), 1);
        assert_eq!(primary.column_number(), expected_column);
    }

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "DEF LOOP_BREAK\nWHILE 1\nBREAK\nENDWH\nEND\nWHILE 1\nLOOP_BREAK\nBREAK\nENDWH\nEVAL 1",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(1)]);

    let mut writer = RecordingWriter::default();
    let failure = failure(execute_with_embedded_standard_library(
        "DEF INVALID_BREAK\nBREAK\nEND",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
}

#[test]
fn embedded_standard_library_while_has_one_line_number_scope_across_body() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nWHILE COUNT < 1\nBIF 0, 20\n10 LET COUNT = COUNT + 1\n20 LET COUNT = COUNT + 1\nENDWH\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(1)]);
}

#[test]
fn line_number_on_structured_start_statement_belongs_to_enclosing_scope() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nBIF 0, 100\n100 WHILE 0\nLET COUNT = 1\nENDWH\nEVAL 7",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(7)]);
}

#[test]
fn structured_body_keeps_enclosing_publication_capability() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "WHILE 0\nVAR SCORE\nENDWH\nLET SCORE = 3\nEVAL SCORE",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(3)]);
}

#[test]
fn structured_marker_cannot_become_a_line_number_target() {
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_with_embedded_standard_library(
        "WHILE 1\n10 ENDWH",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
}

#[test]
fn embedded_standard_library_do_runs_once_and_repeats_until_condition_is_true() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nDO\nLET COUNT = COUNT + 1\nUNTIL COUNT >= 3\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(3)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nDO\nLET COUNT = COUNT + 1\nUNTIL COUNT >= 1\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(1)]);
}

#[test]
fn embedded_standard_library_for_repeats_with_a_fixed_end_value() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = 1 TO 3\nLET COUNT = COUNT + LOOP_INDEX\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(6), Value::integer(4)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR INNER_COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nLET INNER_COUNT = 3\nFOR LOOP_INDEX = 1 TO INNER_COUNT\nLET COUNT = COUNT + 1\nLET INNER_COUNT = 1\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(3), Value::integer(4)]);
}

#[test]
fn embedded_standard_library_for_evaluates_bounds_in_order_once_and_can_skip_body() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = COUNT + 1 TO COUNT + 2\nLET COUNT = COUNT + 10\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(20), Value::integer(3)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = 3 TO 1\nLET COUNT = COUNT + 1\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(0), Value::integer(3)]);
}

#[test]
fn embedded_standard_library_for_evaluates_side_effecting_bounds_once_in_order() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nDEF START_BOUND\nLET COUNT = COUNT + 1\nEVAL COUNT\nEND\nDEF END_BOUND\nLET COUNT = COUNT + 10\nEVAL COUNT\nEND\nFOR LOOP_INDEX = START_BOUND() TO END_BOUND()\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(
        result.data_stack(),
        [Value::integer(11), Value::integer(12)]
    );
}

#[test]
fn embedded_standard_library_for_supports_nested_for_loops() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nVAR INNER_LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = 1 TO 2\nFOR INNER_LOOP_INDEX = 1 TO 3\nLET COUNT = COUNT + 1\nNEXT\nNEXT\nEVAL COUNT\nEVAL LOOP_INDEX\nEVAL INNER_LOOP_INDEX",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(
        result.data_stack(),
        [Value::integer(6), Value::integer(3), Value::integer(4)]
    );
}

#[test]
fn embedded_standard_library_for_uses_the_modified_counter_and_preserves_body_stack_values() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = 1 TO 3\nEVAL LOOP_INDEX\nLET LOOP_INDEX = LOOP_INDEX + 1\nNEXT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(1), Value::integer(3)]);
}

#[test]
fn embedded_standard_library_for_supports_nested_structured_blocks_and_case_insensitive_to() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR LOOP_INDEX\nLET COUNT = 0\nFOR LOOP_INDEX = 1 to 2\nIF 1\nWHILE COUNT < 1\nDO\nLET COUNT = COUNT + 1\nUNTIL COUNT >= 1\nENDWH\nENDIF\nSELECT LOOP_INDEX\nCASE 1\nLET COUNT = COUNT + 10\nCASE 2\nLET COUNT = COUNT + 100\nENDSEL\nNEXT\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(111)]);
}

#[test]
fn embedded_standard_library_for_rejects_non_variable_bindings_and_cleans_up_control_value() {
    let mut writer = RecordingWriter::default();
    let binding_failure = failure(execute_with_embedded_standard_library(
        "FOR MISSING = 1 TO 2\nNEXT",
        "program.tbx",
        &mut writer,
    ));
    assert!(matches!(
        binding_failure.cause,
        BatchExecutionFailureCause::Source(_)
    ));

    let mut writer = RecordingWriter::default();
    let non_variable_failure = failure(execute_with_embedded_standard_library(
        "FOR ADD = 1 TO 2\nNEXT",
        "program.tbx",
        &mut writer,
    ));
    assert!(matches!(
        non_variable_failure.cause,
        BatchExecutionFailureCause::Source(_)
    ));

    let mut writer = RecordingWriter::default();
    let failure = failure(execute_with_embedded_standard_library(
        "VAR LOOP_INDEX\nSYNTAX COPY_CONTROL\nSTATEMENT\nEXPECT_END\nEMIT_CONTROL_COPY\nENDS\nDEF CHECK\nFOR LOOP_INDEX = 1 TO 1\nNEXT\nCOPY_CONTROL\nEND\nCHECK",
        "program.tbx",
        &mut writer,
    ));
    let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
        panic!("control-value cleanup probe should fail in user runtime code");
    };
    let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
        panic!("cleanup probe should preserve the runtime error");
    };
    assert!(matches!(
        error.vm().kind(),
        crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
    ));
}

#[test]
fn embedded_standard_library_select_matches_cases_without_fallthrough() {
    for (selector, expected) in [(1, 10), (2, 20), (3, 30), (9, 0)] {
        let mut writer = RecordingWriter::default();
        let source = format!(
            "SELECT {selector}\nCASE 1\nEVAL 10\nCASE 2\nEVAL 20\nCASE 3\nEVAL 30\nENDSEL\nEVAL 0"
        );
        let result = success(execute_with_embedded_standard_library(
            &source,
            "program.tbx",
            &mut writer,
        ));

        let expected_stack = if selector == 9 {
            vec![Value::integer(0)]
        } else {
            vec![Value::integer(expected), Value::integer(0)]
        };
        assert_eq!(result.data_stack(), expected_stack);
    }

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "SELECT 9\nCASE 1\nEVAL 10\nCASE_ELSE\nEVAL 99\nENDSEL",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(99)]);
}

#[test]
fn embedded_standard_library_select_evaluates_selector_once_and_supports_nesting() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nDEF INC\nLET COUNT = COUNT + 1\nEVAL COUNT\nEND\nLET COUNT = 0\nSELECT INC()\nCASE 1\nSELECT 2\nCASE 2\nEVAL 7\nENDSEL\nENDSEL\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));
    assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(1)]);
}

#[test]
fn embedded_standard_library_select_cleans_up_selector_before_following_runtime_code() {
    let mut writer = RecordingWriter::default();
    let failure = failure(execute_with_embedded_standard_library(
        "SYNTAX COPY_CONTROL\nSTATEMENT\nEXPECT_END\nEMIT_CONTROL_COPY\nENDS\nDEF CHECK\nSELECT 1\nCASE 1\nEVAL 7\nENDSEL\nCOPY_CONTROL\nEND\nCHECK",
        "program.tbx",
        &mut writer,
    ));

    let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
        panic!("control-value cleanup probe should fail in user runtime code");
    };
    let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
        panic!("cleanup probe should preserve the runtime error");
    };
    assert!(matches!(
        error.vm().kind(),
        crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
    ));
}

#[test]
fn embedded_standard_library_select_nests_inside_if_while_and_do() {
    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nLET COUNT = 0\nIF 1\nSELECT 1\nCASE 1\nEVAL 10\nENDSEL\nENDIF\nWHILE COUNT < 1\nSELECT 2\nCASE 2\nEVAL 20\nENDSEL\nLET COUNT = COUNT + 1\nENDWH\nDO\nSELECT 3\nCASE 3\nEVAL 30\nENDSEL\nUNTIL 1\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(
        result.data_stack(),
        [
            Value::integer(10),
            Value::integer(20),
            Value::integer(30),
            Value::integer(1)
        ]
    );
}

#[test]
fn embedded_standard_library_select_contains_if_while_and_do_bodies() {
    for (selector, body, expected) in [
        (1, "IF 1\nEVAL 10\nENDIF", 10),
        (
            2,
            "VAR COUNT\nLET COUNT = 0\nWHILE COUNT < 1\nEVAL 20\nLET COUNT = COUNT + 1\nENDWH",
            20,
        ),
        (3, "DO\nEVAL 30\nUNTIL 1", 30),
    ] {
        let mut writer = RecordingWriter::default();
        let source = format!(
            "SELECT {selector}\nCASE 1\n{}\nCASE 2\n{}\nCASE 3\n{}\nCASE_ELSE\nEVAL 96\nENDSEL",
            if selector == 1 { body } else { "EVAL 99" },
            if selector == 2 { body } else { "EVAL 98" },
            if selector == 3 { body } else { "EVAL 97" },
        );
        let result = success(execute_with_embedded_standard_library(
            &source,
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(result.data_stack(), [Value::integer(expected)]);
    }
}

#[test]
fn embedded_standard_library_select_requires_cases_and_orders_else_last() {
    for source in [
        "SELECT 1\nENDSEL",
        "SELECT 1\nCASE_ELSE\nENDSEL",
        "SELECT 1\nCASE_ELSE\nCASE 1\nENDSEL",
        "SELECT 1\nCASE 1\nCASE_ELSE\nCASE_ELSE\nENDSEL",
    ] {
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_with_embedded_standard_library(
            source,
            "program.tbx",
            &mut writer,
        ));
        assert!(matches!(
            failure.cause,
            BatchExecutionFailureCause::Source(_)
        ));
    }
}

#[test]
fn embedded_standard_library_control_structures_support_nested_and_native_if_blocks() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR INNER_COUNT\nLET COUNT = 0\nLET INNER_COUNT = 0\nIF 1\nWHILE COUNT < 2\nDO\nLET INNER_COUNT = INNER_COUNT + 1\nUNTIL INNER_COUNT >= 2\nLET COUNT = COUNT + 1\nENDWH\nENDIF\nEVAL COUNT\nEVAL INNER_COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(3)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR INNER_COUNT\nLET COUNT = 0\nDO\nLET INNER_COUNT = 0\nWHILE INNER_COUNT < 2\nLET INNER_COUNT = INNER_COUNT + 1\nENDWH\nLET COUNT = COUNT + 1\nUNTIL COUNT >= 2\nEVAL COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(2)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR INNER_COUNT\nLET COUNT = 0\nLET INNER_COUNT = 0\nWHILE COUNT < 2\nLET INNER_COUNT = 0\nWHILE INNER_COUNT < 2\nLET INNER_COUNT = INNER_COUNT + 1\nENDWH\nLET COUNT = COUNT + 1\nENDWH\nEVAL COUNT\nEVAL INNER_COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(2)]);

    let mut writer = RecordingWriter::default();
    let result = success(execute_with_embedded_standard_library(
        "VAR COUNT\nVAR INNER_COUNT\nLET COUNT = 0\nLET INNER_COUNT = 0\nDO\nLET INNER_COUNT = 0\nDO\nLET INNER_COUNT = INNER_COUNT + 1\nUNTIL INNER_COUNT >= 2\nLET COUNT = COUNT + 1\nUNTIL COUNT >= 2\nEVAL COUNT\nEVAL INNER_COUNT",
        "program.tbx",
        &mut writer,
    ));

    assert_eq!(result.data_stack(), [Value::integer(2), Value::integer(2)]);
}

#[test]
fn embedded_standard_library_control_structure_markers_are_reserved_by_their_owner() {
    let mut sources = SourceTexts::new();
    let stdlib_source_id = register_embedded_standard_library(&mut sources);
    let mut environment = BatchEnvironment::new().expect("batch environment should build");

    environment
        .compile(&sources, stdlib_source_id)
        .expect("embedded standard library should compile");

    let Some(Binding::SourceWord(while_id)) = environment.bindings.get(&name("WHILE")) else {
        panic!("WHILE should publish as a source word");
    };
    let Some(Binding::SourceWord(do_id)) = environment.bindings.get(&name("DO")) else {
        panic!("DO should publish as a source word");
    };
    let Some(Binding::SourceWord(if_id)) = environment.bindings.get(&name("IF")) else {
        panic!("IF should publish as a source word from the embedded stdlib");
    };
    for (source_name, expected_exit_target, expected_ownership) in [
        ("IF", false, 0),
        ("WHILE", true, 0),
        ("DO", true, 0),
        ("FOR", true, 1),
        ("SELECT", false, 1),
    ] {
        let Some(Binding::SourceWord(id)) = environment.bindings.get(&name(source_name)).copied()
        else {
            panic!("{source_name} should publish as a source word");
        };
        let crate::source_word::SourceWordDispatch::Structured {
            implementation:
                crate::source_word::StructuredSourceWordDispatch::UserDefined(implementation),
            ..
        } = environment
            .source_words
            .lookup()
            .lookup_dispatch(id)
            .expect("stdlib source word should dispatch")
        else {
            panic!("{source_name} should use a user-defined structured implementation");
        };
        assert_eq!(
            implementation.exit_target(),
            expected_exit_target,
            "{source_name}"
        );
        assert_eq!(
            implementation.control_value_ownership(),
            expected_ownership,
            "{source_name}"
        );
    }
    assert_eq!(
        environment
            .bindings
            .syntax_marker_reservation(&name("ENDWH"))
            .map(|reservation| reservation.owner()),
        Some(*while_id)
    );
    assert_eq!(
        environment
            .bindings
            .syntax_marker_reservation(&name("UNTIL"))
            .map(|reservation| reservation.owner()),
        Some(*do_id)
    );
    for marker_name in ["ELSIF", "ELSE", "ENDIF"] {
        assert_eq!(
            environment
                .bindings
                .syntax_marker_reservation(&name(marker_name))
                .map(|reservation| reservation.owner()),
            Some(*if_id),
            "{marker_name} should be owned by IF"
        );
    }
    assert!(STDLIB_SOURCE.contains("SYNTAX WHILE"));
    assert!(STDLIB_SOURCE.contains("SYNTAX DO"));
    assert!(STDLIB_SOURCE.contains("SYNTAX IF"));
}

#[test]
fn embedded_standard_library_control_structure_markers_reject_binding_and_owner_mismatch() {
    for source in [
        "DEF ENDWH\nEND",
        "WHILE 1\nWEND",
        "WHILE 1\nUNTIL 1",
        "DO\nENDWH",
        "WHILE 1",
        "DO",
    ] {
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_with_embedded_standard_library(
            source,
            "program.tbx",
            &mut writer,
        ));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert!(failure.diagnostic().primary().is_some());
    }
}

#[test]
fn embedded_standard_library_allows_wend_as_a_regular_word_name() {
    let mut writer = RecordingWriter::default();

    let result = success(execute_with_embedded_standard_library(
        "DEF WEND\nEND",
        "program.tbx",
        &mut writer,
    ));

    assert!(result.data_stack().is_empty());
}
