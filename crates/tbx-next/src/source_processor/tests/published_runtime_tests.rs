#[test]
fn block_reader_consumes_one_statement_without_outer_reprocessing() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        consume_one_following_statement,
    )
    .expect("block source word should register");
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("marker source word should register");
    let (sources, source_id) = source("BLOCK\nMISSING\nSOURCE_MARKER");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("consumed unresolved statement should not be reprocessed by outer loop");

    assert_eq!(unit.len(), 3);
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(99)))
    );
    assert_eq!(unit.instructions().get(address(2)), Ok(&Instruction::Halt));
}

#[test]
fn block_reader_consumes_multiple_statements_without_duplicates() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("BLOCK2"),
        consume_two_following_statements,
    )
    .expect("block source word should register");
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("marker source word should register");
    let (sources, source_id) = source("BLOCK2\nFIRST\nSECOND\nSOURCE_MARKER");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("both consumed statements should be skipped by outer loop");

    assert_eq!(unit.len(), 4);
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn block_reader_exposes_whole_statement_for_standalone_marker_detection() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        consume_standalone_marker,
    )
    .expect("block source word should register");
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("marker source word should register");
    let (sources, source_id) = source("BLOCK\nEND\nSOURCE_MARKER");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("standalone marker should be consumed by block reader");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(0)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn block_reader_keeps_multi_token_marker_candidate_as_statement() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        consume_standalone_marker,
    )
    .expect("block source word should register");
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("marker source word should register");
    let (sources, source_id) = source("BLOCK\nEND X\nSOURCE_MARKER");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("multi-token marker candidate should be available to the handler");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn block_reader_recognizes_owner_declared_marker_roles() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word_with_markers(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        classify_one_declared_block_item,
        vec![marker(
            "ELSE",
            SourceWordSyntaxMarkerRole::BlockContinuation,
        )],
    )
    .expect("block source word should register with marker");
    let (sources, source_id) = source("BLOCK\nelse");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("declared marker should be classified");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(10)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(sources.view(), source_id, 6, 10)))
    );
}

#[test]
fn block_reader_does_not_treat_undeclared_name_as_marker() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word_with_markers(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        classify_one_declared_block_item,
        vec![marker("ENDIF", SourceWordSyntaxMarkerRole::BlockTerminator)],
    )
    .expect("block source word should register with marker");
    let (sources, source_id) = source("BLOCK\nELSE");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("undeclared marker spelling should remain a statement");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
}

#[test]
fn block_reader_keeps_other_owner_marker_as_statement() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word_with_markers(
        &mut source_words,
        &mut bindings,
        name("OUTER"),
        consume_until_declared_terminator,
        vec![marker(
            "OUT_END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )],
    )
    .expect("outer source word should register with marker");
    register_native_source_word_with_markers(
        &mut source_words,
        &mut bindings,
        name("INNER"),
        classify_one_declared_block_item,
        vec![marker(
            "INNER_END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )],
    )
    .expect("inner source word should register with a distinct marker");
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("source word should register");
    let (sources, source_id) = source("OUTER\nINNER_END\nOUT_END\nSOURCE_MARKER");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("outer reader should classify only its own markers");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(20)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn block_reader_reports_eof_span_for_missing_terminator_errors() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        read_eof_terminal_as_missing_terminator,
    )
    .expect("block source word should register");
    let (sources, source_id) = source("BLOCK");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect_err("missing block terminator should use EOF terminal span");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::UnsupportedSourceWord {
            span: span(sources.view(), source_id, 5, 5)
        })
    );
}

#[test]
fn block_reader_does_not_convert_lexical_failure_terminal_to_eof() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("BLOCK"),
        observe_lex_terminal_without_converting_to_eof,
    )
    .expect("block source word should register");
    let (sources, source_id) = source("BLOCK\n!");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect_err("lexical failure should remain the source terminal error");

    assert!(matches!(
        error,
        SourceProcessorError::Lex(LexError::InvalidCharacter { .. })
    ));
}

#[test]
fn structured_source_word_body_uses_processor_owned_forward_traversal() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let body_word = publish_initial(&mut words, &mut bindings, "BODY", completed_primitive(7));
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_structured_probe,
        vec![marker("END", SourceWordSyntaxMarkerRole::BlockTerminator)],
        structured_grammar(Vec::new(), "END"),
    );
    let (sources, source_id) = source("BLOCK\nBODY\nEND");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("structured source word should compile");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(10)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(body_word))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Push(value(30)))
    );
    assert_eq!(unit.instructions().get(address(3)), Ok(&Instruction::Halt));
}

#[test]
fn structured_source_word_body_does_not_provide_additional_source_capability() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "REQUEST_BLOCK",
        start_requesting_structured_source_word,
        Vec::new(),
        structured_grammar(Vec::new(), "END"),
    );
    let (sources, source_id) = source("REQUEST_BLOCK library");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect_err("structured source word body should reject the capability");

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(
            SourceWordError::AdditionalSourceProcessingUnavailable { .. }
        )
    ));
}

#[test]
fn builtin_use_in_structured_body_does_not_provide_additional_source_capability() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    publish_test_if(
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let (sources, source_id) = source("IF 1\nUSE \"library\"\nENDIF");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("USE in a structured body should reject the capability");

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(
            SourceWordError::AdditionalSourceProcessingUnavailable { .. }
        )
    ));
}

#[test]
fn structured_body_context_can_select_owner_local_build_target() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let body_word = publish_initial(&mut words, &mut bindings, "BODY", completed_primitive(7));
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_owner_local_target_probe,
        vec![marker("END", SourceWordSyntaxMarkerRole::BlockTerminator)],
        structured_grammar(Vec::new(), "END"),
    );
    let (sources, source_id) = source("BLOCK\nBODY\nEND\nBODY");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("owner-local body target should compile");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(body_word))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(body_word))
    );
    assert_eq!(unit.instructions().get(address(2)), Ok(&Instruction::Halt));
}

#[test]
fn nested_child_completion_inside_owner_local_target_stays_in_enclosing_target() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "OUTER",
        start_owner_local_target_probe,
        vec![marker(
            "OUT_END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )],
        structured_grammar(Vec::new(), "OUT_END"),
    );
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "INNER",
        start_structured_probe,
        vec![marker(
            "IN_END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )],
        structured_grammar(Vec::new(), "IN_END"),
    );
    let (sources, source_id) = source("OUTER\nINNER\nIN_END\nOUT_END");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("nested child completion should stay inside parent owner-local target");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(10)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(30)))
    );
    assert_eq!(unit.instructions().get(address(2)), Ok(&Instruction::Halt));
}

#[test]
fn structured_owner_can_commit_distinct_owner_local_targets_after_marker_switch() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let first_word = publish_initial(&mut words, &mut bindings, "FIRST", completed_primitive(7));
    let second_word = publish_initial(&mut words, &mut bindings, "SECOND", completed_primitive(8));
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_split_owner_local_targets_probe,
        vec![
            marker("ELSE", SourceWordSyntaxMarkerRole::BlockContinuation),
            marker("END", SourceWordSyntaxMarkerRole::BlockTerminator),
        ],
        structured_grammar(vec![("ELSE", MarkerCardinality::Optional)], "END"),
    );
    let (sources, source_id) = source("BLOCK\nFIRST\nELSE\nSECOND\nEND");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("owner should commit both owner-local body targets");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(first_word))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(20)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(second_word))
    );
    assert_eq!(unit.instructions().get(address(3)), Ok(&Instruction::Halt));
}

#[test]
fn structured_completion_failure_does_not_commit_owner_local_target() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    publish_initial(&mut words, &mut bindings, "BODY", completed_primitive(7));
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_failing_owner_local_target_probe,
        vec![marker("END", SourceWordSyntaxMarkerRole::BlockTerminator)],
        structured_grammar(Vec::new(), "END"),
    );
    let (sources, source_id) = source("BLOCK\nBODY\nEND");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect_err("owner completion failure should reject the structured instance");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::UnsupportedSourceWord {
            span: span(sources.view(), source_id, 11, 14)
        })
    );
}

#[test]
fn structured_current_owner_marker_uses_grammar_without_binding_fallback() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_structured_probe,
        vec![
            marker("ELSE", SourceWordSyntaxMarkerRole::BlockContinuation),
            marker("END", SourceWordSyntaxMarkerRole::BlockTerminator),
        ],
        structured_grammar(vec![("ELSE", MarkerCardinality::Optional)], "END"),
    );
    let (sources, source_id) = source("BLOCK\nELSE\nELSE\nEND");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect_err("second optional marker should be a grammar error");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::StructuredGrammar {
            span: span(sources.view(), source_id, 11, 15),
            source: crate::structured_grammar::GrammarProgressError::CardinalityExceeded {
                group_index: 0
            },
        })
    );
}

#[test]
fn nested_structured_source_word_isolates_ancestor_markers_until_child_completes() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let outer_end_word =
        publish_initial(&mut words, &mut bindings, "OUT_END", completed_primitive(8));
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "OUTER",
        start_structured_probe,
        vec![marker(
            "OUT_END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )],
        structured_grammar(Vec::new(), "OUT_END"),
    );
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "INNER",
        start_structured_probe,
        vec![marker(
            "IN_END",
            SourceWordSyntaxMarkerRole::BlockTerminator,
        )],
        structured_grammar(Vec::new(), "IN_END"),
    );
    let (sources, source_id) = source("OUTER\nINNER\nOUT_END\nIN_END\nOUT_END");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("nested structured source words should compile");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(10)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(10)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(outer_end_word))
    );
    assert_eq!(
        unit.instructions().get(address(3)),
        Ok(&Instruction::Push(value(30)))
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Push(value(30)))
    );
}

#[test]
fn structured_body_context_can_remove_publication_capability() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "PROBE",
        start_no_publication_probe,
        vec![marker("END", SourceWordSyntaxMarkerRole::BlockTerminator)],
        structured_grammar(Vec::new(), "END"),
    );
    let (sources, source_id) = source("PROBE\nVAR A\nEND");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_word_publication(
            &mut bindings,
            source_words.lookup(),
            &mut globals,
        ),
    )
    .expect_err("body VAR should fail through capability, not spelling special-case");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarPublicationContextUnavailable)
    );
    assert_eq!(bindings.get(&name("A")), None);
    assert_eq!(globals.len(), 0);
}

#[test]
fn structured_marker_can_switch_owner_local_line_number_scope() {
    let mut words = PublishedWords::new();
    let mut primitives = PrimitiveRegistry::new();
    let mut bindings = Bindings::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    publish_initial(&mut words, &mut bindings, "BODY", completed_primitive(7));
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_split_scope_probe,
        vec![
            marker("ELSE", SourceWordSyntaxMarkerRole::BlockContinuation),
            marker("END", SourceWordSyntaxMarkerRole::BlockTerminator),
        ],
        structured_grammar(vec![("ELSE", MarkerCardinality::Optional)], "END"),
    );
    let (sources, source_id) = source("BLOCK\n10 BODY\nBIF 1, 10\nELSE\n10 BODY\nBIF 1, 10\nEND");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("same local line number should be valid in separate owner scopes");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(10)))
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Push(value(20)))
    );
    assert_eq!(
        unit.instructions().get(address(8)),
        Ok(&Instruction::Push(value(30)))
    );
}

#[test]
fn structured_reused_owner_local_scope_resolves_new_patches_incrementally() {
    let mut words = PublishedWords::new();
    let mut primitives = PrimitiveRegistry::new();
    let mut bindings = Bindings::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7 = primitives.register(push_7);
    let push5 = primitives.register(push_5);
    let fail = primitives.register(fail_after_partial_stack_update);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
        .expect("PUSH7 primitive should register");
    register_primitive(&mut words, &mut bindings, name("PUSH5"), push5)
        .expect("PUSH5 primitive should register");
    register_primitive(&mut words, &mut bindings, name("FAIL"), fail)
        .expect("FAIL primitive should register");
    let mut source_words = SourceWordRegistry::new();
    register_structured_probe(
        &mut source_words,
        &mut bindings,
        "BLOCK",
        start_owner_local_target_probe,
        vec![
            marker("ELSE", SourceWordSyntaxMarkerRole::BlockContinuation),
            marker("END", SourceWordSyntaxMarkerRole::BlockTerminator),
        ],
        structured_grammar(vec![("ELSE", MarkerCardinality::Optional)], "END"),
    );
    let (sources, source_id) = source(
        "BLOCK\nBIF 0, 10\nFAIL\n10 PUSH7\nBIF 1, 10\nELSE\nBIF 0, 20\nFAIL\n20 PUSH5\nBIF 1, 20\nEND",
    );

    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("reused owner-local scope should resolve marker and terminator patches");

    assert_eq!(result.data_stack(), [value(7), value(20), value(5)]);
}

#[test]
fn builtin_if_runs_true_false_and_elsif_paths() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();

    run_with_source_words_operators_and_mut_globals(
        "IF 1\nLET A = 11\nELSE\nLET A = 22\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(11)));

    run_with_source_words_operators_and_mut_globals(
        "IF 0\nLET B = 11\nELSE\nLET B = 22\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[1]), Ok(value(22)));

    run_with_source_words_operators_and_mut_globals(
        "IF 0\nLET C = 11\nELSIF 1\nLET C = 33\nELSE\nLET C = 44\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[2]), Ok(value(33)));
}

#[test]
fn builtin_if_source_to_vm_covers_simple_false_and_multiple_elsif_selection() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();

    run_with_source_words_operators_and_mut_globals(
        "IF 1\nLET A = 1\nENDIF\nIF 0\nLET B = 1\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(1)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(0)));

    run_with_source_words_operators_and_mut_globals(
        "IF 0\nLET C = 10\nELSIF 0\nLET C = 20\nELSIF 1\nLET C = 30\nELSE\nLET C = 40\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[2]), Ok(value(30)));
}

#[test]
fn builtin_if_accepts_empty_branches_and_nested_if() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();

    run_with_source_words_operators_and_mut_globals(
        "IF 0\nELSIF 0\nELSE\nENDIF\nIF 1\nIF 0\nLET A = 10\nELSE\nLET A = 20\nENDIF\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(20)));
}

#[test]
fn builtin_if_quotation_local_line_number_branch_runs_inside_body_only() {
    let (mut words, mut primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    publish_test_if(
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let push7 = primitives.register(push_7);
    let fail = primitives.register(fail_after_partial_stack_update);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
        .expect("PUSH7 primitive should register");
    register_primitive(&mut words, &mut bindings, name("FAIL"), fail)
        .expect("FAIL primitive should register");
    let (sources, source_id) = source("IF 1\nBIF 0, 20\nFAIL\n20 PUSH7\nENDIF");

    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("quotation-local line-number branch should run inside the branch body");
    assert_eq!(result.data_stack(), [value(7)]);

    let (sources, source_id) = source("IF 1\nBIF 0, 99\nENDIF\n99 PUSH7");
    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("quotation-local branch should not resolve a parent line number");
    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), source_id, 12, 14),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Undefined {
                    line_number: LocalLineNumber::new(99),
                    span: span(sources.view(), source_id, 12, 14),
                }),
            },
        })
    );

    let (sources, source_id) = source("IF 0\nELSE\nBIF 0, 20\nFAIL\n20 PUSH7\nENDIF");
    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("same local line number should work in a later IF branch body");
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn builtin_if_rejects_parent_jump_into_branch_body_line_number() {
    let (mut words, mut primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    publish_test_if(
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let push7 = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
        .expect("PUSH7 primitive should register");
    let (sources, source_id) = source("BIF 0, 20\nIF 1\n20 PUSH7\nENDIF");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("parent scope should not resolve a quotation-local line number");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), source_id, 7, 9),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Undefined {
                    line_number: LocalLineNumber::new(20),
                    span: span(sources.view(), source_id, 7, 9),
                }),
            },
        })
    );
}

#[test]
fn builtin_if_allows_same_line_number_in_separate_branch_quotations() {
    let (mut words, mut primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    publish_test_if(
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let push7 = primitives.register(push_7);
    let push5 = primitives.register(push_5);
    let fail = primitives.register(fail_after_partial_stack_update);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
        .expect("PUSH7 primitive should register");
    register_primitive(&mut words, &mut bindings, name("PUSH5"), push5)
        .expect("PUSH5 primitive should register");
    register_primitive(&mut words, &mut bindings, name("FAIL"), fail)
        .expect("FAIL primitive should register");

    let (sources, source_id) = source("IF 1\n10 PUSH7\nELSE\nBIF 1, 10\nFAIL\n11 PUSH5\nENDIF");
    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("same local line number should be valid in separate IF branch quotations");
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn builtin_if_rejects_marker_payload_and_order_errors_at_local_span() {
    let (words, primitives, operators, source_words, bindings, _globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("IF 1\nELSE\nELSIF 1\nENDIF");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("ELSIF after ELSE should be rejected by common grammar");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::StructuredGrammar {
            span: span(sources.view(), source_id, 10, 17),
            source: crate::structured_grammar::GrammarProgressError::BackwardMarker {
                marker_group_index: 0,
                current_group_index: 1
            },
        })
    );

    let (sources, source_id) = source("IF 1\nELSE X\nENDIF");
    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("ELSE payload should be rejected by IF semantics");
    assert_eq!(
        error.primary_span(),
        Some(span(sources.view(), source_id, 10, 11))
    );

    drop((words, primitives));
}

#[test]
fn builtin_if_rejects_missing_condition_and_marker_line_number_prefix() {
    let (_words, _primitives, operators, source_words, bindings, _globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("IF 1\nELSIF\nENDIF");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("ELSIF requires a condition");
    assert_eq!(
        error.primary_span(),
        Some(span(sources.view(), source_id, 5, 10))
    );

    let (sources, source_id) = source("IF 1\n100 ELSE\nENDIF");
    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("line-number-prefixed marker should not classify as a marker");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), source_id, 9, 13),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName
            },
        })
    );
}

#[test]
fn builtin_if_body_cannot_publish_and_failed_if_does_not_poison_next_compile() {
    let (_words, _primitives, operators, source_words, mut bindings, mut globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("IF 1\nVAR SCORE\nENDIF");

    let _unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_word_publication_and_operators(
            &mut bindings,
            source_words.lookup(),
            operators.lookup(),
            &mut globals,
        ),
    )
    .expect("IF body should inherit publication capability");

    assert!(matches!(
        bindings.get(&name("SCORE")),
        Some(Binding::Variable(_))
    ));

    let (sources, source_id) = source("IF 1\nENDIF");
    compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("independent source should compile after failed IF");
}

#[test]
fn builtin_if_body_def_capability_failure_does_not_publish_runtime_definition() {
    let (mut words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    publish_test_if(
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );
    let mut code = PublishedCode::new();
    let initial_words_len = words.len();

    let (sources, source_id) = source("IF 1\nDEF INNER\nEND\nENDIF");
    let _unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_runtime_definition_publication_and_operators(
            &mut bindings,
            source_words.lookup(),
            operators.lookup(),
            &mut globals,
            &mut code,
            &mut words,
        ),
    )
    .expect("IF body should inherit runtime definition publication capability");

    assert!(matches!(
        bindings.get(&name("INNER")),
        Some(Binding::Word(_))
    ));
    assert!(words.len() > initial_words_len);
    assert!(code.len() > 0);

    compile_with_def(
        "DEF OK\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );
    assert!(matches!(bindings.get(&name("OK")), Some(Binding::Word(_))));
}

#[test]
fn failed_if_lexical_input_returns_no_unit_and_independent_source_runs() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();
    let (sources, source_id) = source("IF 1\nLET A = 2\n!");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("malformed IF source should not return a completed unit");
    assert!(matches!(error, SourceProcessorError::Lex(_)));

    run_with_source_words_operators_and_mut_globals(
        "IF 1\nLET A = 12\nENDIF",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(globals.view().read(variables[0]), Ok(value(12)));
}

#[test]
fn builtin_if_runtime_errors_map_to_original_branch_body_spans() {
    for (source_text, expected_start, expected_end) in [
        ("IF 1\nFAIL\nELSE\nENDIF", 5, 9),
        ("IF 0\nELSIF 1\nFAIL\nELSE\nENDIF", 13, 17),
        ("IF 0\nELSIF 0\nELSE\nFAIL\nENDIF", 18, 22),
    ] {
        let mut words = PublishedWords::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let fail = primitives.register(fail_after_partial_stack_update);
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut globals = GlobalVariables::new();
        publish_test_if(
            &mut bindings,
            &mut globals,
            &mut source_words,
            operators.lookup(),
        );
        register_primitive(&mut words, &mut bindings, name("FAIL"), fail)
            .expect("FAIL primitive should register");
        let (sources, source_id) = source(source_text);

        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect("IF source should compile");
        let error = run_unit(
            &unit,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("selected IF branch should fail at runtime");

        let SourceProcessorError::Runtime(error) = error else {
            panic!("expected runtime error");
        };
        assert_eq!(
            unit.source_span(error.vm().location()),
            Ok(Some(span(
                sources.view(),
                source_id,
                expected_start,
                expected_end,
            )))
        );
        assert_eq!(
            error.source_span(),
            Ok(Some(span(
                sources.view(),
                source_id,
                expected_start,
                expected_end,
            )))
        );
    }
}

#[test]
fn builtin_if_generated_jumps_use_branch_origin_spans() {
    let (_words, _primitives, operators, source_words, bindings, _globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("IF 1\nLET A = 2\nENDIF");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("IF should compile");

    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::JumpIfZero(address(4)))
    );
    assert_eq!(unit.instructions().get(address(4)), Ok(&Instruction::Halt));
    assert_eq!(
        unit.source_span(location(&unit, 1))
            .unwrap()
            .map(|span| (span.start(), span.end())),
        Some((65, 95))
    );
    assert_eq!(
        unit.source_span(location(&unit, 2))
            .unwrap()
            .map(|span| (span.start(), span.end())),
        Some((13, 14))
    );
}

#[test]
fn builtin_if_generated_jumps_map_if_elsif_and_merge_origins() {
    let (_words, _primitives, operators, source_words, bindings, _globals, _variables) =
        global_source_fixture();
    let (sources, source_id) =
        source("IF 0\nLET A = 1\nELSIF 1\nLET A = 2\nELSE\nLET A = 3\nENDIF");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("IF/ELSIF/ELSE should compile");

    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::JumpIfZero(address(5)))
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Jump(address(12)))
    );
    assert_eq!(
        unit.instructions().get(address(6)),
        Ok(&Instruction::JumpIfZero(address(10)))
    );
    assert_eq!(
        unit.instructions().get(address(9)),
        Ok(&Instruction::Jump(address(12)))
    );
    for (index, expected) in [
        (1, (65, 95)),
        (4, (111, 131)),
        (6, (203, 233)),
        (9, (253, 273)),
        (7, (31, 32)),
    ] {
        assert_eq!(
            unit.source_span(location(&unit, index))
                .unwrap()
                .map(|span| (span.start(), span.end())),
            Some(expected)
        );
    }
}

#[test]
fn builtin_if_merge_jump_targets_final_halt_instruction_not_executable_end() {
    let (_words, _primitives, operators, source_words, bindings, _globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("IF 1\nLET A = 2\nELSE\nLET A = 3\nENDIF");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("IF/ELSE should compile");

    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::JumpIfZero(address(5)))
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Jump(address(7)))
    );
    assert_eq!(unit.instructions().get(address(7)), Ok(&Instruction::Halt));
    assert_eq!(
        unit.source_span(location(&unit, 4))
            .unwrap()
            .map(|span| (span.start(), span.end())),
        Some((253, 273))
    );
}

#[test]
fn marker_reservation_blocks_publication_through_production_source_processing() {
    for reserved in ["END", "STATEMENT", "BLOCK", "ENDS"] {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let source_text = format!("VAR {reserved}");
        let (sources, source_id) = source(&source_text);

        let error = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_word_publication(
                &mut bindings,
                source_words.lookup(),
                &mut globals,
            ),
        )
        .expect_err("syntax-marker reservation should reject variable publication");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                span: span(sources.view(), source_id, 4, source_text.len())
            }),
            "{reserved} should be reserved by a structured source word"
        );
        assert_eq!(bindings.get(&name(reserved)), None);
        assert_eq!(globals.len(), 0);
    }
}

#[test]
fn syntax_body_uses_owned_marker_recognition_for_kind_lines() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    let (sources, source_id, error) = publish_user_source_word_error(
        "SYNTAX BROKEN\nBLOCK\nENDS",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxDefinition {
            span: span(sources.view(), source_id, 14, 19),
            kind: crate::source_word::SyntaxDefinitionErrorKind::MissingKind
        })
    );
    assert_eq!(bindings.get(&name("BROKEN")), None);
}

#[test]
fn builtin_if_can_call_published_runtime_word_from_branch_body() {
    let mut session = RuntimeDefinitionSession::new();
    session.register_primitive("PUSH7", push_7);
    session.publish_def("DEF TOUCH\nPUSH7\nEND");
    let (sources, source_id) = source("IF 1\nTOUCH\nENDIF");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("IF source should compile");
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
        ),
    )
    .expect("IF branch should execute published runtime word");

    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn nested_block_reader_fixture_advances_the_shared_cursor_once() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("NEST"),
        nested_reader_fixture,
    )
    .expect("nested source word should register");
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("marker source word should register");
    let (sources, source_id) = source("NEST\nINNER\nOUTER\nSOURCE_MARKER");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("inner and outer reader use should share one cursor");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn source_word_binding_does_not_lower_to_runtime_call() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("source word should register");

    let (_sources, _source_id, unit) = {
        let (sources, source_id) = source("source_marker");
        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("source word should compile");
        (sources, source_id, unit)
    };

    assert!(!matches!(
        unit.instructions().get(address(0)),
        Ok(Instruction::Call(_))
    ));
}

#[test]
fn source_word_binding_without_lookup_is_internal_context_error() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let source_word = register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("source word should register");
    let (sources, source_id) = source("source_marker");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::new(&bindings),
    )
    .expect_err("source word binding without lookup should fail as context error");

    assert_eq!(
        error,
        SourceProcessorError::SourceWordContextUnavailable { id: source_word }
    );
}

#[test]
fn variable_and_unresolved_leading_names_are_not_source_word_dispatch() {
    let mut globals = crate::global_variable::GlobalVariables::new();
    let variable = globals.allocate();
    let mut bindings = Bindings::new();
    bindings
        .insert_new(name("A"), Binding::Variable(variable))
        .expect("variable should register");

    let (sources, source_id) = source("A");
    let variable_error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::new(&bindings),
    )
    .expect_err("variable should not dispatch as source word");
    assert_eq!(
        variable_error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), source_id, 0, 1),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::TargetIsNotWord
            },
        })
    );

    let (sources, source_id) = source("MISSING");
    let unresolved_error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::new(&bindings),
    )
    .expect_err("unresolved name should not dispatch as source word");
    assert_eq!(
        unresolved_error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), source_id, 0, 7),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName
            },
        })
    );
}

#[test]
fn var_declares_global_variable_through_source_word_binding_without_runtime_instruction() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    let builtin = register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");

    let (_sources, _id, unit) =
        compile_with_var("VAR SCORE", &mut bindings, &mut globals, &source_words);

    let Some(Binding::Variable(id)) = bindings.get(&name("score")).copied() else {
        panic!("SCORE should be published as a variable");
    };
    assert_eq!(
        bindings.get(&name("VAR")),
        Some(&Binding::SourceWord(builtin.var()))
    );
    assert_eq!(
        bindings.get(&name("EVAL")),
        Some(&Binding::SourceWord(builtin.eval()))
    );
    assert_eq!(globals.view().read(id), Ok(value(0)));
    assert_eq!(globals.len(), 1);
    assert_eq!(unit.len(), 1);
    assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
}

#[test]
fn dim_publishes_normalized_zero_initialized_global_arrays() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut arrays = GlobalArrays::new();

    let (_sources, _id, unit) =
        compile_with_dim("DIM @MiXeD[1]", &mut bindings, &mut arrays, &source_words)
            .expect("minimum-size array should compile");
    let Some(Binding::Array(array)) = bindings.get(&name("mixed")).copied() else {
        panic!("DIM should publish an array binding using a normalized name");
    };
    assert_eq!(arrays.view().read(array, 0), Ok(value(0)));
    assert_eq!(arrays.len(), 1);
    assert_eq!(unit.len(), 1);

    compile_with_dim(
        "DIM @LARGE[32767]",
        &mut bindings,
        &mut arrays,
        &source_words,
    )
    .expect("maximum-size array should compile");
    let Some(Binding::Array(large)) = bindings.get(&name("large")).copied() else {
        panic!("DIM should publish the second array");
    };
    assert_ne!(array, large);
    assert_eq!(arrays.view().read(large, 32766), Ok(value(0)));
    assert_eq!(arrays.len(), 2);
}

#[test]
fn dim_rejects_invalid_sizes_and_syntax_without_publication_or_storage() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut arrays = GlobalArrays::new();

    let (sources, source_id) = source("DIM @A[0]");
    let mut globals = GlobalVariables::new();
    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_word_publication(
            &mut bindings,
            source_words.lookup(),
            &mut globals,
        )
        .with_global_arrays(&mut arrays),
    )
    .expect_err("zero-sized declaration should fail");
    assert_eq!(
        error.primary_span(),
        Some(span(sources.view(), source_id, 7, 8)),
        "size diagnostics should point at the invalid literal"
    );

    for (input, expected) in [
        ("DIM @A[0]", "size"),
        ("DIM @A[-1]", "size"),
        ("DIM @A[32768]", "size"),
        ("DIM @A[1+1]", "size"),
        ("DIM @A[$10]", "size"),
    ] {
        let error = compile_with_dim(input, &mut bindings, &mut arrays, &source_words)
            .expect_err("invalid array declaration should fail");
        assert!(
            matches!(
                error,
                SourceProcessorError::SourceWord(
                    crate::source_word::SourceWordError::DimSize { .. }
                )
            ),
            "{input} should report a size error, got {error:?} ({expected})"
        );
        assert!(bindings.get(&name("A")).is_none());
        assert_eq!(arrays.len(), 0);
    }

    let error = compile_with_dim("DIM @A[1", &mut bindings, &mut arrays, &source_words)
        .expect_err("missing closing bracket should fail");
    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(crate::source_word::SourceWordError::DimSyntax {
            kind: DimSyntaxErrorKind::MissingRightBracket,
            ..
        })
    ));
    assert_eq!(arrays.len(), 0);

    for input in ["DIM A[1]", "DIM @[1]", "DIM @A 1]", "DIM @A[1] EXTRA"] {
        let error = compile_with_dim(input, &mut bindings, &mut arrays, &source_words)
            .expect_err("malformed array declaration should fail");
        assert!(
            matches!(
                error,
                SourceProcessorError::SourceWord(
                    crate::source_word::SourceWordError::DimSyntax { .. }
                )
            ),
            "{input} should carry a syntax diagnostic: {error:?}"
        );
        assert_eq!(arrays.len(), 0);
        assert!(bindings.get(&name("A")).is_none());
    }

    compile_with_dim("DIM @A[2]", &mut bindings, &mut arrays, &source_words)
        .expect("valid declaration should recover after failed declarations");
    assert_eq!(arrays.len(), 1);
    let Some(Binding::Array(existing)) = bindings.get(&name("A")).copied() else {
        panic!("valid A declaration should remain published");
    };
    let conflict = compile_with_dim("DIM @A[3]", &mut bindings, &mut arrays, &source_words)
        .expect_err("array redefinition should fail");
    assert!(matches!(
        conflict,
        SourceProcessorError::SourceWord(
            crate::source_word::SourceWordError::DimNameConflict { .. }
        )
    ));
    assert_eq!(bindings.get(&name("A")), Some(&Binding::Array(existing)));
    assert_eq!(arrays.view().read(existing, 0), Ok(value(0)));
    assert_eq!(arrays.len(), 1);
}

#[test]
fn dim_rejects_single_namespace_and_syntax_marker_conflicts_before_allocation() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let word = WordId::test_invalid(0);
    bindings
        .insert_new(name("WORD"), Binding::Word(word))
        .unwrap();
    bindings
        .insert_new(
            name("SCALAR"),
            Binding::Variable(GlobalVarId::test_invalid(0)),
        )
        .unwrap();
    let marker_owner = source_words.register(|_| Ok(()));
    bindings
        .insert_new_source_word_with_markers(
            name("MARK_OWNER"),
            marker_owner,
            &[name("CUSTOM_MARK")],
        )
        .unwrap();
    let mut arrays = GlobalArrays::new();

    for input in [
        "DIM @WORD[1]",
        "DIM @SCALAR[1]",
        "DIM @DIM[1]",
        "DIM @CUSTOM_MARK[1]",
    ] {
        let error = compile_with_dim(input, &mut bindings, &mut arrays, &source_words)
            .expect_err("conflicting name should fail");
        assert!(
            matches!(
                error,
                SourceProcessorError::SourceWord(
                    crate::source_word::SourceWordError::DimNameConflict { .. }
                )
            ),
            "{input} should report a name conflict: {error:?}"
        );
        assert_eq!(arrays.len(), 0);
    }

    let reserved = compile_with_dim("DIM @REM[1]", &mut bindings, &mut arrays, &source_words)
        .expect_err("semantic reserved name should fail");
    assert!(matches!(
        reserved,
        SourceProcessorError::SourceWord(
            crate::source_word::SourceWordError::DimReservedName { .. }
        )
    ));
    assert_eq!(arrays.len(), 0);
}

#[test]
fn eval_source_word_binding_rejects_runtime_word_registration() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut words = PublishedWords::new();
    let builtin = register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");

    let result = register_primitive(
        &mut words,
        &mut bindings,
        name("EVAL"),
        PrimitiveId::from_slot(0),
    );

    assert_eq!(
        result,
        Err(crate::bootstrap::PrimitiveBootstrapError::NameConflict)
    );
    assert_eq!(
        bindings.get(&name("EVAL")),
        Some(&Binding::SourceWord(builtin.eval()))
    );
}

#[test]
fn var_uses_normalized_name_identity_for_mixed_case_declarations() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");

    compile_with_var("vAr Mixed_Case", &mut bindings, &mut globals, &source_words);

    let binding = bindings
        .get(&name("MIXED_CASE"))
        .copied()
        .expect("mixed-case declaration should publish");
    assert_eq!(bindings.get(&name("mixed_case")), Some(&binding));
    assert!(matches!(binding, Binding::Variable(_)));
}

#[test]
fn var_rejects_malformed_statements_with_primary_source_word_error_span() {
    for source_text in ["VAR", "VAR 123", "VAR SCORE EXTRA"] {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");

        let (sources, id, error) =
            compile_with_var_error(source_text, &mut bindings, &mut globals, &source_words);

        let expected = match source_text {
            "VAR" => SourceWordError::VarSyntax {
                span: span(sources.view(), id, 0, 3),
                kind: VarSyntaxErrorKind::MissingName,
            },
            "VAR 123" => SourceWordError::VarSyntax {
                span: span(sources.view(), id, 4, 7),
                kind: VarSyntaxErrorKind::MissingName,
            },
            "VAR SCORE EXTRA" => SourceWordError::VarSyntax {
                span: span(sources.view(), id, 10, 15),
                kind: VarSyntaxErrorKind::TrailingToken {
                    kind: TokenKind::Name,
                },
            },
            _ => unreachable!(),
        };
        assert_eq!(error, SourceProcessorError::SourceWord(expected));
        assert_eq!(globals.len(), 0, "{source_text:?} should not allocate");
        assert_eq!(bindings.get(&name("SCORE")), None);
    }
}

#[test]
fn var_rejects_duplicate_and_cross_kind_name_collisions_at_declared_name_span() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    compile_with_var("VAR SCORE", &mut bindings, &mut globals, &source_words);

    let (sources, id, error) =
        compile_with_var_error("VAR score", &mut bindings, &mut globals, &source_words);

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
            span: span(sources.view(), id, 4, 9)
        })
    );
    assert_eq!(globals.len(), 1);

    let mut words = PublishedWords::new();
    let mut runtime_bindings = Bindings::new();
    let mut runtime_globals = GlobalVariables::new();
    let primitive = PrimitiveId::from_slot(90);
    register_primitive(
        &mut words,
        &mut runtime_bindings,
        name("RUNTIME"),
        primitive,
    )
    .expect("runtime word should register");
    register_builtin_source_words(&mut source_words, &mut runtime_bindings)
        .expect("VAR should register after runtime word");
    let (sources, id, error) = compile_with_var_error(
        "VAR RUNTIME",
        &mut runtime_bindings,
        &mut runtime_globals,
        &source_words,
    );
    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
            span: span(sources.view(), id, 4, 11)
        })
    );
    assert_eq!(runtime_globals.len(), 0);
}

#[test]
fn var_rejects_existing_source_word_and_explicit_global_collisions_normally() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("VAR source word should bootstrap");
    register_test_global(&mut globals, &mut bindings, "A");

    for (source_text, start, end) in [("VAR VAR", 4, 7), ("VAR let", 4, 7), ("VAR A", 4, 5)] {
        let (sources, id, error) =
            compile_with_var_error(source_text, &mut bindings, &mut globals, &source_words);
        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                span: span(sources.view(), id, start, end)
            }),
            "{source_text:?} should be an ordinary binding collision"
        );
    }
    assert!(bindings.get(&name("VAR")).is_some());
    assert!(bindings.get(&name("LET")).is_some());
    assert!(bindings.get(&name("A")).is_some());
}

#[test]
fn single_letter_names_follow_ordinary_global_declaration_rules() {
    let (words, primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");

    for source_text in ["I", "LET I = 1"] {
        let (sources, id) = source(source_text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_word_publication(
                &mut bindings,
                source_words.lookup(),
                &mut globals,
            ),
        )
        .expect_err("undeclared single-letter global should fail");
        match source_text {
            "I" => {
                assert!(matches!(
                    &error,
                    SourceProcessorError::Compile(CompileError {
                        kind: CompileErrorKind::WordResolution {
                            source: WordResolutionError::UndefinedName
                        },
                        ..
                    })
                ));
                assert_eq!(error.primary_span(), Some(span(sources.view(), id, 0, 1)));
            }
            "LET I = 1" => {
                assert!(matches!(
                    &error,
                    SourceProcessorError::SourceWord(SourceWordError::LetTarget {
                        source: ExpressionVariableErrorKind::UndefinedName,
                        ..
                    })
                ));
                assert_eq!(error.primary_span(), Some(span(sources.view(), id, 4, 5)));
            }
            _ => unreachable!(),
        }
        assert_eq!(globals.len(), 0);
    }

    let (sources, id) = source("VAR I\nLET I = 1\nEVAL I");
    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_word_publication_and_operators(
            &mut bindings,
            source_words.lookup(),
            operators.lookup(),
            &mut globals,
        ),
    )
    .expect("VAR I should declare an ordinary global");
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
    .expect("declared single-letter global should read and write");
    assert_eq!(result.data_stack(), [value(1)]);
    assert_eq!(globals.len(), 1);

    let (sources, id) = source("VAR i");
    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_word_publication_and_operators(
            &mut bindings,
            source_words.lookup(),
            operators.lookup(),
            &mut globals,
        ),
    )
    .expect_err("a case-insensitive redeclaration should collide normally");
    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
            span: span(sources.view(), id, 4, 5)
        })
    );
    assert_eq!(globals.len(), 1);
}

#[test]
fn line_number_prefixed_var_is_rejected_before_publication() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("VAR source word should bootstrap");

    let (sources, id, error) =
        compile_with_var_error("10 VAR SCORE", &mut bindings, &mut globals, &source_words);

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarLocalLineNumberPrefix {
            span: span(sources.view(), id, 0, 2)
        })
    );
    assert_eq!(globals.len(), 0);
    assert_eq!(bindings.get(&name("SCORE")), None);
}

#[test]
fn successful_var_commit_survives_later_statement_and_completed_lexical_failures() {
    for source_text in ["VAR SCORE\nMISSING", "VAR SCORE\n!"] {
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("VAR source word should bootstrap");

        let (_sources, _id, error) =
            compile_with_var_error(source_text, &mut bindings, &mut globals, &source_words);

        assert!(
            matches!(
                error,
                SourceProcessorError::Compile(_) | SourceProcessorError::Lex(_)
            ),
            "{source_text:?} should fail after the completed VAR statement"
        );
        assert!(matches!(
            bindings.get(&name("SCORE")),
            Some(Binding::Variable(_))
        ));
        assert_eq!(globals.len(), 1);
    }
}

#[test]
fn same_statement_lexical_failure_does_not_commit_var() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("VAR source word should bootstrap");

    let (_sources, _id, error) =
        compile_with_var_error("VAR SCORE !", &mut bindings, &mut globals, &source_words);

    assert!(matches!(error, SourceProcessorError::Lex(_)));
    assert_eq!(bindings.get(&name("SCORE")), None);
    assert_eq!(globals.len(), 0);
}

#[test]
fn def_publishes_empty_runtime_word_with_return_mapped_to_end() {
    let (mut words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    let builtin = register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut code = PublishedCode::new();
    let initial_words_len = words.len();

    let (sources, id, unit) = compile_with_def(
        "def Foo\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );

    let Some(Binding::Word(foo)) = bindings.get(&name("FOO")).copied() else {
        panic!("FOO should be published as a runtime word");
    };
    assert_eq!(
        bindings.get(&name("DEF")),
        Some(&Binding::SourceWord(builtin.def()))
    );
    assert_eq!(words.len(), initial_words_len + 1);
    assert_eq!(
        words.get(foo).expect("published word should be defined"),
        &crate::word::WordDefinition::Compiled {
            entry: code.instruction_view().location(address(0))
        }
    );
    assert_eq!(code.len(), 1);
    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Return)
    );
    assert_eq!(
        code.source_mapping()
            .source_span(code.instruction_view().location(address(0))),
        Ok(Some(span(sources.view(), id, 8, 11)))
    );
    assert_eq!(unit.len(), 1);
    assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
}

#[test]
fn def_body_compiles_calls_before_appending_single_return() {
    let (mut words, mut primitives, operators) = operator_fixture();
    let primitive = primitives.register(push_7);
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    register_primitive(&mut words, &mut bindings, name("PUSH7"), primitive)
        .expect("runtime word should register");
    let mut globals = GlobalVariables::new();
    let mut code = PublishedCode::new();

    let (sources, id, _unit) = compile_with_def(
        "DEF FOO\npush7\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );

    assert_eq!(code.len(), 2);
    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(
            resolve_word_name(&bindings, "PUSH7").expect("PUSH7 should resolve")
        ))
    );
    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::Return)
    );
    assert_eq!(
        code.source_mapping()
            .source_span(code.instruction_view().location(address(1))),
        Ok(Some(span(sources.view(), id, 14, 17)))
    );
}

#[test]
fn def_rejects_header_errors_without_consuming_body_or_publishing() {
    for source_text in ["DEF", "DEF 123\nEND", "DEF FOO x y\nEND"] {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut code = PublishedCode::new();
        let initial_words_len = words.len();

        let (sources, id, error) = compile_with_def_error(
            source_text,
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        let expected = match source_text {
            "DEF" => SourceWordError::DefSyntax {
                span: span(sources.view(), id, 0, 3),
                kind: DefSyntaxErrorKind::MissingName,
            },
            "DEF 123\nEND" => SourceWordError::DefSyntax {
                span: span(sources.view(), id, 4, 7),
                kind: DefSyntaxErrorKind::MissingName,
            },
            "DEF FOO x y\nEND" => SourceWordError::DefSyntax {
                span: span(sources.view(), id, 10, 11),
                kind: DefSyntaxErrorKind::TrailingToken {
                    kind: TokenKind::Name,
                },
            },
            _ => unreachable!(),
        };
        assert_eq!(error, SourceProcessorError::SourceWord(expected));
        assert_eq!(bindings.get(&name("FOO")), None);
        assert_eq!(words.len(), initial_words_len);
        assert_eq!(code.len(), 0);
    }
}

#[test]
fn def_header_publishes_local_references_with_call_base_offsets() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    let multiply = resolve_word_name(&session.bindings, "MULTIPLY")
        .expect("MULTIPLY should be a named operator");
    session.publish_def("DEF AREA width, height\nEVAL width * height\nEND");

    let Some(Binding::Word(area)) = session.bindings.get(&name("AREA")).copied() else {
        panic!("AREA should be published");
    };
    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::CopyFromCallBase { offset: 2 })
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::CopyFromCallBase { offset: 1 })
    );
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(multiply))
    );
    assert_eq!(
        session.code.instruction_view().get(address(3)),
        Ok(&Instruction::Return)
    );

    let (caller_sources, caller_id) = source("EVAL AREA(6, 7)");
    let caller = compile_source(
        caller_sources.view(),
        caller_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("AREA caller should compile");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("AREA caller should run");
    // Header names are call-base references only; arguments remain on the
    // data stack because this issue does not add arity or cleanup rules.
    assert_eq!(result.data_stack(), [value(6), value(7), value(42)]);
    assert_eq!(
        caller.instructions().get(address(2)),
        Ok(&Instruction::Call(area))
    );
}

#[test]
fn compiled_word_scratch_is_independent_from_an_explicit_same_named_global() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    let global_i = register_test_global(&mut session.globals, &mut session.bindings, "I");
    session
        .globals
        .view_mut()
        .write(global_i, value(91))
        .expect("global I should accept a test value");
    let (definition_sources, definition_id, _) =
        session.publish_def("DEF SCRATCH\nLET I = 7\nEVAL I\nEND");

    let (sources, id) = source("EVAL SCRATCH()");
    let caller = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("scratch caller should compile");
    let code_spaces = [session.code.instruction_view()];
    let source_mappings = [session.code.source_mapping()];
    let result = run_unit(
        &caller,
        SourceExecutionContext::with_code_spaces_and_mappings(
            &session.bindings,
            &code_spaces,
            &source_mappings,
            PublishedWordLookup::new(&session.words),
            session.primitives.lookup(),
        )
        .with_globals(session.globals.view()),
    )
    .expect("compiled word should run");
    assert_eq!(result.data_stack(), [value(7)]);
    assert_eq!(session.globals.view().read(global_i), Ok(value(91)));
    let (top_sources, top_id) = source("EVAL I");
    let top_level = compile_source(
        top_sources.view(),
        top_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("top-level I should remain a global variable");
    assert_eq!(
        top_level.instructions().get(address(0)),
        Ok(&Instruction::LoadVar(global_i))
    );

    let code = session.code.instruction_view();
    assert_eq!(code.get(address(0)), Ok(&Instruction::Push(value(7))));
    assert_eq!(
        code.get(address(1)),
        Ok(&Instruction::StoreScratch(
            crate::instruction::ScratchSlotOperand::from_slot(crate::stack::ScratchSlot::I,)
        ))
    );
    assert_eq!(
        code.get(address(2)),
        Ok(&Instruction::LoadScratch(
            crate::instruction::ScratchSlotOperand::from_slot(crate::stack::ScratchSlot::I,)
        ))
    );
    assert_eq!(
        session
            .code
            .source_mapping()
            .source_span(code.location(address(1))),
        Ok(Some(span(definition_sources.view(), definition_id, 16, 17)))
    );
    assert_eq!(
        session
            .code
            .source_mapping()
            .source_span(code.location(address(2))),
        Ok(Some(span(definition_sources.view(), definition_id, 27, 28)))
    );
}

#[test]
fn def_header_rejects_scratch_local_reference_before_publication() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    let initial_words_len = session.words.len();

    let (_sources, _id, error) = session.publish_def_error("DEF BAD I\nMISSING\nEND");

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefLocalNameConflict { .. })
    ));
    assert_eq!(session.bindings.get(&name("BAD")), None);
    assert_eq!(session.words.len(), initial_words_len);
    assert_eq!(session.code.len(), 0);
}

#[test]
fn scratch_context_flows_through_structured_bodies_and_nested_calls() {
    let mut session = RuntimeDefinitionSession::new();
    session.publish_def("DEF CALLEE\nLET I = 2\nEND");
    session
        .publish_def("DEF CALLER\nLET I = 1\nIF 1\nCALLEE\nLET J = I + 3\nENDIF\nEVAL I + J\nEND");

    let (sources, id) = source("CALLER");
    let caller = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("caller should compile");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("nested scratch calls should run");

    // CALLEE owns its I and cannot overwrite CALLER's I. The IF body keeps the
    // same CALLER scratch context, so J receives caller I + 3.
    assert_eq!(result.data_stack(), [value(5)]);
}

#[test]
fn scratch_name_does_not_capture_statement_or_expression_word_calls() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    let word_i = session.register_primitive("I", push_7);
    session.publish_def("DEF CHECK\nLET I = 9\nI\nEVAL I()\nEVAL I\nEND");

    let (sources, id) = source("CHECK");
    let caller = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("word call fixture should compile");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("same-spelled calls should run");

    assert_eq!(result.data_stack(), [value(7), value(7), value(9)]);
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(word_i))
    );
    assert_eq!(
        session.code.instruction_view().get(address(3)),
        Ok(&Instruction::Call(word_i))
    );
}

#[test]
fn def_header_maps_one_local_reference_to_offset_one() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    session.publish_def("DEF FOO value\nEVAL value\nEND");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::CopyFromCallBase { offset: 1 })
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Return)
    );
}

#[test]
fn def_header_local_reference_is_available_in_while_condition() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    session.publish_syntax(
        "SYNTAX WHILE\nBLOCK\nSTART\nPOSITION AS loop_start\nREAD_EXPR AS condition\nEMIT_EXPR condition\nEMIT_BRANCH_IF_FALSE_COMPLETE\nLAST ENDWH\nEXPECT_END\nEMIT_BRANCH loop_start\nENDS",
    );
    session.publish_def("DEF LOOP value\nWHILE value\nENDWH\nEND");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::CopyFromCallBase { offset: 1 })
    );
}

#[test]
fn def_header_rejects_published_syntax_marker_local_reference_atomically() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    session.publish_syntax("SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS");
    let initial_words_len = session.words.len();

    let (_sources, _id, error) = session.publish_def_error("DEF BAD value, ENDWRAP\nEND");

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefLocalNameConflict { .. })
    ));
    assert_eq!(session.bindings.get(&name("BAD")), None);
    assert_eq!(session.words.len(), initial_words_len);
}

#[test]
fn def_header_supports_three_local_references_and_rejects_invalid_lists_atomically() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    session.publish_def("DEF MIX a, b, c\nEVAL a + b + c\nEND");
    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::CopyFromCallBase { offset: 3 })
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::CopyFromCallBase { offset: 2 })
    );
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(
            resolve_word_name(&session.bindings, "ADD").expect("ADD should resolve")
        ))
    );
    assert_eq!(
        session.code.instruction_view().get(address(3)),
        Ok(&Instruction::CopyFromCallBase { offset: 1 })
    );

    for source_text in [
        "DEF BAD x,\nEND",
        "DEF BAD , x\nEND",
        "DEF BAD x,,y\nEND",
        "DEF BAD x y\nEND",
        "DEF BAD x, END\nEND",
        "DEF BAD x, X\nEND",
    ] {
        let initial_words_len = session.words.len();
        let (_sources, _id, error) = session.publish_def_error(source_text);
        assert!(matches!(
            error,
            SourceProcessorError::SourceWord(
                SourceWordError::DefSyntax { .. }
                    | SourceWordError::DefLocalNameConflict { .. }
                    | SourceWordError::DefLocalReservedName { .. }
            )
        ));
        assert_eq!(session.bindings.get(&name("BAD")), None);
        assert_eq!(session.words.len(), initial_words_len);
    }
}

#[test]
fn def_rejects_name_conflicts_and_reserved_names_before_building_body() {
    let (mut words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    compile_with_var("VAR SCORE", &mut bindings, &mut globals, &source_words);
    let mut code = PublishedCode::new();
    let initial_words_len = words.len();

    let (sources, id, error) = compile_with_def_error(
        "DEF score\nMISSING\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );
    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefNameConflict {
            span: span(sources.view(), id, 4, 9)
        })
    );
    assert_eq!(code.len(), 0);
    assert_eq!(words.len(), initial_words_len);

    let (sources, id, error) = compile_with_def_error(
        "DEF END\nMISSING\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );
    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefNameConflict {
            span: span(sources.view(), id, 4, 7)
        })
    );
    assert_eq!(code.len(), 0);
    assert_eq!(words.len(), initial_words_len);

    let (sources, id, error) = compile_with_def_error(
        "DEF REM\nMISSING\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );
    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefReservedName {
            span: span(sources.view(), id, 4, 7)
        })
    );
    assert_eq!(code.len(), 0);
    assert_eq!(words.len(), initial_words_len);
}

#[test]
fn def_consumes_only_through_standalone_end_and_returns_outer_processing() {
    let (mut words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut code = PublishedCode::new();
    let push7 = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(push7))
        .expect("runtime word should register");

    let (_sources, _id, unit) = compile_with_def(
        "DEF FOO\nEND\nPUSH7",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );

    assert!(matches!(bindings.get(&name("FOO")), Some(Binding::Word(_))));
    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Return)
    );
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(push7))
    );
    assert_eq!(unit.instructions().get(address(1)), Ok(&Instruction::Halt));
}

#[test]
fn def_reports_missing_end_and_lexical_terminal_without_publication() {
    for source_text in ["DEF FOO", "DEF FOO\n!"] {
        let (mut words, _primitives, operators) = operator_fixture();
        let mut source_words = SourceWordRegistry::new();
        let mut bindings = Bindings::new();
        let mut globals = GlobalVariables::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("built-in source words should bootstrap");
        let mut code = PublishedCode::new();
        let initial_words_len = words.len();

        let (sources, id, error) = compile_with_def_error(
            source_text,
            &mut bindings,
            &mut globals,
            &source_words,
            operators.lookup(),
            &mut code,
            &mut words,
        );

        match source_text {
            "DEF FOO" => assert_eq!(
                error,
                SourceProcessorError::SourceWord(SourceWordError::DefMissingEnd {
                    span: span(sources.view(), id, 7, 7)
                })
            ),
            "DEF FOO\n!" => assert!(matches!(
                error,
                SourceProcessorError::SourceWord(SourceWordError::DefLex { .. })
            )),
            _ => unreachable!(),
        }
        assert_eq!(bindings.get(&name("FOO")), None);
        assert_eq!(words.len(), initial_words_len);
        assert_eq!(code.len(), 0);
    }
}

#[test]
fn def_body_failure_does_not_publish_and_later_definition_can_build() {
    let (mut words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut code = PublishedCode::new();
    let push7 = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(push7))
        .expect("runtime word should register");
    let initial_words_len = words.len();

    let (sources, id, error) = compile_with_def_error(
        "DEF BAD\nPUSH7\nMISSING\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefBodyCompile {
            span: span(sources.view(), id, 14, 21)
        })
    );
    assert_eq!(bindings.get(&name("BAD")), None);
    assert_eq!(words.len(), initial_words_len);
    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(push7))
    );

    compile_with_def(
        "DEF GOOD\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );
    assert!(matches!(
        bindings.get(&name("GOOD")),
        Some(Binding::Word(_))
    ));
    assert_eq!(words.len(), initial_words_len + 1);
    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::Return)
    );
}

#[test]
fn nested_def_body_fails_without_inner_publication_capability() {
    let (mut words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut code = PublishedCode::new();
    let initial_words_len = words.len();

    let (sources, id, error) = compile_with_def_error(
        "DEF OUTER\nDEF INNER\nEND\nEND",
        &mut bindings,
        &mut globals,
        &source_words,
        operators.lookup(),
        &mut code,
        &mut words,
    );

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefBodyCompile {
            span: span(sources.view(), id, 10, 13)
        })
    );
    assert_eq!(bindings.get(&name("OUTER")), None);
    assert_eq!(bindings.get(&name("INNER")), None);
    assert_eq!(words.len(), initial_words_len);
}

#[test]
fn def_without_runtime_publication_context_is_structured_error() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let (sources, id) = source("DEF FOO\nEND");

    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect_err("DEF should need runtime publication capability");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefPublicationContextUnavailable {
            span: span(sources.view(), id, 0, 3)
        })
    );
    assert_eq!(bindings.get(&name("FOO")), None);
}

#[test]
fn published_def_runs_from_later_source_without_merging_code_spaces() {
    let mut session = RuntimeDefinitionSession::new();
    let push7 = session.register_primitive("PUSH7", push_7);
    session.publish_def("DEF FOO\nPUSH7\nEND");
    let Some(Binding::Word(foo)) = session.bindings.get(&name("FOO")).copied() else {
        panic!("FOO should be published");
    };
    let (caller_sources, caller_id, caller) = session.compile_caller("foo");
    let original_published_len = session.code.len();

    let result = session
        .run_unit_with_published_code(&caller)
        .expect("later caller should execute published FOO");

    assert_eq!(
        caller.instructions().get(address(0)),
        Ok(&Instruction::Call(foo))
    );
    assert_eq!(
        caller.source_span(location(&caller, 0)),
        Ok(Some(span(caller_sources.view(), caller_id, 0, 3)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(push7))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Return)
    );
    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
    assert_eq!(result.instruction_count(), 2);
    assert_eq!(session.code.len(), original_published_len);
}

#[test]
fn published_def_body_eval_runs_and_feeds_following_runtime_word() {
    let mut session = RuntimeDefinitionSession::new();
    let add = session.register_primitive("ADD", add_top_two);
    session.publish_def("DEF SUM_SEVEN\nEVAL 2\nEVAL 5\nADD\nEND");
    let Some(Binding::Word(sum_seven)) = session.bindings.get(&name("SUM_SEVEN")).copied() else {
        panic!("SUM_SEVEN should be published");
    };

    let (_caller_sources, _caller_id, result) = session.run_caller("SUM_SEVEN");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Push(value(5)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(add))
    );
    assert_eq!(
        session.code.instruction_view().get(address(3)),
        Ok(&Instruction::Return)
    );
    assert_eq!(
        session.bindings.get(&name("SUM_SEVEN")),
        Some(&Binding::Word(sum_seven))
    );
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn published_def_body_can_call_named_operator_word() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators();
    let add = resolve_word_name(&session.bindings, "ADD").expect("ADD should be a named operator");
    session.publish_def("DEF SUM\nADD\nEND");
    let Some(Binding::Word(sum)) = session.bindings.get(&name("SUM")).copied() else {
        panic!("SUM should be published");
    };
    let (caller_sources, caller_id) = source("EVAL SUM(19, 23)");

    let caller = compile_source(
        caller_sources.view(),
        caller_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("caller should compile with named operator word");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("caller should run named operator word");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(add))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Return)
    );
    assert_eq!(
        caller.instructions().get(address(0)),
        Ok(&Instruction::Push(value(19)))
    );
    assert_eq!(
        caller.instructions().get(address(1)),
        Ok(&Instruction::Push(value(23)))
    );
    assert_eq!(
        caller.instructions().get(address(2)),
        Ok(&Instruction::Call(sum))
    );
    assert_eq!(result.data_stack(), [value(42)]);
}

#[test]
fn published_def_body_can_call_builtin_dup_runtime_word() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators_and_stack_primitives();
    let dup = resolve_word_name(&session.bindings, "DUP").expect("DUP should bootstrap");
    let multiply = resolve_word_name(&session.bindings, "MULTIPLY")
        .expect("MULTIPLY should be a named operator");
    session.publish_def("DEF SQUARE\nDUP\nMULTIPLY\nEND");
    let Some(Binding::Word(square)) = session.bindings.get(&name("SQUARE")).copied() else {
        panic!("SQUARE should be published");
    };
    let (caller_sources, caller_id) = source("EVAL SQUARE(7)");

    let caller = compile_source(
        caller_sources.view(),
        caller_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("caller should compile with DUP-backed runtime word");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("caller should run DUP-backed runtime word");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(dup))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Call(multiply))
    );
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Return)
    );
    assert_eq!(
        caller.instructions().get(address(1)),
        Ok(&Instruction::Call(square))
    );
    assert_eq!(result.data_stack(), [value(49)]);
}

#[test]
fn depth_reports_the_user_data_stack_in_source_and_compiled_word_paths() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators_and_stack_primitives();
    session.publish_def("DEF OBSERVE\nDEPTH\nEND");

    let (sources, source_id) = source("DEPTH\nDROP\nEVAL 10\nEVAL 20\nDEPTH\nOBSERVE");
    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("DEPTH should compile as a runtime word");
    let result = session
        .run_unit_with_published_code(&unit)
        .expect("DEPTH should run in source and compiled word paths");

    assert_eq!(
        result.data_stack(),
        [value(10), value(20), value(2), value(3)]
    );
}

#[test]
fn top_level_source_calls_print_and_cr_as_normal_runtime_words() {
    let session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let print = resolve_word_name(&session.bindings, "PUTDEC").expect("PUTDEC should bootstrap");
    let cr = resolve_word_name(&session.bindings, "CR").expect("CR should bootstrap");
    let (sources, id) = source("EVAL 42\nPUTDEC\nCR\nEVAL -7\nPUTDEC");
    let mut output = TestOutput::new();

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("PUTDEC and CR source should compile through normal word resolution");
    let result = session
        .run_unit_with_output(&unit, &mut output)
        .expect("PUTDEC and CR source should run with runtime output");

    let calls = (0..unit.len())
        .filter_map(|index| match unit.instructions().get(address(index)) {
            Ok(Instruction::Call(id)) => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.iter().filter(|id| **id == print).count(), 2);
    assert_eq!(calls.iter().filter(|id| **id == cr).count(), 1);
    assert_eq!(output.chunks(), ["42", "\n", "-7"]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn runtime_word_statement_evaluates_trailing_expression_before_call() {
    let session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let multiply = resolve_word_name(&session.bindings, "MULTIPLY")
        .expect("MULTIPLY should be a named operator");
    let add = resolve_word_name(&session.bindings, "ADD").expect("ADD should be a named operator");
    let print = resolve_word_name(&session.bindings, "PUTDEC").expect("PUTDEC should bootstrap");
    let (sources, id) = source("PUTDEC 2 + 3 * 4\nCR");
    let mut output = TestOutput::new();

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("runtime word trailing expression should compile");
    let result = session
        .run_unit_with_output(&unit, &mut output)
        .expect("runtime word trailing expression should run");

    let expected = [
        Instruction::Push(value(2)),
        Instruction::Push(value(3)),
        Instruction::Push(value(4)),
        Instruction::Call(multiply),
        Instruction::Call(add),
        Instruction::Call(print),
        Instruction::Call(resolve_word_name(&session.bindings, "CR").unwrap()),
    ];
    for (index, instruction) in expected.into_iter().enumerate() {
        assert_eq!(unit.instructions().get(address(index)), Ok(&instruction));
    }
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(sources.view(), id, 7, 8)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 5)),
        Ok(Some(span(sources.view(), id, 0, 6)))
    );
    assert_eq!(output.chunks(), ["14", "\n"]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn runtime_word_statement_accepts_literal_only_expression() {
    let session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let print = resolve_word_name(&session.bindings, "PUTDEC").expect("PUTDEC should bootstrap");
    let cr = resolve_word_name(&session.bindings, "CR").expect("CR should bootstrap");
    let (sources, id) = source("PUTDEC 2\nCR");
    let mut output = TestOutput::new();

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("literal-only trailing expression should compile");
    session
        .run_unit_with_output(&unit, &mut output)
        .expect("literal-only trailing expression should run");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(print))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(cr))
    );
    assert_eq!(output.chunks(), ["2", "\n"]);
}

#[test]
fn runtime_word_statement_accepts_variable_only_expression() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators_and_stack_primitives();
    let variable = session.globals.allocate();
    session
        .bindings
        .insert_new(name("A"), Binding::Variable(variable))
        .expect("variable should register");
    let dup = resolve_word_name(&session.bindings, "DUP").expect("DUP should bootstrap");
    let (sources, id) = source("DUP A");

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("variable-only trailing expression should compile");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::LoadVar(variable))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(dup))
    );
}

#[test]
fn runtime_word_trailing_expression_failure_does_not_commit_statement() {
    let session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let (sources, id) = source("PUTDEC 1 + MISSING");

    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect_err("unresolved trailing expression should fail compilation");

    assert!(matches!(
        error,
        SourceProcessorError::Compile(CompileError {
            kind: CompileErrorKind::ExpressionVariable {
                source: ExpressionVariableErrorKind::UndefinedName
            },
            ..
        })
    ));
}

#[test]
fn runtime_word_statement_expression_is_shared_by_definition_bodies() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    session.publish_def("DEF SHOW\nPUTDEC 2 + 3 * 4\nEND");
    let (caller_sources, caller_id, caller) = session.compile_caller("SHOW");
    let mut output = TestOutput::new();

    session
        .run_unit_with_published_code_and_output(&caller, &mut output)
        .expect("definition body should run its trailing expression");

    assert_eq!(output.chunks(), ["14"]);
    assert_eq!(
        caller.source_span(location(&caller, 0)),
        Ok(Some(span(caller_sources.view(), caller_id, 0, 4)))
    );
}

#[test]
fn top_level_eval_constant_print_cr_runs_e2e() {
    let session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let print = resolve_word_name(&session.bindings, "PUTDEC").expect("PUTDEC should bootstrap");
    let cr = resolve_word_name(&session.bindings, "CR").expect("CR should bootstrap");
    let (sources, id) = source("EVAL 42\nPUTDEC\nCR");
    let mut output = TestOutput::new();

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("constant EVAL, PUTDEC, and CR source should compile");
    let result = session
        .run_unit_with_output(&unit, &mut output)
        .expect("constant EVAL, PUTDEC, and CR source should run with test output");

    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(print))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(cr))
    );
    assert_eq!(output.chunks(), ["42", "\n"]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn top_level_empty_stack_print_runs_without_output_or_newline() {
    let session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let print = resolve_word_name(&session.bindings, "PUTDEC").expect("PUTDEC should bootstrap");
    let (sources, id) = source("PUTDEC");
    let mut output = TestOutput::new();

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("empty stack PUTDEC should compile through normal word resolution");
    let result = session
        .run_unit_with_output(&unit, &mut output)
        .expect("empty stack PUTDEC should run without output");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(print))
    );
    assert!(output.chunks().is_empty());
    assert_eq!(result.data_stack(), []);
}

#[test]
fn top_level_eval_prints_user_defined_runtime_word_result_e2e() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators_and_output_primitives();
    let multiply = resolve_word_name(&session.bindings, "MULTIPLY")
        .expect("MULTIPLY should be a named operator");
    let print = resolve_word_name(&session.bindings, "PUTDEC").expect("PUTDEC should bootstrap");
    let cr = resolve_word_name(&session.bindings, "CR").expect("CR should bootstrap");

    session.publish_def("DEF DOUBLE\nEVAL 2\nMULTIPLY\nEND");
    let Some(Binding::Word(double)) = session.bindings.get(&name("DOUBLE")).copied() else {
        panic!("DOUBLE should be published");
    };
    let (sources, id) = source("EVAL DOUBLE(21)\nPUTDEC\nCR\nEVAL 1\nPUTDEC");
    let mut output = TestOutput::new();

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("DOUBLE result printing source should compile");
    let result = session
        .run_unit_with_published_code_and_output(&unit, &mut output)
        .expect("DOUBLE result should run through EVAL, PUTDEC, CR, and test output");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Call(multiply))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(double))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(print))
    );
    assert_eq!(
        unit.instructions().get(address(3)),
        Ok(&Instruction::Call(cr))
    );
    assert_eq!(
        unit.instructions().get(address(5)),
        Ok(&Instruction::Call(print))
    );
    assert_eq!(
        unit.source_span(location(&unit, 1)),
        Ok(Some(span(sources.view(), id, 5, 11)))
    );
    assert_eq!(output.chunks(), ["42", "\n", "1"]);
    assert_eq!(result.data_stack(), []);
}

#[test]
fn minimal_user_defined_computation_words_run_e2e_from_later_eval_source() {
    let mut session = RuntimeDefinitionSession::new_with_named_operators_and_stack_primitives();
    let add = resolve_word_name(&session.bindings, "ADD").expect("ADD should be a named operator");
    let multiply = resolve_word_name(&session.bindings, "MULTIPLY")
        .expect("MULTIPLY should be a named operator");
    let greater = resolve_word_name(&session.bindings, "GREATER?")
        .expect("GREATER? should be a named operator");
    let dup = resolve_word_name(&session.bindings, "DUP").expect("DUP should bootstrap");

    session.publish_def("DEF SUM\nADD\nEND");
    session.publish_def("DEF DOUBLE\nEVAL 2\nMULTIPLY\nEND");
    session.publish_def("DEF SQUARE\nDUP\nMULTIPLY\nEND");
    session.publish_def("DEF IS_POSITIVE\nEVAL 0\nGREATER?\nEND");
    let Some(Binding::Word(sum)) = session.bindings.get(&name("SUM")).copied() else {
        panic!("SUM should be published");
    };
    let Some(Binding::Word(double)) = session.bindings.get(&name("DOUBLE")).copied() else {
        panic!("DOUBLE should be published");
    };
    let Some(Binding::Word(square)) = session.bindings.get(&name("SQUARE")).copied() else {
        panic!("SQUARE should be published");
    };
    let Some(Binding::Word(is_positive)) = session.bindings.get(&name("IS_POSITIVE")).copied()
    else {
        panic!("IS_POSITIVE should be published");
    };
    let (caller_sources, caller_id) = source(
        "EVAL SUM(19, 23)\n\
         EVAL DOUBLE(21)\n\
         EVAL SQUARE(-3)\n\
         EVAL IS_POSITIVE(5)\n\
         EVAL IS_POSITIVE(0)\n\
         EVAL IS_POSITIVE(-1)",
    );

    let caller = compile_source(
        caller_sources.view(),
        caller_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("later EVAL caller should compile");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("later EVAL caller should run all published computation words");

    assert_eq!(
        session.code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(add))
    );
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(3)),
        Ok(&Instruction::Call(multiply))
    );
    assert_eq!(
        session.code.instruction_view().get(address(5)),
        Ok(&Instruction::Call(dup))
    );
    assert_eq!(
        session.code.instruction_view().get(address(6)),
        Ok(&Instruction::Call(multiply))
    );
    assert_eq!(
        session.code.instruction_view().get(address(8)),
        Ok(&Instruction::Push(value(0)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(9)),
        Ok(&Instruction::Call(greater))
    );
    assert_eq!(
        caller.instructions().get(address(2)),
        Ok(&Instruction::Call(sum))
    );
    assert_eq!(
        caller.instructions().get(address(4)),
        Ok(&Instruction::Call(double))
    );
    assert_eq!(
        caller.instructions().get(address(7)),
        Ok(&Instruction::Call(square))
    );
    assert_eq!(
        caller.instructions().get(address(9)),
        Ok(&Instruction::Call(is_positive))
    );
    assert_eq!(
        caller.source_span(location(&caller, 4)),
        Ok(Some(span(caller_sources.view(), caller_id, 22, 28)))
    );
    assert_eq!(
        result.data_stack(),
        [value(42), value(42), value(9), value(1), value(0), value(0)]
    );
}

#[test]
fn published_def_body_can_call_existing_published_runtime_word() {
    let mut session = RuntimeDefinitionSession::new();
    session.register_primitive("PUSH3", push_3);
    session.register_primitive("PUSH4", push_4);
    session.publish_def("DEF BASE\nPUSH3\nEND");
    let Some(Binding::Word(base)) = session.bindings.get(&name("BASE")).copied() else {
        panic!("BASE should be published");
    };

    session.publish_def("DEF WRAP\nbase\nPUSH4\nEND");
    let Some(Binding::Word(wrap)) = session.bindings.get(&name("WRAP")).copied() else {
        panic!("WRAP should be published");
    };
    let (_caller_sources, _caller_id, caller) = session.compile_caller("wrap");
    let result = session
        .run_unit_with_published_code(&caller)
        .expect("nested published word should run");

    assert_eq!(
        caller.instructions().get(address(0)),
        Ok(&Instruction::Call(wrap))
    );
    assert_eq!(
        session.code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(base))
    );
    assert_eq!(result.data_stack(), [value(3), value(4)]);
}

#[test]
fn production_published_runtime_error_maps_to_definition_source_span() {
    let mut session = RuntimeDefinitionSession::new();
    session.register_primitive("PUSH7", push_7);
    let fail = session.register_primitive("FAIL", fail_after_partial_stack_update);
    let (definition_sources, definition_id, _unit) =
        session.publish_def("DEF BAD\nPUSH7\nfail\nEND");
    let (caller_sources, caller_id, caller) = session.compile_caller("bad");
    let published_views = [session.code.instruction_view()];
    let published_mappings = [session.code.source_mapping()];

    let error = run_unit(
        &caller,
        SourceExecutionContext::with_code_spaces_and_mappings(
            &session.bindings,
            &published_views,
            &published_mappings,
            PublishedWordLookup::new(&session.words),
            session.primitives.lookup(),
        ),
    )
    .expect_err("published body failure should fail caller");

    assert_eq!(
        caller.source_span(location(&caller, 0)),
        Ok(Some(span(caller_sources.view(), caller_id, 0, 3)))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Call(fail))
    );
    assert_runtime_error(
        error,
        session.code.instruction_view().location(address(1)),
        Ok(Some(span(definition_sources.view(), definition_id, 14, 18))),
    );
}

#[test]
fn failed_def_fragment_does_not_publish_and_later_def_runs_after_fragment() {
    let mut session = RuntimeDefinitionSession::new();
    session.register_primitive("PUSH5", push_5);
    session.register_primitive("PUSH7", push_7);
    let (keep_sources, keep_id, _keep_unit) = session.publish_def("DEF KEEP\nPUSH5\nEND");
    let Some(Binding::Word(keep)) = session.bindings.get(&name("KEEP")).copied() else {
        panic!("KEEP should be published");
    };
    let keep_entry = session.code.instruction_view().location(address(0));
    let keep_span = span(keep_sources.view(), keep_id, 9, 14);
    let words_len_before_failure = session.words.len();
    let code_len_before_failure = session.code.len();

    let (bad_sources, bad_id, error) = session.publish_def_error("DEF BAD\nPUSH7\nMISSING\nEND");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefBodyCompile {
            span: span(bad_sources.view(), bad_id, 14, 21)
        })
    );
    assert_eq!(session.bindings.get(&name("BAD")), None);
    assert_eq!(session.words.len(), words_len_before_failure);
    assert_eq!(
        session.words.get(keep).expect("KEEP should remain defined"),
        &crate::word::WordDefinition::Compiled { entry: keep_entry }
    );
    assert_eq!(
        session.code.source_mapping().source_span(keep_entry),
        Ok(Some(keep_span))
    );
    assert_eq!(
        session
            .code
            .instruction_view()
            .get(address(code_len_before_failure)),
        Ok(&Instruction::Call(
            resolve_word_name(&session.bindings, "PUSH7").expect("PUSH7 should resolve")
        ))
    );

    session.publish_def("DEF GOOD\nPUSH7\nEND");
    let Some(Binding::Word(good)) = session.bindings.get(&name("GOOD")).copied() else {
        panic!("GOOD should be published");
    };
    let expected_good_entry = session
        .code
        .instruction_view()
        .location(address(code_len_before_failure + 1));
    assert_eq!(
        session.words.get(good).expect("GOOD should be defined"),
        &crate::word::WordDefinition::Compiled {
            entry: expected_good_entry
        }
    );

    let (_keep_caller_sources, _keep_caller_id, keep_result) = session.run_caller("keep");
    let (_good_caller_sources, _good_caller_id, good_result) = session.run_caller("good");

    assert_eq!(keep_result.data_stack(), [value(5)]);
    assert_eq!(good_result.data_stack(), [value(7)]);
}

#[test]
fn published_code_redefinition_preserves_early_bound_callers_and_mappings() {
    let mut session = RuntimeDefinitionSession::new();
    session.register_primitive("PUSH41", push_41);
    let (old_sources, old_id, _old_def_unit) = session.publish_def("DEF TARGET\nPUSH41\nEND");
    let Some(Binding::Word(old)) = session.bindings.get(&name("TARGET")).copied() else {
        panic!("TARGET should be published");
    };
    let (_old_caller_sources, _old_caller_id, old_caller) = session.compile_caller("target");
    let old_entry = session.code.instruction_view().location(address(0));
    let old_span = span(old_sources.view(), old_id, 11, 17);

    let (new_sources, new_id) = source("99\nEND");
    let new_value_span = span(new_sources.view(), new_id, 0, 2);
    let new_end_span = span(new_sources.view(), new_id, 3, 6);
    let redefinition = session
        .code
        .redefine_word(
            &mut session.words,
            &mut session.bindings,
            &name("TARGET"),
            |builder| {
                builder.append_mapped(Instruction::Push(value(99)), new_value_span)?;
                builder.append_mapped(Instruction::Return, new_end_span)?;
                Ok(())
            },
        )
        .expect("TARGET should redefine in published code");
    let (_new_caller_sources, _new_caller_id, new_caller) = session.compile_caller("target");
    let new_entry = session.code.instruction_view().location(address(2));

    let old_result = session
        .run_unit_with_published_code(&old_caller)
        .expect("old caller should still run old body");
    let new_result = session
        .run_unit_with_published_code(&new_caller)
        .expect("new caller should run new body");

    assert_eq!(redefinition.previous(), old);
    assert_ne!(redefinition.previous(), redefinition.current());
    assert_eq!(
        old_caller.instructions().get(address(0)),
        Ok(&Instruction::Call(redefinition.previous()))
    );
    assert_eq!(
        new_caller.instructions().get(address(0)),
        Ok(&Instruction::Call(redefinition.current()))
    );
    assert_eq!(old_result.data_stack(), [value(41)]);
    assert_eq!(new_result.data_stack(), [value(99)]);
    assert_eq!(
        session
            .words
            .get(old)
            .expect("old word should remain defined"),
        &crate::word::WordDefinition::Compiled { entry: old_entry }
    );
    assert_eq!(
        session
            .words
            .get(redefinition.current())
            .expect("new word should be defined"),
        &crate::word::WordDefinition::Compiled { entry: new_entry }
    );
    assert_eq!(
        session.code.source_mapping().source_span(old_entry),
        Ok(Some(old_span))
    );
    assert_eq!(
        session.code.source_mapping().source_span(new_entry),
        Ok(Some(new_value_span))
    );
}

#[test]
fn case_variants_resolve_to_same_word_id_during_compile() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let id = publish_initial(&mut words, &mut bindings, "ready?", completed_primitive(2));

    let (_sources, _source_id, unit) = compile_with_bindings("ready?\nReady?\nREADY?", &bindings);

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(id))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(id))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(id))
    );
}

#[test]
fn primitive_and_compiled_words_use_same_resolve_and_emit_path() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut shared_code = InstructionSequence::new();
    let primitive = publish_initial(&mut words, &mut bindings, "PRIM", completed_primitive(3));
    let compiled = publish_initial(
        &mut words,
        &mut bindings,
        "USER_WORD",
        completed_compiled(&mut shared_code, 10),
    );

    let (_sources, _source_id, unit) = compile_with_bindings("prim\nuser_word", &bindings);

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(primitive))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(compiled))
    );
}

#[test]
fn saved_execution_unit_keeps_old_word_id_after_redefinition() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut shared_code = InstructionSequence::new();
    let old = publish_initial(&mut words, &mut bindings, "TARGET", completed_primitive(4));

    let (_old_sources, _old_source_id, old_unit) = compile_with_bindings("target", &bindings);
    let redefinition = redefine_word(
        &mut words,
        &mut bindings,
        &name("TARGET"),
        completed_compiled(&mut shared_code, 11),
    )
    .expect("existing word should redefine");
    let (_new_sources, _new_source_id, new_unit) = compile_with_bindings("target", &bindings);

    assert_eq!(redefinition.previous(), old);
    assert_ne!(redefinition.previous(), redefinition.current());
    assert_eq!(
        old_unit.instructions().get(address(0)),
        Ok(&Instruction::Call(redefinition.previous()))
    );
    assert_eq!(
        new_unit.instructions().get(address(0)),
        Ok(&Instruction::Call(redefinition.current()))
    );
    assert_eq!(
        old_unit.instructions().get(address(0)),
        Ok(&Instruction::Call(old))
    );
}

#[test]
fn undefined_name_is_span_compile_error_without_publication_mutation() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let id = publish_initial(&mut words, &mut bindings, "KNOWN", completed_primitive(5));
    primitives.register(push_7);
    let (sources, source_id) = source("known\nmissing");

    let error = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::new(&bindings),
    )
    .expect_err("undefined name should fail");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), source_id, 6, 13),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName
            },
        })
    );
    assert_eq!(words.len(), 1);
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings.get(&name("KNOWN")), Some(&Binding::Word(id)));
    assert_eq!(primitives.len(), 1);
}

#[test]
fn primitive_word_call_runs_from_temporary_execution_unit() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), primitive)
        .expect("primitive should register");
    let (sources, source_id) = source("push7");

    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("primitive call should run");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
    assert_eq!(result.instruction_count(), 2);
}

#[test]
fn compiled_word_call_runs_with_temporary_and_published_code_spaces() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let primitives = PrimitiveRegistry::new();
    let mut published_code = InstructionSequence::new();
    publish_initial(
        &mut words,
        &mut bindings,
        "USER_WORD",
        completed_compiled(&mut published_code, 10),
    );
    published_code.append(Instruction::Return);
    let (sources, source_id) = source("user_word");
    let published_views = [published_code.view()];

    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("published compiled word should run");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(10)]);
    assert_eq!(result.instruction_count(), 2);
}

#[test]
fn integer_literals_primitive_and_compiled_calls_run_in_source_order() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let mut published_code = InstructionSequence::new();
    let push2 = primitives.register(push_2);
    let primitive = primitives.register(add_top_two);
    publish_initial(
        &mut words,
        &mut bindings,
        "USER_WORD",
        completed_compiled(&mut published_code, 5),
    );
    published_code.append(Instruction::Return);
    register_primitive(&mut words, &mut bindings, name("PUSH2"), push2)
        .expect("primitive should register");
    register_primitive(&mut words, &mut bindings, name("ADD"), primitive)
        .expect("primitive should register");
    let (sources, source_id) = source("PUSH2\nuser_word\nadd");
    let published_views = [published_code.view()];

    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("mixed source should run");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn published_compiled_word_can_call_nested_compiled_words() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let primitives = PrimitiveRegistry::new();

    let mut inner_code = InstructionSequence::new();
    let inner_entry = inner_code.append(Instruction::Push(value(3)));
    inner_code.append(Instruction::Return);
    let inner = publish_initial(
        &mut words,
        &mut bindings,
        "INNER",
        completed_compiled_at(&inner_code, inner_entry),
    );

    let mut middle_code = InstructionSequence::new();
    let middle_entry = middle_code.append(Instruction::Call(inner));
    middle_code.append(Instruction::Push(value(4)));
    middle_code.append(Instruction::Return);
    let middle = publish_initial(
        &mut words,
        &mut bindings,
        "MIDDLE",
        completed_compiled_at(&middle_code, middle_entry),
    );

    let mut outer_code = InstructionSequence::new();
    let outer_entry = outer_code.append(Instruction::Call(middle));
    outer_code.append(Instruction::Push(value(5)));
    outer_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "OUTER",
        completed_compiled_at(&outer_code, outer_entry),
    );

    let (sources, source_id) = source("outer");
    let published_views = [inner_code.view(), middle_code.view(), outer_code.view()];

    let result = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("nested compiled call should run");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(3), value(4), value(5)]);
}

#[test]
fn saved_unit_runs_old_compiled_entry_after_redefinition() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let primitives = PrimitiveRegistry::new();

    let mut old_code = InstructionSequence::new();
    let old_entry = old_code.append(Instruction::Push(value(41)));
    old_code.append(Instruction::Return);
    let old = publish_initial(
        &mut words,
        &mut bindings,
        "TARGET",
        completed_compiled_at(&old_code, old_entry),
    );
    let (_old_sources, _old_source_id, old_unit) = compile_with_bindings("target", &bindings);

    let mut new_code = InstructionSequence::new();
    let new_entry = new_code.append(Instruction::Push(value(99)));
    new_code.append(Instruction::Return);
    let redefinition = redefine_word(
        &mut words,
        &mut bindings,
        &name("TARGET"),
        completed_compiled_at(&new_code, new_entry),
    )
    .expect("existing word should redefine");
    let (new_sources, new_source_id) = source("target");
    let published_views = [old_code.view(), new_code.view()];

    let old_result = run_unit(
        &old_unit,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("old early-bound unit should run");
    let new_result = run_source(
        new_sources.view(),
        new_source_id,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("newly compiled unit should run");

    assert_eq!(redefinition.previous(), old);
    assert_ne!(redefinition.previous(), redefinition.current());
    assert_eq!(
        old_unit.instructions().get(address(0)),
        Ok(&Instruction::Call(old))
    );
    assert_eq!(old_result.data_stack(), [value(41)]);
    assert_eq!(new_result.data_stack(), [value(99)]);
    assert_eq!(words.len(), 2);
}
use super::*;
