use super::*;

#[test]
fn empty_source_compiles_to_halt_only_and_runs() {
    let (sources, id, unit) = compile("");
    let view = sources.view();

    assert_eq!(unit.entry(), address(0));
    assert_eq!(unit.entry_location(), location(&unit, 0));
    assert_eq!(unit.len(), 1);
    assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(view, id, 0, 0)))
    );

    let words = PublishedWords::new();
    let bindings = Bindings::new();
    let primitives = PrimitiveRegistry::new();
    let result = run_unit(
        &unit,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("halt-only unit should run");
    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), []);
    assert_eq!(result.instruction_count(), 1);
}

#[test]
fn line_boundary_only_source_runs_as_halt_only() {
    for source in ["\n", "\r", "\r\n", "\n\r\n\r"] {
        let (sources, id, unit) = compile(source);
        let eof = source.len();

        assert_eq!(unit.len(), 1);
        assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(sources.view(), id, eof, eof))),
            "{source:?} should map Halt to EOF"
        );
    }
}

#[test]
fn segmentation_collects_completed_statements_without_top_level_boundaries() {
    let (_sources, _id, segmented) = segment("RUN\n\nCALL F\r\n");

    assert_eq!(segmented.completed_statements().len(), 2);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [TokenKind::Name]
    );
    assert_eq!(
        token_kinds(segmented.completed_statements()[1].tokens()),
        [TokenKind::Name, TokenKind::Name]
    );
    assert_eq!(segmented.incomplete_tail(), []);
    assert!(matches!(segmented.terminal(), Terminal::Eof { .. }));
}

#[test]
fn segmentation_completes_final_non_empty_statement_at_eof() {
    let (_sources, _id, segmented) = segment("RUN F");

    assert_eq!(segmented.completed_statements().len(), 1);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [TokenKind::Name, TokenKind::Name]
    );
    assert_eq!(segmented.incomplete_tail(), []);
}

#[test]
fn segmentation_preserves_parenthesized_line_boundary_inside_statement() {
    let (_sources, _id, segmented) = segment("BIF A + (\nB)\nRUN");

    assert_eq!(segmented.completed_statements().len(), 2);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [
            TokenKind::Name,
            TokenKind::Name,
            TokenKind::Plus,
            TokenKind::LParen,
            TokenKind::LineBoundary,
            TokenKind::Name,
            TokenKind::RParen
        ]
    );
    assert_eq!(
        token_kinds(segmented.completed_statements()[1].tokens()),
        [TokenKind::Name]
    );
}

#[test]
fn segmentation_skips_statement_leading_rem_comments() {
    let (_sources, _id, segmented) = segment("REM ? @ あ\nrem trailing\r\nRUN");

    assert_eq!(segmented.completed_statements().len(), 1);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [TokenKind::Name]
    );
    assert_eq!(segmented.incomplete_tail(), []);
    assert!(matches!(segmented.terminal(), Terminal::Eof { .. }));
}

#[test]
fn segmentation_does_not_treat_continued_line_rem_as_comment() {
    let (sources, id, segmented) = segment("BIF A + (\nREM)\nRUN");

    assert_eq!(segmented.completed_statements().len(), 2);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [
            TokenKind::Name,
            TokenKind::Name,
            TokenKind::Plus,
            TokenKind::LParen,
            TokenKind::LineBoundary,
            TokenKind::Name,
            TokenKind::RParen
        ]
    );
    assert_eq!(
        sources
            .view()
            .slice(segmented.completed_statements()[0].tokens()[5].span()),
        Ok("REM")
    );
    assert_eq!(
        token_kinds(segmented.completed_statements()[1].tokens()),
        [TokenKind::Name]
    );
    assert_eq!(
        segmented.completed_statements()[0].tokens()[5]
            .span()
            .source_id(),
        id
    );
}

#[test]
fn segmentation_does_not_treat_nonleading_rem_as_comment() {
    let (sources, _id, segmented) = segment("PUTDEC REM\nRUN");

    assert_eq!(segmented.completed_statements().len(), 2);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [TokenKind::Name, TokenKind::Name]
    );
    assert_eq!(
        sources
            .view()
            .slice(segmented.completed_statements()[0].tokens()[0].span()),
        Ok("PUTDEC")
    );
    assert_eq!(
        sources
            .view()
            .slice(segmented.completed_statements()[0].tokens()[1].span()),
        Ok("REM")
    );
    assert_eq!(
        token_kinds(segmented.completed_statements()[1].tokens()),
        [TokenKind::Name]
    );
}

#[test]
fn segmentation_distinguishes_completed_prefix_from_lexical_failure() {
    let (sources, id, segmented) = segment("VAR SCORE\n!");

    assert_eq!(segmented.completed_statements().len(), 1);
    assert_eq!(
        token_kinds(segmented.completed_statements()[0].tokens()),
        [TokenKind::Name, TokenKind::Name]
    );
    assert_eq!(segmented.incomplete_tail(), []);
    assert_eq!(
        segmented.terminal(),
        Terminal::LexError(LexError::InvalidCharacter {
            span: span(sources.view(), id, 10, 11),
            character: '!',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        })
    );
}

#[test]
fn segmentation_keeps_unbounded_lexical_failure_prefix_as_incomplete_tail() {
    let (sources, id, segmented) = segment("VAR SCORE !");

    assert_eq!(segmented.completed_statements(), []);
    assert_eq!(
        token_kinds(segmented.incomplete_tail()),
        [TokenKind::Name, TokenKind::Name]
    );
    assert_eq!(
        segmented.terminal(),
        Terminal::LexError(LexError::InvalidCharacter {
            span: span(sources.view(), id, 10, 11),
            character: '!',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        })
    );
}

#[test]
fn standalone_integer_statements_are_rejected() {
    for source_text in ["100", "100 + 1"] {
        let (sources, id, error) = compile_with_operators_error(source_text);

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, 3),
                kind: CompileErrorKind::BareExpression,
            }),
            "{source_text:?} should not compile as a statement"
        );
    }
}

#[test]
fn standalone_integer_without_line_number_is_rejected() {
    let (sources, id, error) = compile_error("1");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 1),
            kind: CompileErrorKind::BareExpression,
        })
    );
}

#[test]
fn line_number_prefix_requires_a_statement_leading_name() {
    let (sources, id, error) = compile_error("100 1");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 3),
            kind: CompileErrorKind::BareExpression,
        })
    );
}

#[test]
fn top_level_bare_expression_is_rejected_before_expression_parsing() {
    let cases = [
        ("1 + 2 * 3", 0, 1),
        ("(1 + 2)", 0, 1),
        ("1 < 2", 0, 1),
        ("-1", 0, 1),
    ];

    for (source, start, end) in cases {
        let (sources, id, error) = compile_with_operators_error(source);
        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, start, end),
                kind: CompileErrorKind::BareExpression,
            }),
            "{source:?} should not compile as a top-level statement"
        );
    }
}

#[test]
fn unresolved_and_variable_leading_expression_inputs_are_not_rescued() {
    let mut globals = crate::global_variable::GlobalVariables::new();
    let variable = globals.allocate();
    let mut bindings = Bindings::new();
    bindings
        .insert_new(name("A"), Binding::Variable(variable))
        .expect("variable should register");

    let cases = [("MISSING + 1", 0, 7), ("A + 1", 0, 1)];

    for (input, start, end) in cases {
        let (sources, id) = source(input);
        let mut words = PublishedWords::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect_err("name-leading bare expression should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, start, end),
                kind: CompileErrorKind::BareExpression,
            }),
            "{input:?} should not be rescued as a top-level expression"
        );
    }
}

#[test]
fn local_line_number_prefixed_bare_expression_is_rejected() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (sources, id) = source("100 MISSING + 2\nBIF 1, 100");
    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_operators(&bindings, operators.lookup()),
    )
    .expect_err("line-number-prefixed bare expression should fail");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 4, 11),
            kind: CompileErrorKind::BareExpression,
        })
    );
}

#[test]
fn local_line_number_prefixed_missing_name_uses_word_resolution_error() {
    let (sources, id, error) = compile_error("100 MISSING");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 4, 11),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName,
            },
        })
    );
}

#[test]
fn local_line_number_prefixed_runtime_word_still_compiles() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, result) = run_with_bindings_and_operators(
        "100 PUSH7\nBIF 1, 100",
        &bindings,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn comments_do_not_change_statement_meaning_or_source_mapping() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    let push7_word = register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (sources, id, unit) = compile_with_bindings_and_operators(
        "# ? @ あ\r\nREM ignored\n100 PUSH7 # trailing\nBIF 0, 100 # at EOF",
        &bindings,
        operators.lookup(),
    );

    assert_eq!(unit.len(), 4);
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(push7_word))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::JumpIfZero(address(0)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(sources.view(), id, 27, 32)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 2)),
        Ok(Some(span(sources.view(), id, 44, 47)))
    );
}

#[test]
fn comment_only_lines_do_not_accept_local_line_number_prefixes() {
    let mut words = PublishedWords::new();
    let bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);

    for (source_text, start, end) in [("100 # comment", 0, 3), ("100 REM comment", 4, 7)] {
        let (sources, id) = source(source_text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect_err("comment-only line number should fail");

        match source_text {
            "100 # comment" => assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, start, end),
                    kind: CompileErrorKind::BareExpression,
                })
            ),
            "100 REM comment" => assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, start, end),
                    kind: CompileErrorKind::WordResolution {
                        source: WordResolutionError::UndefinedName,
                    },
                })
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn bif_zero_condition_jumps_to_forward_line_number() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, result) = run_with_bindings_and_operators(
        "100 BIF 0, 200\nPUSH7\n200 push7",
        &bindings,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn bif_nonzero_condition_falls_through() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, result) = run_with_bindings_and_operators(
        "100 BIF 1, 200\nPUSH7\n200 push7",
        &bindings,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7), value(7)]);
}

#[test]
fn bif_condition_uses_expression_precedence_and_comparison() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, result) = run_with_bindings_and_operators(
        "BIF 1 + 2 * 3 <> 7, 200\nPUSH7\n200 push7",
        &bindings,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn bif_resolves_backward_line_number_without_cross_space_lookup() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, unit) =
        compile_with_bindings_and_operators("100 push7\nBIF 1, 100", &bindings, operators.lookup());

    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::JumpIfZero(address(0)))
    );
}

#[test]
fn unused_local_line_number_definition_is_accepted() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, result) = run_with_bindings_and_operators(
        "100 push7",
        &bindings,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn physical_line_integer_inside_parenthesized_continuation_is_not_line_number() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let (_sources, _id, result) = run_with_bindings_and_operators(
        "BIF (1 +\n2) = 4, 200\nPUSH7\n200 push7",
        &bindings,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn bif_expression_operator_calls_map_to_operator_source_spans() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let push7_id = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(push7_id))
        .expect("primitive word should register");
    let (sources, id, unit) = compile_with_bindings_and_operators(
        "BIF 1 + 2 * 3, 100\n100 PUSH7",
        &bindings,
        operators.lookup(),
    );
    let view = sources.view();

    assert_eq!(
        unit.instructions().get(address(3)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Multiply)
        ))
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Add)
        ))
    );
    assert_eq!(
        unit.source_span(location(&unit, 3)),
        Ok(Some(span(view, id, 10, 11)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 4)),
        Ok(Some(span(view, id, 6, 7)))
    );
}

#[test]
fn bif_condition_lowers_builtin_variable_names_to_load_var() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    let variables = [
        register_test_global(&mut globals, &mut bindings, "A"),
        register_test_global(&mut globals, &mut bindings, "B"),
    ];
    let push7_id = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(push7_id))
        .expect("primitive word should register");

    let (sources, id, unit) = compile_with_bindings_and_operators(
        "BIF a + B, 100\n100 PUSH7",
        &bindings,
        operators.lookup(),
    );
    let view = sources.view();

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::LoadVar(variables[0]))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::LoadVar(variables[1]))
    );
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(view, id, 4, 5)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 1)),
        Ok(Some(span(view, id, 8, 9)))
    );
}

#[test]
fn bif_condition_lowers_user_published_variable_name_to_load_var() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("VAR source word should bootstrap");
    compile_with_var("VAR Score", &mut bindings, &mut globals, &source_words);
    let Some(Binding::Variable(score)) = bindings.get(&name("score")).copied() else {
        panic!("SCORE should be a published variable");
    };
    let push7_id = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(push7_id))
        .expect("primitive word should register");

    let (sources, id, unit) = compile_with_bindings_and_operators(
        "BIF score = 0, 100\n100 PUSH7",
        &bindings,
        operators.lookup(),
    );

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::LoadVar(score))
    );
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(sources.view(), id, 4, 9)))
    );
}

#[test]
fn definition_body_empty_slice_appends_no_return_or_publication() {
    let bindings = Bindings::new();
    let (_sources, _id, code) = compile_body("", DefinitionBodyCompileContext::new(&bindings));

    assert_eq!(code.len(), 0);
}

#[test]
fn definition_body_lowers_existing_runtime_word_call_with_source_mapping() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let call_target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("TARGET"), Binding::Word(call_target))
        .expect("runtime word should register");

    let (sources, id, code) = compile_body("target", DefinitionBodyCompileContext::new(&bindings));

    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Call(call_target))
    );
    assert_eq!(
        code.source_mapping()
            .source_span(code.instruction_view().location(address(0))),
        Ok(Some(span(sources.view(), id, 0, 6)))
    );
}

#[test]
fn definition_body_dispatches_source_word_through_binding_capability() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("source word should register");

    let (_sources, _id, code) = compile_body(
        "source_marker",
        DefinitionBodyCompileContext {
            bindings: &bindings,
            operators: None,
            source_words: Some(source_words.lookup()),
            local_references: None,
        },
    );

    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn definition_body_let_lowers_expression_to_published_builder() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = [
        register_test_global(&mut globals, &mut bindings, "A"),
        register_test_global(&mut globals, &mut bindings, "B"),
    ];

    let (_sources, _id, code) = compile_body(
        "LET A = 1 + 2",
        DefinitionBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Add)
        ))
    );
    assert_eq!(
        code.instruction_view().get(address(3)),
        Ok(&Instruction::StoreVar(variables[0]))
    );
}

#[test]
fn definition_body_eval_lowers_expression_without_store() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");

    let (_sources, _id, code) = compile_body(
        "EVAL 1 + 2",
        DefinitionBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Add)
        ))
    );
    assert_eq!(code.len(), 3);
}

#[test]
fn definition_body_resolves_local_references_before_global_bindings() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let global_width = globals.allocate();
    bindings
        .insert_new(name("WIDTH"), Binding::Variable(global_width))
        .expect("global variable should register");
    let local_names = [name("WIDTH"), name("HEIGHT")];
    let local_references = DefinitionLocalReferences::from_names(&local_names);

    let (_sources, _id, code) = compile_body(
        "EVAL width + height",
        DefinitionBodyCompileContext::with_local_references(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            &local_references,
        ),
    );

    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::CopyFromCallBase { offset: 2 })
    );
    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::CopyFromCallBase { offset: 1 })
    );
    assert_eq!(
        code.instruction_view().get(address(2)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Add)
        ))
    );
    assert_eq!(
        bindings.get(&name("WIDTH")),
        Some(&Binding::Variable(global_width))
    );
}

#[test]
fn scratch_name_does_not_capture_array_reference_syntax() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let mut arrays = GlobalArrays::new();
    let array = arrays.allocate(1);
    bindings
        .insert_new(name("I"), Binding::Array(array))
        .expect("isolated resolver fixture should bind array I");
    let local_references = DefinitionLocalReferences::default();

    let (_sources, _id, code) = compile_body(
        "EVAL @I[1]",
        DefinitionBodyCompileContext::with_local_references(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            &local_references,
        ),
    );

    assert_eq!(
        code.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::LoadArrayElement(array))
    );
}

#[test]
fn definition_body_local_references_are_not_available_without_context() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("source words should bootstrap");
    let local_names = [name("VALUE")];
    let local_references = DefinitionLocalReferences::from_names(&local_names);
    let error = compile_body_error(
        "EVAL VALUE",
        DefinitionBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .2;

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::Expression {
            source: ExpressionError::Variable(_)
        })
    ));
    assert_eq!(local_references.resolve("VALUE"), Some(1));
}

#[test]
fn definition_body_var_fails_without_publication_capability() {
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let original_globals_len = globals.len();

    let (_sources, _id, error) = compile_body_error(
        "VAR SCORE",
        DefinitionBodyCompileContext {
            bindings: &bindings,
            operators: None,
            source_words: Some(source_words.lookup()),
            local_references: None,
        },
    );

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarPublicationContextUnavailable)
    );
    assert_eq!(bindings.get(&name("SCORE")), None);
    assert_eq!(globals.len(), original_globals_len);
}

#[test]
fn definition_body_rejects_bare_expression() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let bindings = Bindings::new();
    let (sources, id, error) = compile_body_error(
        "1 + 2",
        DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 1),
            kind: CompileErrorKind::BareExpression,
        })
    );
}

#[test]
fn definition_body_patches_forward_and_backward_local_line_numbers() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");

    let (_sources, _id, code) = compile_body(
        "100 BIF 0, 200\nPUSH7\n200 BIF 0, 100",
        DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        code.instruction_view().get(address(1)),
        Ok(&Instruction::JumpIfZero(address(3)))
    );
    assert_eq!(
        code.instruction_view().get(address(4)),
        Ok(&Instruction::JumpIfZero(address(0)))
    );
}

#[test]
fn definition_body_rejects_duplicate_and_undefined_local_line_numbers() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");
    let (duplicate_sources, duplicate_id, duplicate) = compile_body_error(
        "100 BIF 1, 100\n100 PUSH7",
        DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );
    let (undefined_sources, undefined_id, undefined) = compile_body_error(
        "BIF 1, 200",
        DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        duplicate,
        SourceProcessorError::Compile(CompileError {
            span: span(duplicate_sources.view(), duplicate_id, 15, 18),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Duplicate {
                    line_number: LocalLineNumber::new(100),
                    original_span: span(duplicate_sources.view(), duplicate_id, 0, 3),
                    duplicate_span: span(duplicate_sources.view(), duplicate_id, 15, 18),
                }),
            },
        })
    );
    assert_eq!(
        undefined,
        SourceProcessorError::Compile(CompileError {
            span: span(undefined_sources.view(), undefined_id, 7, 10),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Undefined {
                    line_number: LocalLineNumber::new(200),
                    span: span(undefined_sources.view(), undefined_id, 7, 10),
                }),
            },
        })
    );
}

#[test]
fn definition_body_unused_line_number_prefix_is_compile_time_only() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");
    let (first_sources, first_id, first_segmented) = segment("BIF 0, 100\n100 PUSH7");
    let (second_sources, second_id, second_segmented) = segment("100 PUSH7");
    let mut code = PublishedCode::new();

    compile_body_into(
        &mut code,
        first_sources.view(),
        first_id,
        &first_segmented,
        DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
    )
    .expect("first body should compile");
    let second_start = code.len();
    compile_body_into(
        &mut code,
        second_sources.view(),
        second_id,
        &second_segmented,
        DefinitionBodyCompileContext::with_operators(&bindings, operators.lookup()),
    )
    .expect("second body should compile independently");

    assert_eq!(
        code.instruction_view().get(address(second_start)),
        Ok(&Instruction::Call(target))
    );
}

#[test]
fn definition_body_unpublished_self_or_later_word_is_undefined() {
    let bindings = Bindings::new();
    for source_text in ["SELF", "LATER"] {
        let (sources, id, error) =
            compile_body_error(source_text, DefinitionBodyCompileContext::new(&bindings));
        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, source_text.len()),
                kind: CompileErrorKind::WordResolution {
                    source: WordResolutionError::UndefinedName,
                },
            }),
            "{source_text:?} should not receive a temporary body binding"
        );
    }
}

#[test]
fn quotation_body_empty_slice_completes_empty_static_quotation() {
    let bindings = Bindings::new();
    let (_sources, _id, quotation) =
        compile_quotation("", QuotationBodyCompileContext::new(&bindings));

    assert_eq!(quotation.len(), 0);
}

#[test]
fn quotation_body_lowers_existing_runtime_word_call_with_source_mapping() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let call_target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("TARGET"), Binding::Word(call_target))
        .expect("runtime word should register");

    let (sources, id, quotation) =
        compile_quotation("target", QuotationBodyCompileContext::new(&bindings));

    assert_eq!(
        quotation.instruction_view().get(address(0)),
        Ok(&Instruction::Call(call_target))
    );
    assert_eq!(
        quotation
            .source_mapping()
            .source_span(quotation_location(&quotation, 0)),
        Ok(Some(span(sources.view(), id, 0, 6)))
    );
}

#[test]
fn quotation_body_dispatches_source_word_through_binding_capability() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("source word should register");

    let (_sources, _id, quotation) = compile_quotation(
        "source_marker",
        QuotationBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert_eq!(
        quotation.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(99)))
    );
}

#[test]
fn quotation_body_does_not_provide_additional_source_capability() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("REQUEST"),
        request_additional_source_for_test,
    )
    .expect("source word should register");

    let (_sources, _id, error) = compile_quotation_error(
        "REQUEST library",
        QuotationBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(
            SourceWordError::AdditionalSourceProcessingUnavailable { .. }
        )
    ));
}

#[test]
fn builtin_use_in_quotation_body_does_not_provide_additional_source_capability() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");

    let (_sources, _id, error) = compile_quotation_error(
        "USE \"library\"",
        QuotationBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(
            SourceWordError::AdditionalSourceProcessingUnavailable { .. }
        )
    ));
}

#[test]
fn quotation_body_let_lowers_without_publication_capability() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = [
        register_test_global(&mut globals, &mut bindings, "A"),
        register_test_global(&mut globals, &mut bindings, "B"),
    ];

    let (_sources, _id, quotation) = compile_quotation(
        "LET A = 1 + 2",
        QuotationBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert_eq!(
        quotation.instruction_view().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        quotation.instruction_view().get(address(1)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        quotation.instruction_view().get(address(2)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Add)
        ))
    );
    assert_eq!(
        quotation.instruction_view().get(address(3)),
        Ok(&Instruction::StoreVar(variables[0]))
    );
}

#[test]
fn definition_local_references_are_visible_inside_quotations() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("source words should bootstrap");
    let local_names = [name("VALUE")];
    let local_references = DefinitionLocalReferences::from_names(&local_names);

    let (_sources, _id, quotation) = compile_quotation(
        "EVAL value",
        QuotationBodyCompileContext::with_local_references(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            &local_references,
        ),
    );

    assert_eq!(
        quotation.instruction_view().get(address(0)),
        Ok(&Instruction::CopyFromCallBase { offset: 1 })
    );
}

#[test]
fn quotation_body_var_fails_without_binding_or_global_publication() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let original_bindings_len = bindings.len();
    let original_globals_len = globals.len();

    let (_sources, _id, error) = compile_quotation_error(
        "VAR SCORE",
        QuotationBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::VarPublicationContextUnavailable)
    );
    assert_eq!(bindings.len(), original_bindings_len);
    assert_eq!(bindings.get(&name("SCORE")), None);
    assert_eq!(globals.len(), original_globals_len);
}

#[test]
fn quotation_body_def_fails_without_runtime_definition_publication() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let words = PublishedWords::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let original_bindings_len = bindings.len();

    let (sources, id, error) = compile_quotation_error(
        "DEF F\nEND",
        QuotationBodyCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    );

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::DefPublicationContextUnavailable {
            span: span(sources.view(), id, 0, 3),
        })
    );
    assert_eq!(bindings.len(), original_bindings_len);
    assert_eq!(bindings.get(&name("F")), None);
    assert_eq!(words.len(), 0);
}

#[test]
fn quotation_body_patches_forward_and_backward_local_line_numbers() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");

    let (_sources, _id, quotation) = compile_quotation(
        "100 BIF 0, 200\nPUSH7\n200 BIF 0, 100",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        quotation.instruction_view().get(address(1)),
        Ok(&Instruction::JumpIfZero(address(3)))
    );
    assert_eq!(
        quotation.instruction_view().get(address(4)),
        Ok(&Instruction::JumpIfZero(address(0)))
    );
}

#[test]
fn quotation_body_unused_line_number_prefix_is_compile_time_only() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");

    let (_sources, _id, quotation) = compile_quotation(
        "100 PUSH7",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        quotation.instruction_view().get(address(0)),
        Ok(&Instruction::Call(target))
    );
}

#[test]
fn quotation_body_rejects_duplicate_and_undefined_local_line_numbers() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");
    let (duplicate_sources, duplicate_id, duplicate) = compile_quotation_error(
        "100 BIF 1, 100\n100 PUSH7",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );
    let (undefined_sources, undefined_id, undefined) = compile_quotation_error(
        "BIF 1, 200",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        duplicate,
        SourceProcessorError::Compile(CompileError {
            span: span(duplicate_sources.view(), duplicate_id, 15, 18),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Duplicate {
                    line_number: LocalLineNumber::new(100),
                    original_span: span(duplicate_sources.view(), duplicate_id, 0, 3),
                    duplicate_span: span(duplicate_sources.view(), duplicate_id, 15, 18),
                }),
            },
        })
    );
    assert_eq!(
        undefined,
        SourceProcessorError::Compile(CompileError {
            span: span(undefined_sources.view(), undefined_id, 7, 10),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Undefined {
                    line_number: LocalLineNumber::new(200),
                    span: span(undefined_sources.view(), undefined_id, 7, 10),
                }),
            },
        })
    );
}

#[test]
fn quotation_body_line_number_scope_is_independent_per_quotation() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");

    let (_first_sources, _first_id, first) = compile_quotation(
        "100 PUSH7\nBIF 0, 100",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );
    let (_second_sources, _second_id, second) = compile_quotation(
        "100 PUSH7\nBIF 0, 100",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );
    let (_undefined_sources, _undefined_id, undefined) = compile_quotation_error(
        "BIF 0, 100",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );

    assert_eq!(
        first.instruction_view().get(address(2)),
        Ok(&Instruction::JumpIfZero(address(0)))
    );
    assert_eq!(
        second.instruction_view().get(address(2)),
        Ok(&Instruction::JumpIfZero(address(0)))
    );
    assert!(matches!(
        undefined,
        SourceProcessorError::Compile(CompileError {
            kind: CompileErrorKind::LineNumber { .. },
            ..
        })
    ));
}

#[test]
fn quotation_body_failure_returns_no_completed_artifact_and_next_build_can_succeed() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let target = words.add(completed_primitive(0));
    bindings
        .insert_new(name("PUSH7"), Binding::Word(target))
        .expect("runtime word should register");
    let (sources, id, error) = compile_quotation_error(
        "BIF 1, 200",
        QuotationBodyCompileContext::with_operators(&bindings, operators.lookup()),
    );
    let (_next_sources, _next_id, next) =
        compile_quotation("PUSH7", QuotationBodyCompileContext::new(&bindings));

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 7, 10),
            kind: CompileErrorKind::LineNumber {
                source: Box::new(LineNumberError::Undefined {
                    line_number: LocalLineNumber::new(200),
                    span: span(sources.view(), id, 7, 10),
                }),
            },
        })
    );
    assert_eq!(next.len(), 1);
    assert_eq!(
        next.instruction_view().get(address(0)),
        Ok(&Instruction::Call(target))
    );
}

#[test]
fn let_lowers_rhs_expression_then_store_var_with_source_mapping() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = [
        register_test_global(&mut globals, &mut bindings, "A"),
        register_test_global(&mut globals, &mut bindings, "B"),
    ];
    let (sources, id) = source("LET A = 1 + 2 * 3");

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect("LET should compile");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(1)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Push(value(3)))
    );
    assert_eq!(
        unit.instructions().get(address(3)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Multiply)
        ))
    );
    assert_eq!(
        unit.instructions().get(address(4)),
        Ok(&Instruction::Call(
            operators.lookup().resolve(OperatorSemantic::Add)
        ))
    );
    assert_eq!(
        unit.instructions().get(address(5)),
        Ok(&Instruction::StoreVar(variables[0]))
    );
    assert_eq!(unit.instructions().get(address(6)), Ok(&Instruction::Halt));
    assert!(!matches!(
        unit.instructions().get(address(0)),
        Ok(Instruction::Call(_))
    ));
    assert_eq!(
        unit.source_span(location(&unit, 5)),
        Ok(Some(span(sources.view(), id, 4, 5)))
    );
}

#[test]
fn let_updates_builtin_and_user_variables_with_case_insensitive_resolution() {
    let (words, primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = [
        register_test_global(&mut globals, &mut bindings, "A"),
        register_test_global(&mut globals, &mut bindings, "B"),
    ];
    compile_with_var("VAR Score", &mut bindings, &mut globals, &source_words);
    let Some(Binding::Variable(score)) = bindings.get(&name("score")).copied() else {
        panic!("SCORE should be a published variable");
    };

    globals
        .view_mut()
        .write(variables[1], value(4))
        .expect("B should be writable");
    run_with_source_words_operators_and_mut_globals(
        "let a = b + 2 * 3\nLET score = A",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(globals.view().read(variables[0]), Ok(value(10)));
    assert_eq!(globals.view().read(score), Ok(value(10)));
}

#[test]
fn source_to_vm_e2e_updates_builtin_variables_through_formal_expression_context() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();

    let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
        "LET A = 40 + 2\nLET B = A + 1",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), []);
    assert_eq!(globals.view().read(variables[0]), Ok(value(42)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(43)));
}

#[test]
fn source_to_vm_e2e_uses_character_and_hex_literals_in_let_expressions() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();

    let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
        "LET A = ''' + 1\nLET B = A + $F1\nEVAL '''",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(globals.view().read(variables[0]), Ok(value(40)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(281)));
    assert_eq!(result.data_stack(), [value(39)]);
}

#[test]
fn source_to_vm_e2e_uses_triple_quote_in_definition_body() {
    let mut session = RuntimeDefinitionSession::new();
    session.publish_def("DEF QUOTE\nEVAL '''\nEND");
    let (sources, id) = source("EVAL QUOTE()");

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("definition containing triple quote should compile");
    let result = session
        .run_unit_with_published_code(&unit)
        .expect("definition containing triple quote should run");

    assert_eq!(result.data_stack(), [value(39)]);
}
