use super::*;

#[test]
fn registered_file_and_stdin_sources_share_the_same_execution_path() {
    for display_name in ["program.tbx", "<stdin>"] {
        let (sources, source_id) = source("EVAL 2 + 3\nEVAL ADD(4, 5)", display_name);
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(5), Value::integer(9)]);
        assert_eq!(writer.text(), "");
    }
}

#[test]
fn m20_environment_supports_variables_definitions_stack_words_and_output() {
    let text = "VAR VALUE\nLET VALUE = 4\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(VALUE)\nPUTDEC\nCR";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(4)]);
    assert_eq!(writer.text(), "4\n");
}

#[test]
fn single_letter_global_works_after_an_explicit_batch_declaration() {
    let (sources, source_id) = source("VAR I\nLET I = 1\nEVAL I", "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(1)]);
    assert_eq!(writer.text(), "");
}

#[test]
fn global_arrays_support_expression_reads_and_indexed_writes() {
    let text = "DIM @DATA[4]\nVAR INDEX\nLET INDEX = 2\nLET @DATA[1] = 7\nLET @DATA[INDEX + 1] = @DATA[1] + 5\nEVAL @DATA[1]\nEVAL @DATA[(INDEX + 1)]\nLET @DATA[4] = 9\nEVAL @DATA[4]";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(7), Value::integer(12), Value::integer(9)]
    );
}

#[test]
fn pack_stores_expression_values_in_array_order() {
    let text = "DIM @DATA[3]\nPACK @DATA = 10, 20, 30\nEVAL @DATA[1]\nEVAL @DATA[2]\nEVAL @DATA[3]";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(10), Value::integer(20), Value::integer(30)]
    );
}

#[test]
fn pack_consumes_only_the_array_sized_stack_suffix() {
    let text = "DIM @DATA[2]\nEVAL 99\nPACK @DATA = 10, 20\nEVAL @DATA[1]\nEVAL @DATA[2]";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(99), Value::integer(10), Value::integer(20)]
    );
}

#[test]
fn pack_accepts_general_expressions_and_length_one_arrays() {
    let text = "VAR LEFT_VALUE\nVAR RIGHT_VALUE\nLET LEFT_VALUE = 2\nLET RIGHT_VALUE = 3\nDIM @DATA[3]\nPACK @DATA = LEFT_VALUE + 1, RIGHT_VALUE * 2, ABS(LEFT_VALUE)\nEVAL @DATA[1]\nEVAL @DATA[2]\nEVAL @DATA[3]";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(3), Value::integer(6), Value::integer(2)]
    );

    let (sources, source_id) = source("DIM @ONE[1]\nPACK @ONE = 42\nEVAL @ONE[1]", "program.tbx");
    let mut writer = RecordingWriter::default();
    let result = success(execute_registered_source(&sources, source_id, &mut writer));
    assert_eq!(result.data_stack(), [Value::integer(42)]);
}

#[test]
fn pack_consumes_only_the_top_values_when_rhs_leaves_extras() {
    let text = "DIM @DATA[2]\nPACK @DATA = 10, 20, 30\nEVAL @DATA[1]\nEVAL @DATA[2]";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(10), Value::integer(20), Value::integer(30)]
    );
}

#[test]
fn pack_rejects_invalid_targets_and_syntax_with_source_diagnostics() {
    for text in [
        "PACK @MISSING = 1",
        "VAR VALUE\nLET VALUE = 1\nPACK @VALUE = 1",
        "DIM @DATA[1]\nPACK DATA = 1",
        "DIM @DATA[1]\nPACK @DATA",
        "DIM @DATA[1]\nPACK @DATA =",
        "DIM @DATA[1]\nPACK @DATA = 1 2",
    ] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            failure.class(),
            UserFacingFailureClass::UserProgram,
            "{text}"
        );
        assert!(failure.diagnostic().primary().is_some(), "{text}");
    }
}

#[test]
fn array_element_access_resolves_names_case_insensitively() {
    let (sources, source_id) = source(
        "DIM @Scores[2]\nLET @sCoReS[2] = 11\nEVAL @SCORES[2]",
        "program.tbx",
    );
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(11)]);
}

#[test]
fn array_index_runtime_errors_are_preserved() {
    for index in ["0", "-1", "3"] {
        let text = format!("DIM @A[2]\nEVAL @A[{index}]");
        let (sources, source_id) = source(&text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            failure.class(),
            UserFacingFailureClass::UserProgram,
            "{index}"
        );
    }
}

#[test]
fn array_access_rejects_non_arrays_and_malformed_forms_at_compile_time() {
    for text in [
        "DIM @DATA[2]\nEVAL @DATA",
        "DIM @DATA[2]\nEVAL @DATA[]",
        "DIM @DATA[2]\nEVAL @DATA[1",
        "DIM @DATA[2]\nEVAL @DATA[1] 2",
        "EVAL @MISSING[1]",
        "EVAL @Z[1]",
        "EVAL @ABS[1]",
        "EVAL @LET[1]",
    ] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            failure.class(),
            UserFacingFailureClass::UserProgram,
            "{text}"
        );
        assert!(
            failure.diagnostic().primary().is_some(),
            "{text}: {:?}",
            failure
        );
    }
}

#[test]
fn putchr_accepts_character_hex_and_triple_quote_literals_from_source() {
    let text = "PUTCHR 'A'\nPUTCHR $41\nPUTCHR '''\nPUTCHR $27\nPUTCHR $0A";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), []);
    assert_eq!(writer.text(), "AA''\n");
}

#[test]
fn putchr_reports_ascii_range_errors_after_hex_literal_evaluation() {
    for text in ["PUTCHR -1", "PUTCHR 128", "PUTCHR $F1"] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(
            failure.class(),
            UserFacingFailureClass::UserProgram,
            "{text}"
        );
        assert_eq!(writer.text(), "", "{text}");
    }
}

#[test]
fn abs_is_available_as_a_runtime_word_in_ordinary_expressions() {
    let (sources, source_id) = source("EVAL ABS(0)\nEVAL ABS(42)\nEVAL ABS(-42)", "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(0), Value::integer(42), Value::integer(42)]
    );
    assert_eq!(writer.text(), "");
}

#[test]
fn abs_minimum_integer_reports_a_runtime_failure_from_source() {
    let (sources, source_id) = source("EVAL ABS(-32768)", "program.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
    assert_eq!(writer.text(), "");
}

#[test]
fn print_lowers_fixed_text_and_integer_expressions_in_source_order() {
    let text = "VAR AGE\nVAR BONUS\nLET AGE = 42\nLET BONUS = 5\nPRINT \"Im \", AGE, \" years old. TOTAL = \", AGE + BONUS";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(writer.text(), "Im 42 years old. TOTAL = 47");
}

#[test]
fn print_does_not_emit_for_commas_or_add_a_newline() {
    let (sources, source_id) = source("PRINT \"A\", \"B\"", "program.tbx");
    let mut writer = RecordingWriter::default();

    success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(writer.text(), "AB");
}

#[test]
fn print_rejects_missing_items_and_invalid_item_separators() {
    for text in [
        "PRINT",
        "PRINT , 1",
        "PRINT 1,",
        "PRINT 1,,2",
        "PRINT \"A\" + 1",
    ] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        assert!(
            matches!(
                execute_registered_source(&sources, source_id, &mut writer),
                BatchExecutionResult::Failure(_)
            ),
            "{text} should be rejected"
        );
    }
}

#[test]
fn print_does_not_execute_items_after_expression_failure() {
    let (sources, source_id) = source("PRINT \"before\", 1 / 0, \"after\"", "program.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
    assert_eq!(writer.text(), "before");
}

#[test]
fn print_does_not_execute_items_after_output_failure() {
    let (sources, source_id) = source("PRINT \"before\", 7, \"after\"", "program.tbx");
    let mut writer = RecordingWriter::failing_after(1);

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert_eq!(writer.text(), "before");
}

#[test]
fn print_lowers_local_references_inside_a_definition() {
    let text = "DEF SHOW value\nPRINT \"value=\", value\nEND\nEVAL SHOW(7)";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(writer.text(), "value=7");
}

#[test]
fn print_syntax_diagnostics_point_at_the_invalid_separator_or_item() {
    for (text, expected_column) in [("PRINT 1,", 8), ("PRINT 1,,2", 9), ("PRINT 1 2", 9)] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
        let primary = failure.diagnostic().primary().unwrap_or_else(|| {
            panic!("PRINT syntax failure should have a source position: {text}")
        });
        assert_eq!(primary.line_number(), 1, "{text}");
        assert_eq!(primary.column_number(), expected_column, "{text}");
        assert_eq!(primary.source_line(), text, "{text}");
    }
}

#[test]
fn invalid_extra_character_literal_quotes_keep_a_source_diagnostic_span() {
    let text = "EVAL ''''";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
    let primary = failure
        .diagnostic()
        .primary()
        .expect("diagnostic should have a span");

    assert_eq!(primary.line_number(), 1);
    assert_eq!(primary.column_number(), 6);
    assert_eq!(primary.source_line(), text);
}

#[test]
fn print_preserves_expression_diagnostics_for_undefined_names() {
    let (sources, source_id) = source("PRINT A + MISSING", "program.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
    let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
        panic!("PRINT expression failure should be a source failure");
    };

    assert!(matches!(
        user_failure.original_error(),
        SourceProcessorError::SourceWord(crate::source_word::SourceWordError::Expression {
            source: crate::expression::ExpressionError::Variable(_),
        })
    ));
}

#[test]
fn located_compile_failure_renders_registered_display_name_and_position() {
    for display_name in ["relative/program.tbx", "<stdin>"] {
        let (sources, source_id) = source("UNKNOWN", display_name);
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        let primary = failure
            .diagnostic()
            .primary()
            .expect("compile failure should have a source position");
        assert_eq!(primary.display_name(), display_name);
        assert_eq!(primary.line_number(), 1);
        assert_eq!(primary.column_number(), 1);
    }
}

#[test]
fn foreign_source_id_is_source_less_environment_failure() {
    let (sources, _) = source("EVAL 1", "program.tbx");
    let (foreign_sources, foreign_source_id) = source("EVAL 2", "other.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(
        &sources,
        foreign_source_id,
        &mut writer,
    ));
    drop(foreign_sources);

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert!(failure.diagnostic().primary().is_none());
    assert_eq!(failure.diagnostic().target(), Some("source program"));
}

#[test]
fn runtime_failure_is_a_located_user_program_failure() {
    let (sources, source_id) = source("DUP", "program.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
    assert_eq!(
        failure
            .diagnostic()
            .primary()
            .map(|primary| primary.source_line()),
        Some("DUP")
    );
}

#[test]
fn runtime_output_failure_is_a_located_environment_failure() {
    let (sources, source_id) = source("EVAL 7\nPUTDEC", "program.tbx");
    let mut writer = RecordingWriter::failing_after(0);

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert!(failure.diagnostic().primary().is_some());
    assert_eq!(writer.text(), "");
}

#[test]
fn successful_runtime_output_is_not_rolled_back_by_a_later_failure() {
    let (sources, source_id) = source("EVAL 7\nPUTDEC\nCR", "program.tbx");
    let mut writer = RecordingWriter::failing_after(1);

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(failure.class(), UserFacingFailureClass::Environment);
    assert_eq!(writer.text(), "7");
}
