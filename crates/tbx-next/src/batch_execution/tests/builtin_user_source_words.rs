use super::*;

#[test]
fn batch_top_level_can_publish_and_use_a_source_word_before_a_definition() {
    let text = "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nLET A = 0\nSLET A = 7\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(A)";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(7)]);
}

#[test]
fn user_syntax_can_resolve_and_emit_runtime_words_variables_and_integers() {
    let text = "SYNTAX EMIT
STATEMENT
READ_NAME AS variable
RESOLVE_VAR variable AS target
EMIT_LOAD target
READ_NAME AS word
RESOLVE_WORD word AS callable
EMIT_CALL callable
EMIT_INT 7
ENDS
LET A = 3
EMIT A DUP";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(3), Value::integer(3), Value::integer(7)]
    );
}

#[test]
fn user_syntax_emits_control_value_operations_and_executes_them_as_a_lifo() {
    let text = "SYNTAX CONTROL
STATEMENT
READ_EXPR AS value
EXPECT_END
EMIT_EXPR value
EMIT_CONTROL_PUSH
EMIT_CONTROL_COPY
EMIT_CONTROL_DROP
ENDS
CONTROL 42";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(42)]);
}

#[test]
fn user_syntax_control_value_operations_restore_an_outer_lifo_value() {
    let text = "SYNTAX PUSH_CONTROL
STATEMENT
READ_EXPR AS value
EXPECT_END
EMIT_EXPR value
EMIT_CONTROL_PUSH
ENDS
SYNTAX COPY_CONTROL
STATEMENT
EXPECT_END
EMIT_CONTROL_COPY
ENDS
SYNTAX DROP_CONTROL
STATEMENT
EXPECT_END
EMIT_CONTROL_DROP
ENDS
PUSH_CONTROL 10
PUSH_CONTROL 20
COPY_CONTROL
DROP_CONTROL
COPY_CONTROL
DROP_CONTROL";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(
        result.data_stack(),
        [Value::integer(20), Value::integer(10)]
    );
}

#[test]
fn user_syntax_control_value_underflow_reaches_the_runtime_error() {
    let text = "SYNTAX COPY_CONTROL
STATEMENT
EXPECT_END
EMIT_CONTROL_COPY
ENDS
COPY_CONTROL";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();
    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

    let BatchExecutionFailureCause::Source(user_failure) = failure.cause else {
        panic!("control-value underflow should be a source runtime failure");
    };
    let SourceProcessorError::Runtime(error) = user_failure.original_error() else {
        panic!("control-value underflow should preserve the runtime error");
    };
    assert!(matches!(
        error.vm().kind(),
        crate::vm::VmErrorKind::ControlValueStackUnderflow { .. }
    ));
}

#[test]
fn control_value_emit_operations_reject_operands() {
    for operation in [
        "EMIT_CONTROL_PUSH",
        "EMIT_CONTROL_COPY",
        "EMIT_CONTROL_DROP",
    ] {
        let text = format!("SYNTAX CONTROL\nSTATEMENT\n{operation} EXTRA\nENDS\nCONTROL");
        let (sources, source_id) = source(&text, "program.tbx");
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert!(matches!(
            failure.cause,
            BatchExecutionFailureCause::Source(_)
        ));
    }
}

#[test]
fn user_syntax_can_resolve_a_fixed_runtime_word_at_use_time() {
    let text = "SYNTAX LATER_CALL\nSTATEMENT\nRESOLVE_WORD_LITERAL LATER AS word\nEMIT_CALL word\nENDS\nDEF LATER\nDUP\nEND\nEVAL 7\nLATER_CALL";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(7)]);
}

#[test]
fn fixed_runtime_word_resolution_reports_the_literal_operand_span() {
    for (text, expected_line) in [
        (
            "SYNTAX FIXED_CALL\nSTATEMENT\nRESOLVE_WORD_LITERAL MISSING AS word\nEMIT_CALL word\nENDS\nFIXED_CALL",
            "RESOLVE_WORD_LITERAL MISSING AS word",
        ),
        (
            "SYNTAX FIXED_CALL\nSTATEMENT\nRESOLVE_WORD_LITERAL A AS word\nEMIT_CALL word\nENDS\nLET A = 1\nFIXED_CALL",
            "RESOLVE_WORD_LITERAL A AS word",
        ),
    ] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
        let primary = failure
            .diagnostic()
            .primary()
            .expect("fixed runtime word failure should retain a source span");

        assert_eq!(primary.source_line(), expected_line);
        assert!(primary.column_number() > "RESOLVE_WORD_LITERAL ".len());
    }
}

#[test]
fn user_syntax_reports_runtime_emit_binding_and_literal_errors_at_source_spans() {
    for text in [
        "SYNTAX S\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_WORD name AS word\nENDS\nLET A = 1\nS A",
        "SYNTAX S\nSTATEMENT\nEMIT_INT 32768\nENDS\nS",
    ] {
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();
        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
        let primary = failure
            .diagnostic()
            .primary()
            .expect("invalid emit should retain a source span");

        assert!(!primary.source_line().is_empty());
    }
}

#[test]
fn user_syntax_reports_undefined_runtime_word_at_name_span() {
    let text = "SYNTAX S\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_WORD name AS word\nENDS\nS MISSING";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let failure = failure(execute_registered_source(&sources, source_id, &mut writer));
    let primary = failure
        .diagnostic()
        .primary()
        .expect("undefined runtime word should retain a source span");

    assert!(!primary.source_line().is_empty());
    assert!(primary.column_number() > 0);
}

#[test]
fn batch_top_level_can_publish_and_use_a_block_source_word() {
    let text = "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS\nWRAP\nENDWRAP\nEVAL 9";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(9)]);
}

#[test]
fn batch_top_level_can_publish_multiple_source_words_in_sequence() {
    let text = "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nSYNTAX SADD\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nLET A = 0\nSLET A = 3\nSADD A = 4\nEVAL A";
    let (sources, source_id) = source(text, "program.tbx");
    let mut writer = RecordingWriter::default();

    let result = success(execute_registered_source(&sources, source_id, &mut writer));

    assert_eq!(result.data_stack(), [Value::integer(4)]);
}
