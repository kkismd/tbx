use super::*;
use crate::source_word_evaluator::SourceWordEvaluationError;

#[test]
fn user_defined_statement_publishes_and_dispatches_let_equivalent() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "SLET A = 1 + 2 * 3",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(7)));
}

#[test]
fn user_defined_statement_dispatches_bif_equivalent_with_delimiter_expect() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX UBIF\nSTATEMENT\nREAD_EXPR_UNTIL \",\" AS condition\nEXPECT \",\"\nREAD_LINE_NUM AS line\nEXPECT_END\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE line\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "UBIF 0, 100\nLET A = 1\n100 LET A = 2",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(2)));
}

#[test]
fn user_defined_statement_dispatches_fixed_name_input_case_insensitively() {
    let mut session = RuntimeDefinitionSession::new();
    session.publish_syntax("SYNTAX EXPECTTO\nSTATEMENT\nEXPECT_NAME TO\nEXPECT_END\nENDS");

    let (sources, id) = source("EXPECTTO to");
    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("fixed Name input should compile");
    session
        .run_unit_with_published_code(&unit)
        .expect("fixed Name input should run");
}

#[test]
fn user_defined_statement_stages_expression_until_fixed_name_and_leaves_delimiter() {
    let mut session = RuntimeDefinitionSession::new();
    session.publish_syntax(
        "SYNTAX NAMEEXPR\nSTATEMENT\nREAD_EXPR_UNTIL_NAME TO AS start\nEXPECT_NAME TO\nREAD_EXPR AS end\nEXPECT_END\nEMIT_EXPR start\nEMIT_EXPR end\nENDS",
    );

    let (sources, id) = source("NAMEEXPR 1 + (2) to 3 + 4");
    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("fixed Name expression input should compile");
    let result = session
        .run_unit_with_published_code(&unit)
        .expect("fixed Name expression input should run");

    assert_eq!(result.data_stack(), [value(3), value(7)]);
}

#[test]
fn fixed_name_input_rejects_wrong_name_and_missing_delimiter_with_source_spans() {
    let mut session = RuntimeDefinitionSession::new();
    session.publish_syntax(
        "SYNTAX NAMEEXPR\nSTATEMENT\nREAD_EXPR_UNTIL_NAME TO AS start\nEXPECT_NAME TO\nREAD_EXPR AS end\nEXPECT_END\nEMIT_EXPR start\nEMIT_EXPR end\nENDS",
    );
    session.publish_syntax("SYNTAX EXPECTTO\nSTATEMENT\nEXPECT_NAME TO\nEXPECT_END\nENDS");

    let (wrong_sources, wrong_id) = source("EXPECTTO FROM");
    let wrong_error = compile_source(
        wrong_sources.view(),
        wrong_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect_err("a different Name must be rejected");
    assert_eq!(
        wrong_error.primary_span(),
        Some(
            wrong_sources
                .view()
                .span(wrong_id, 9, 13)
                .expect("wrong span")
        )
    );

    let (missing_sources, missing_id) = source("NAMEEXPR 1 + 2");
    let missing_error = compile_source(
        missing_sources.view(),
        missing_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect_err("a missing Name delimiter must be rejected");
    assert_eq!(
        missing_error.primary_span(),
        Some(
            missing_sources
                .view()
                .span(missing_id, 13, 14)
                .expect("missing delimiter span"),
        )
    );
}

#[test]
fn fixed_name_operations_reject_malformed_source_processing_definitions() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _variables) =
        global_source_fixture();

    let (_sources, _id, missing) = publish_user_source_word_error(
        "SYNTAX BROKEN\nSTATEMENT\nEXPECT_NAME\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    assert!(matches!(
        missing,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition {
            kind: crate::source_word::SyntaxDefinitionErrorKind::ExpectedName,
            ..
        })
    ));

    let (_sources, _id, trailing) = publish_user_source_word_error(
        "SYNTAX BROKEN2\nSTATEMENT\nEXPECT_NAME TO EXTRA\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    assert!(matches!(
        trailing,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition {
            kind: crate::source_word::SyntaxDefinitionErrorKind::TrailingOperationToken {
                kind: TokenKind::Name
            },
            ..
        })
    ));

    publish_user_source_word(
        "SYNTAX BROKEN3\nSTATEMENT\nEXPECT_NAME TO\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let (sources, id) = source("BROKEN3 123");
    let non_name_use = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("a non-Name token must be rejected by EXPECT_NAME");
    assert_eq!(
        non_name_use.primary_span(),
        Some(sources.view().span(id, 8, 11).expect("non-Name span"))
    );

    for definition in [
        "SYNTAX BROKEN4\nSTATEMENT\nREAD_EXPR_UNTIL_NAME\nENDS",
        "SYNTAX BROKEN5\nSTATEMENT\nREAD_EXPR_UNTIL_NAME TO\nENDS",
        "SYNTAX BROKEN6\nSTATEMENT\nREAD_EXPR_UNTIL_NAME TO AS expr EXTRA\nENDS",
    ] {
        let (sources, id) = source(definition);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_user_source_word_publication_and_operators(
                &mut bindings,
                &source_words,
                operators.lookup(),
                &mut globals,
            ),
        )
        .expect_err("READ_EXPR_UNTIL_NAME definition should fail");
        assert!(
            matches!(
                error,
                SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition { .. })
            ),
            "{definition}: {error:?}"
        );
    }
}

#[test]
fn user_defined_block_with_only_terminator_dispatches_as_structured_source_word() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "WRAP\nLET A = 4\nENDWRAP",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(4)));
}

#[test]
fn user_defined_block_publishes_exit_target_and_control_value_metadata() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX TARGET\nBLOCK EXIT_TARGET\nSTART\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDTARGET\nEXPECT_END\nEMIT_CONTROL_DROP\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    let Some(Binding::SourceWord(id)) = bindings.get(&name("TARGET")).copied() else {
        panic!("TARGET should publish as a source word");
    };
    let SourceWordDispatch::Structured {
        implementation: StructuredSourceWordDispatch::UserDefined(implementation),
        ..
    } = source_words
        .lookup()
        .lookup_dispatch(id)
        .expect("published source word should dispatch")
    else {
        panic!("TARGET should keep the user-defined structured implementation");
    };
    assert!(implementation.exit_target());
    assert_eq!(implementation.control_value_ownership(), 1);

    publish_user_source_word(
        "SYNTAX MULTI\nBLOCK\nSTART\nEMIT_CONTROL_PUSH\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDMULTI\nEXPECT_END\nEMIT_CONTROL_DROP\nEMIT_CONTROL_DROP\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let Some(Binding::SourceWord(id)) = bindings.get(&name("MULTI")).copied() else {
        panic!("MULTI should publish as a source word");
    };
    let SourceWordDispatch::Structured {
        implementation: StructuredSourceWordDispatch::UserDefined(implementation),
        ..
    } = source_words
        .lookup()
        .lookup_dispatch(id)
        .expect("published source word should dispatch")
    else {
        panic!("MULTI should keep the user-defined structured implementation");
    };
    assert!(!implementation.exit_target());
    assert_eq!(implementation.control_value_ownership(), 2);
}

#[test]
fn block_attributes_and_control_value_contracts_are_validated_at_publication() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    for (definition, expected_kind) in [
        (
            "SYNTAX PLAIN\nBLOCK\nSTART\nEXPECT_END\nLAST ENDPLAIN\nEXPECT_END\nENDS",
            None,
        ),
        (
            "SYNTAX UNKNOWN\nBLOCK OTHER\nSTART\nEXPECT_END\nLAST ENDUNKNOWN\nEXPECT_END\nENDS",
            Some(SyntaxDefinitionErrorKind::UnsupportedKind),
        ),
        (
            "SYNTAX MULTI\nBLOCK EXIT_TARGET EXTRA\nSTART\nEXPECT_END\nLAST ENDMULTI\nEXPECT_END\nENDS",
            Some(SyntaxDefinitionErrorKind::TrailingOperationToken {
                kind: TokenKind::Name,
            }),
        ),
    ] {
        if expected_kind.is_none() {
            publish_user_source_word(
                definition,
                &mut bindings,
                &mut globals,
                &mut source_words,
                operators.lookup(),
            );
            continue;
        }
        let (_sources, _id, error) = publish_user_source_word_error(
            definition,
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );
        let SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition { kind, .. }) =
            error
        else {
            panic!("invalid BLOCK attribute should be a syntax-definition error");
        };
        assert_eq!(Some(kind), expected_kind);
    }

    let invalid_definitions = [
        (
            "SYNTAX STARTDROP\nBLOCK\nSTART\nEMIT_CONTROL_DROP\nEXPECT_END\nLAST ENDSTARTDROP\nEXPECT_END\nENDS",
            SyntaxDefinitionErrorKind::ControlValueStartDrop,
            "EMIT_CONTROL_DROP",
        ),
        (
            "SYNTAX MARKPUSH\nBLOCK\nSTART\nEXPECT_END\nMARK MID\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDMARKPUSH\nEXPECT_END\nENDS",
            SyntaxDefinitionErrorKind::ControlValueMarkerOperation,
            "EMIT_CONTROL_PUSH",
        ),
        (
            "SYNTAX MARKDROP\nBLOCK\nSTART\nEXPECT_END\nMARK MID\nEMIT_CONTROL_DROP\nEXPECT_END\nLAST ENDMARKDROP\nEXPECT_END\nENDS",
            SyntaxDefinitionErrorKind::ControlValueMarkerOperation,
            "EMIT_CONTROL_DROP",
        ),
        (
            "SYNTAX LASTPUSH\nBLOCK\nSTART\nEXPECT_END\nLAST ENDLASTPUSH\nEMIT_CONTROL_PUSH\nEXPECT_END\nENDS",
            SyntaxDefinitionErrorKind::ControlValueTerminatorPush,
            "EMIT_CONTROL_PUSH",
        ),
        (
            "SYNTAX MISMATCH\nBLOCK\nSTART\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDMISMATCH\nEXPECT_END\nENDS",
            SyntaxDefinitionErrorKind::ControlValueOwnershipMismatch,
            "LAST ENDMISMATCH",
        ),
        (
            "SYNTAX EXCESSDROP\nBLOCK\nSTART\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDEXCESSDROP\nEXPECT_END\nEMIT_CONTROL_DROP\nEMIT_CONTROL_DROP\nENDS",
            SyntaxDefinitionErrorKind::ControlValueOwnershipMismatch,
            "EMIT_CONTROL_DROP",
        ),
        (
            "SYNTAX ORDER\nBLOCK\nSTART\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDORDER\nEMIT_CONTROL_DROP\nEMIT_INT 1\nENDS",
            SyntaxDefinitionErrorKind::ControlValueCleanupOrder,
            "EMIT_INT",
        ),
    ];
    for (definition, expected_kind, primary_text) in invalid_definitions {
        let (sources, source_id, error) = publish_user_source_word_error(
            definition,
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );
        let SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition { span, kind }) =
            error
        else {
            panic!("invalid control-value contract should be a syntax-definition error");
        };
        assert_eq!(kind, expected_kind);
        let start = definition
            .find(primary_text)
            .expect("primary text should be in definition");
        let expected_span = sources
            .view()
            .span(source_id, start, start + primary_text.len())
            .expect("expected primary span");
        assert_eq!(span, expected_span);
    }
}

#[test]
fn request_exit_is_validated_and_compiled_against_the_innermost_exit_target() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    for definition in [
        "SYNTAX INVALID1\nSTATEMENT\nREQUEST_EXIT\nREQUEST_EXIT\nENDS",
        "SYNTAX INVALID2\nSTATEMENT\nREQUEST_EXIT\nEXPECT_END\nENDS",
        "SYNTAX INVALID3\nBLOCK\nSTART\nREQUEST_EXIT\nLAST ENDINVALID3\nEXPECT_END\nENDS",
        "SYNTAX INVALID4\nBLOCK\nSTART\nEXPECT_END\nMARK MID\nREQUEST_EXIT\nLAST ENDINVALID4\nEXPECT_END\nENDS",
        "SYNTAX INVALID5\nBLOCK\nSTART\nEXPECT_END\nLAST ENDINVALID5\nREQUEST_EXIT\nENDS",
    ] {
        let (_sources, _id, error) = publish_user_source_word_error(
            definition,
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );
        assert!(matches!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition {
                kind: SyntaxDefinitionErrorKind::RequestExitPlacement,
                ..
            })
        ));
    }
    publish_user_source_word(
        "SYNTAX BREAK\nSTATEMENT\nEXPECT_END\nREQUEST_EXIT\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    publish_user_source_word(
        "SYNTAX TARGET\nBLOCK EXIT_TARGET\nSTART\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDTARGET\nEXPECT_END\nEMIT_CONTROL_DROP\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    publish_user_source_word(
        "SYNTAX WRAP\nBLOCK\nSTART\nEMIT_CONTROL_PUSH\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nEMIT_CONTROL_DROP\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    publish_user_source_word(
        "SYNTAX ZERO\nBLOCK EXIT_TARGET\nSTART\nEXPECT_END\nLAST ENDZERO\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    let (sources, source_id) = source("TARGET\nWRAP\nBREAK\nENDWRAP\nENDTARGET");
    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("exit within a target should compile");
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::PushControlValue)
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::PushControlValue)
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::DropControlValue)
    );
    assert_eq!(
        unit.instructions().get(address(3)),
        Ok(&Instruction::DropControlValue)
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Jump(address(7)))
    );
    assert_eq!(
        unit.instructions().get(address(5)),
        Ok(&Instruction::DropControlValue)
    );
    assert_eq!(
        unit.instructions().get(address(6)),
        Ok(&Instruction::DropControlValue)
    );
    assert_eq!(unit.instructions().get(address(7)), Ok(&Instruction::Halt));
    let break_span = sources.view().span(source_id, 12, 17).expect("BREAK span");
    assert_eq!(unit.source_span(location(&unit, 2)), Ok(Some(break_span)));
    assert_eq!(unit.source_span(location(&unit, 3)), Ok(Some(break_span)));
    assert_eq!(unit.source_span(location(&unit, 4)), Ok(Some(break_span)));

    let (zero_sources, zero_id) = source("ZERO\nBREAK\nENDZERO");
    let zero = compile_source(
        zero_sources.view(),
        zero_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("zero-ownership target should compile");
    assert_eq!(
        zero.instructions().get(address(0)),
        Ok(&Instruction::Jump(address(1)))
    );
    assert_eq!(zero.instructions().get(address(1)), Ok(&Instruction::Halt));
    assert_eq!(
        zero.source_span(location(&zero, 0)),
        Ok(Some(
            zero_sources
                .view()
                .span(zero_id, 5, 10)
                .expect("BREAK span")
        ))
    );

    let (multiple_sources, multiple_id) =
        source("TARGET\nWRAP\nWRAP\nBREAK\nENDWRAP\nENDWRAP\nENDTARGET");
    let multiple = compile_source(
        multiple_sources.view(),
        multiple_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("multiple non-target frames should be transparent");
    for index in 3..6 {
        assert_eq!(
            multiple.instructions().get(address(index)),
            Ok(&Instruction::DropControlValue)
        );
    }
    assert_eq!(
        multiple.instructions().get(address(6)),
        Ok(&Instruction::Jump(address(10)))
    );

    let (nested_sources, nested_id) = source("TARGET\nTARGET\nBREAK\nENDTARGET\nENDTARGET");
    let nested = compile_source(
        nested_sources.view(),
        nested_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("nested targets should compile");
    assert_eq!(
        nested.instructions().get(address(2)),
        Ok(&Instruction::DropControlValue)
    );
    assert_eq!(
        nested.instructions().get(address(3)),
        Ok(&Instruction::Jump(address(5)))
    );

    let (outside_sources, outside_id) = source("BREAK");
    let outside = compile_source(
        outside_sources.view(),
        outside_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("exit without a target should fail");
    assert!(matches!(
        outside,
        SourceProcessorError::Compile(CompileError {
            kind: CompileErrorKind::StructuredExitTargetUnavailable,
            ..
        })
    ));
    assert_eq!(
        outside.primary_span(),
        Some(
            outside_sources
                .view()
                .span(outside_id, 0, 5)
                .expect("BREAK span")
        )
    );
}

#[test]
fn user_defined_block_declares_marker_grammar_and_reservations_from_sections() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX TWOPART\nBLOCK\nSTART\nEXPECT_END\nMARK MID\nEXPECT_END\nLAST ENDTWO\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    let Some(Binding::SourceWord(id)) = bindings.get(&name("TWOPART")).copied() else {
        panic!("TWOPART should publish as a source word");
    };
    let SourceWordDispatch::Structured { grammar, .. } = source_words
        .lookup()
        .lookup_dispatch(id)
        .expect("published source word should dispatch")
    else {
        panic!("TWOPART should keep the structured source word kind");
    };
    assert_eq!(grammar.groups().len(), 1);
    assert_eq!(grammar.groups()[0].cardinality(), MarkerCardinality::One);
    assert_eq!(
        bindings
            .syntax_marker_reservation(&name("MID"))
            .map(|reservation| reservation.owner()),
        Some(id)
    );
    assert_eq!(
        bindings
            .syntax_marker_reservation(&name("ENDTWO"))
            .map(|reservation| reservation.owner()),
        Some(id)
    );

    run_with_source_words_operators_and_mut_globals(
        "TWOPART\nLET A = 1\nMID\nLET A = 2\nENDTWO",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(2)));
}

#[test]
fn user_defined_block_nests_with_native_structured_source_words_both_directions() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "IF 1\nWRAP\nLET A = 5\nENDWRAP\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(5)));

    run_with_source_words_operators_and_mut_globals(
        "WRAP\nIF 1\nLET A = 6\nENDIF\nENDWRAP",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(6)));
}

#[test]
fn user_defined_blocks_nest_without_confusing_outer_markers() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "WRAP\nWRAP\nLET A = 7\nENDWRAP\nENDWRAP",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(7)));
}

#[test]
fn user_defined_block_shares_one_line_number_scope_across_markers() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nMARK MID\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    let (sources, source_id) = source("WRAP\nBIF 0, 20\n10 LET A = 1\nMID\n20 LET A = 2\nENDWRAP");
    compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("a line-number branch may cross markers in one structured owner");

    let (duplicate_sources, duplicate_id) =
        source("WRAP\n10 LET A = 1\nMID\n10 LET A = 2\nENDWRAP");
    let error = compile_source(
        duplicate_sources.view(),
        duplicate_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("one structured block must reject duplicate line numbers across markers");
    assert!(matches!(
        error,
        SourceProcessorError::Compile(CompileError {
            kind: CompileErrorKind::LineNumber { .. },
            ..
        })
    ));
}

#[test]
fn nested_user_defined_blocks_have_independent_line_number_scopes() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    for (name_text, end_text) in [("OUTER", "ENDOUTER"), ("INNER", "ENDINNER")] {
        publish_user_source_word(
            &format!(
                "SYNTAX {name_text}\nBLOCK\nSTART\nEXPECT_END\nLAST {end_text}\nEXPECT_END\nENDS"
            ),
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );
    }

    let (sources, source_id) =
        source("OUTER\n10 LET A = 1\nINNER\n10 LET A = 2\nENDINNER\nENDOUTER");
    compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("nested structured owners may reuse line numbers independently");

    for text in [
        "OUTER\nBIF 0, 20\nINNER\n20 LET A = 2\nENDINNER\nENDOUTER",
        "OUTER\n10 LET A = 1\nINNER\nBIF 0, 10\nENDINNER\nENDOUTER",
        "OUTER\nINNER\n10 LET A = 1\nENDINNER\nINNER\nBIF 0, 10\nENDINNER\nENDOUTER",
    ] {
        let (sources, source_id) = source(text);
        let error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect_err("line-number references must not cross structured owner scopes");
        assert!(matches!(
            error,
            SourceProcessorError::Compile(CompileError {
                kind: CompileErrorKind::LineNumber { .. },
                ..
            })
        ));
    }
}

#[test]
fn user_defined_block_rejects_marker_order_violation_without_binding_fallback() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX ORDERED\nBLOCK\nSTART\nEXPECT_END\nMARK FIRST\nEXPECT_END\nMARK_OPTIONAL SECOND\nEXPECT_END\nLAST ENDORDER\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let (sources, source_id) = source("ORDERED\nSECOND\nFIRST\nENDORDER");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("out-of-order marker should fail grammar validation");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::StructuredGrammar {
            span: span(sources.view(), source_id, 8, 14),
            source: crate::structured_grammar::GrammarProgressError::RequiredGroupUnmet {
                required_group_index: 0,
                attempted_group_index: 1,
            }
        })
    );
}

#[test]
fn user_defined_block_publication_is_atomic_when_marker_reservation_conflicts() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    let (_sources, _source_id, error) = publish_user_source_word_error(
        "SYNTAX BROKEN\nBLOCK\nSTART\nEXPECT_END\nLAST ENDIF\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxNameConflict { .. })
    ));
    assert_eq!(bindings.get(&name("BROKEN")), None);
}

#[test]
fn user_defined_block_rejects_missing_last_without_binding_fallback() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    let (_sources, _source_id, error) = publish_user_source_word_error(
        "SYNTAX BROKEN\nBLOCK\nSTART\nEXPECT_END\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition {
            kind: crate::source_word::SyntaxDefinitionErrorKind::MissingKind,
            ..
        })
    ));
    assert_eq!(bindings.get(&name("BROKEN")), None);
}

#[test]
fn user_defined_block_allows_start_local_in_later_sections() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX ASSIGNBLOCK\nBLOCK\nSTART\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT_END\nLAST ENDASSIGN\nREAD_EXPR AS expr\nEXPECT_END\nEMIT_EXPR expr\nEMIT_STORE target\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "ASSIGNBLOCK A\nENDASSIGN 8",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(8)));
}

#[test]
fn user_defined_block_allows_required_marker_local_in_later_sections() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX MARKASSIGN\nBLOCK\nSTART\nEXPECT_END\nMARK TARGET\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT_END\nLAST ENDMARKASSIGN\nREAD_EXPR AS expr\nEXPECT_END\nEMIT_EXPR expr\nEMIT_STORE target\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "MARKASSIGN\nTARGET A\nENDMARKASSIGN 9",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(9)));
}

#[test]
fn user_defined_block_rejects_optional_marker_local_in_later_sections() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    let (_sources, _source_id, error) = publish_user_source_word_error(
        "SYNTAX BROKEN\nBLOCK\nSTART\nEXPECT_END\nMARK_OPTIONAL TARGET\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT_END\nLAST ENDBROKEN\nREAD_EXPR AS expr\nEXPECT_END\nEMIT_EXPR expr\nEMIT_STORE target\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxBuild {
            source: crate::source_word_ir::SourceWordBuildError::UndefinedLocal { .. }
        })
    ));
    assert_eq!(bindings.get(&name("BROKEN")), None);
}

#[test]
fn user_defined_block_rejects_repeating_marker_local_in_later_sections() {
    for marker_header in ["MARK_ANY TARGET", "MARK_SOME TARGET"] {
        let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
            global_source_fixture();
        let source = format!(
            "SYNTAX BROKEN\nBLOCK\nSTART\nEXPECT_END\n{marker_header}\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT_END\nLAST ENDBROKEN\nREAD_EXPR AS expr\nEXPECT_END\nEMIT_EXPR expr\nEMIT_STORE target\nENDS"
        );
        let (_sources, _source_id, error) = publish_user_source_word_error(
            &source,
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );

        assert!(matches!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::SyntaxBuild {
                source: crate::source_word_ir::SourceWordBuildError::UndefinedLocal { .. }
            })
        ));
        assert_eq!(bindings.get(&name("BROKEN")), None);
    }
}

#[test]
fn user_defined_block_allows_optional_and_repeating_marker_local_inside_same_section() {
    for (marker_header, expected) in [
        ("MARK_OPTIONAL TARGET", 10),
        ("MARK_ANY TARGET", 11),
        ("MARK_SOME TARGET", 12),
    ] {
        let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
            global_source_fixture();
        let source = format!(
            "SYNTAX LOCALONLY\nBLOCK\nSTART\nEXPECT_END\n{marker_header}\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEXPECT_END\nEMIT_EXPR expr\nEMIT_STORE target\nLAST ENDLOCALONLY\nEXPECT_END\nENDS"
        );

        publish_user_source_word(
            &source,
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );

        run_with_source_words_operators_and_mut_globals(
            &format!("LOCALONLY\nTARGET A = {expected}\nENDLOCALONLY"),
            &bindings,
            &mut globals,
            &source_words,
            &words,
            &primitives,
            operators.lookup(),
        );

        assert_eq!(globals.view().read(variables[0]), Ok(value(expected)));
    }
}

#[test]
fn user_defined_while_uses_complete_branch_for_exit_and_explicit_back_branch() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX UWHILE\nBLOCK\nSTART\nPOSITION AS loop_start\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_COMPLETE\nLAST ENDUWHILE\nEXPECT_END\nEMIT_BRANCH loop_start\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "LET A = 0\nUWHILE A < 3\nLET A = A + 1\nENDUWHILE",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(3)));

    run_with_source_words_operators_and_mut_globals(
        "LET A = 2\nUWHILE A < 3\nLET A = A + 1\nENDUWHILE",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(3)));

    run_with_source_words_operators_and_mut_globals(
        "LET A = 9\nUWHILE A < 3\nLET A = 0\nENDUWHILE",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(9)));
}

#[test]
fn user_defined_if_resolves_following_through_complete_branch_sections() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX UIF\nBLOCK\nSTART\nREAD_EXPR AS start_condition\nEMIT_EXPR start_condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_ANY UELSIF\nEMIT_BRANCH_COMPLETE_IF_FOLLOWING\nPATCH_FOLLOWING\nREAD_EXPR AS elsif_condition\nEMIT_EXPR elsif_condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_OPTIONAL UELSE\nEMIT_BRANCH_COMPLETE_IF_FOLLOWING\nPATCH_FOLLOWING\nEXPECT_END\nLAST ENDUIF\nEXPECT_END\nPATCH_FOLLOWING\nPATCH_COMPLETE\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "UIF 0\nLET A = 1\nUELSIF 0\nLET A = 2\nUELSIF 1\nLET A = 3\nUELSE\nLET A = 4\nENDUIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(3)));

    run_with_source_words_operators_and_mut_globals(
        "UIF 1\nLET A = 1\nUELSIF 1\nLET A = 2\nUELSE\nLET A = 4\nENDUIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(1)));

    run_with_source_words_operators_and_mut_globals(
        "UIF 0\nLET A = 1\nUELSIF 0\nLET A = 2\nUELSE\nLET A = 4\nENDUIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(4)));

    run_with_source_words_operators_and_mut_globals(
        "UIF 0\nLET A = 1\nENDUIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(4)));
}

#[test]
fn nested_user_defined_structural_branches_remain_owner_local() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX UWHILE\nBLOCK\nSTART\nPOSITION AS loop_start\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_COMPLETE\nLAST ENDUWHILE\nEXPECT_END\nEMIT_BRANCH loop_start\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    publish_user_source_word(
        "SYNTAX UIF\nBLOCK\nSTART\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_OPTIONAL UELSE\nEMIT_BRANCH_COMPLETE_IF_FOLLOWING\nPATCH_FOLLOWING\nEXPECT_END\nLAST ENDUIF\nEXPECT_END\nPATCH_FOLLOWING\nPATCH_COMPLETE\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    run_with_source_words_operators_and_mut_globals(
        "LET A = 0\nLET B = 0\nUWHILE A < 3\nUIF A = 1\nLET B = B + 10\nUELSE\nLET B = B + 1\nENDUIF\nLET A = A + 1\nENDUWHILE",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(3)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(12)));
}

#[test]
fn user_defined_source_words_publish_and_dispatch_later_in_same_processing_session() {
    let (words, primitives, operators, source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    let (sources, source_id) = source(
        "SYNTAX SLET\n\
         STATEMENT\n\
         READ_NAME AS name\n\
         RESOLVE_VAR name AS target\n\
         EXPECT \"=\"\n\
         READ_EXPR AS expr\n\
         EMIT_EXPR expr\n\
         EMIT_STORE target\n\
         ENDS\n\
         SYNTAX UWHILE\n\
         BLOCK\n\
         START\n\
         POSITION AS loop_start\n\
         READ_EXPR AS condition\n\
         EMIT_EXPR condition\n\
         EMIT_BRANCH_IF_FALSE_COMPLETE\n\
         LAST ENDUWHILE\n\
         EXPECT_END\n\
         EMIT_BRANCH loop_start\n\
         ENDS\n\
         SYNTAX UIF\n\
         BLOCK\n\
         START\n\
         READ_EXPR AS condition\n\
         EMIT_EXPR condition\n\
         EMIT_BRANCH_IF_FALSE_FOLLOWING\n\
         MARK_OPTIONAL UELSE\n\
         EMIT_BRANCH_COMPLETE_IF_FOLLOWING\n\
         PATCH_FOLLOWING\n\
         EXPECT_END\n\
         LAST ENDUIF\n\
         EXPECT_END\n\
         PATCH_FOLLOWING\n\
         PATCH_COMPLETE\n\
         ENDS\n\
         SLET A = 0\n\
         UWHILE A < 3\n\
         UIF A = 1\n\
         SLET B = B + 10\n\
         UELSE\n\
         SLET B = B + 1\n\
         ENDUIF\n\
         SLET A = A + 1\n\
         ENDUWHILE",
    );

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_user_source_word_publication_and_operators(
            &mut bindings,
            &source_words,
            operators.lookup(),
            &mut globals,
        ),
    )
    .expect("same-session source words should publish before later dispatch");

    let Some(Binding::SourceWord(slet)) = bindings.get(&name("SLET")).copied() else {
        panic!("SLET should publish as a source word");
    };
    assert!(matches!(
        source_words.lookup().lookup_dispatch(slet),
        Ok(SourceWordDispatch::OneShot(
            OneShotSourceWordDispatch::UserDefined(_)
        ))
    ));
    let Some(Binding::SourceWord(uwhile)) = bindings.get(&name("UWHILE")).copied() else {
        panic!("UWHILE should publish as a source word");
    };
    assert!(matches!(
        source_words.lookup().lookup_dispatch(uwhile),
        Ok(SourceWordDispatch::Structured {
            implementation: StructuredSourceWordDispatch::UserDefined(_),
            ..
        })
    ));
    assert_eq!(
        bindings
            .syntax_marker_reservation(&name("ENDUWHILE"))
            .map(|reservation| reservation.owner()),
        Some(uwhile)
    );

    let result = run_unit(
        &unit,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_mut_globals(globals.view_mut()),
    )
    .expect("same-session user-defined source words should lower runnable code");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(globals.view().read(variables[0]), Ok(value(3)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(12)));
}

#[test]
fn user_defined_processing_failure_preserves_publication_and_later_owner_state() {
    let (words, primitives, operators, mut source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    publish_user_source_word(
        "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    publish_user_source_word(
        "SYNTAX UIF\nBLOCK\nSTART\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_FOLLOWING\nMARK_OPTIONAL UELSE\nEMIT_BRANCH_COMPLETE_IF_FOLLOWING\nPATCH_FOLLOWING\nEXPECT_END\nLAST ENDUIF\nEXPECT_END\nPATCH_FOLLOWING\nPATCH_COMPLETE\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let source_words_len = source_words.len();

    let (sources, source_id) = source("UIF 1\nSLET UNKNOWN = 1\nUELSE\nSLET A = 2\nENDUIF");
    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("inner user-defined statement failure should fail processing");

    assert_eq!(
        error.primary_span(),
        Some(span(sources.view(), source_id, 11, 18))
    );
    assert_eq!(source_words.len(), source_words_len);
    assert!(matches!(
        bindings.get(&name("SLET")),
        Some(Binding::SourceWord(_))
    ));
    assert!(matches!(
        bindings.get(&name("UIF")),
        Some(Binding::SourceWord(_))
    ));

    run_with_source_words_operators_and_mut_globals(
        "UIF 1\nSLET A = 5\nUELSE\nSLET A = 9\nENDUIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(5)));
}

#[test]
fn user_defined_return_equivalent_runs_through_runtime_definition_body() {
    let mut session = RuntimeDefinitionSession::new();
    register_builtin_global_variables(&mut session.globals, &mut session.bindings)
        .expect("A-Z variables should bootstrap");
    session.publish_syntax("SYNTAX URETURN\nSTATEMENT\nEXPECT_END\nEMIT_RETURN\nENDS");
    session.publish_def("DEF STOP\nEVAL 42\nIF 1\nURETURN\nENDIF\nLET A = 1\nEND");

    let (sources, source_id) = source("EVAL 7\nSTOP\nLET A = 2");
    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("caller should compile with source words");
    let code_spaces = [session.code.instruction_view()];
    let source_mappings = [session.code.source_mapping()];
    let result = run_unit(
        &unit,
        SourceExecutionContext::with_code_spaces_and_mappings(
            &session.bindings,
            &code_spaces,
            &source_mappings,
            PublishedWordLookup::new(&session.words),
            session.primitives.lookup(),
        )
        .with_mut_globals(session.globals.view_mut()),
    )
    .expect("caller should run against published code");
    assert_eq!(result.data_stack(), [value(7), value(42)]);
    let Some(Binding::Variable(a)) = session.bindings.get(&name("A")).copied() else {
        panic!("A should remain a variable binding");
    };
    assert_eq!(session.globals.view().read(a), Ok(value(2)));
}

#[test]
fn user_defined_return_equivalent_is_rejected_outside_runtime_word_body_at_call_span() {
    let mut session = RuntimeDefinitionSession::new();
    session.publish_syntax("SYNTAX URETURN\nSTATEMENT\nEXPECT_END\nEMIT_RETURN\nENDS");

    let (sources, source_id) = source("URETURN");
    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect_err("top-level return must fail during source processing");

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::UserDefinedEvaluation {
            source: SourceWordEvaluationError::ReturnOutsideRuntimeWord { .. }
        })
    ));
    assert_eq!(
        error.primary_span(),
        Some(span(sources.view(), source_id, 0, 7))
    );
}
