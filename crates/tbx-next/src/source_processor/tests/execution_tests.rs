#[test]
fn invalid_user_defined_statement_is_not_published() {
    let (_words, _primitives, operators, mut source_words, mut bindings, mut globals, _vars) =
        global_source_fixture();
    let (_sources, _id, error) = publish_user_source_word_error(
        "SYNTAX BROKEN\nSTATEMENT\nEMIT_STORE missing\nENDS\nBROKEN",
        &mut bindings,
        &mut globals,
        &mut source_words,
        operators.lookup(),
    );

    assert!(matches!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::SyntaxBuild { .. })
    ));
    assert_eq!(bindings.get(&name("BROKEN")), None);
}

#[test]
fn source_to_vm_e2e_publishes_user_var_for_later_let_in_same_source() {
    let (words, primitives, operators, source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    let (sources, id) = source("VAR SCORE\nLET SCORE = SCORE + 1\nLET A = SCORE");

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
    .expect("same-source VAR and LET statements should compile");
    let Some(Binding::Variable(score)) = bindings.get(&name("SCORE")).copied() else {
        panic!("SCORE should be published by the VAR statement");
    };

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
    .expect("same-source VAR and LET unit should run");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), []);
    assert_eq!(globals.view().read(score), Ok(value(1)));
    assert_eq!(globals.view().read(variables[0]), Ok(value(1)));
}

#[test]
fn source_to_vm_e2e_successful_var_survives_later_statement_failure() {
    let (_words, _primitives, operators, source_words, mut bindings, mut globals, _variables) =
        global_source_fixture();
    let original_globals_len = globals.len();
    let (sources, id) = source("VAR SCORE\nLET A = MISSING + 1");

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
    .expect_err("later LET RHS failure should fail the source");

    let SourceProcessorError::SourceWord(SourceWordError::Expression {
        source: ExpressionError::Variable(error),
    }) = error
    else {
        panic!("expected later LET RHS variable error");
    };
    assert_eq!(error.span(), span(sources.view(), id, 18, 25));
    assert_eq!(error.kind(), ExpressionVariableErrorKind::UndefinedName);
    let Some(Binding::Variable(score)) = bindings.get(&name("SCORE")).copied() else {
        panic!("completed VAR should remain published after later failure");
    };
    assert_eq!(globals.len(), original_globals_len + 1);
    assert_eq!(globals.view().read(score), Ok(value(0)));
}

#[test]
fn source_to_vm_e2e_failed_var_does_not_publish_binding_or_storage() {
    for source_text in ["VAR SCORE EXTRA", "VAR A"] {
        let (_words, _primitives, operators, source_words, mut bindings, mut globals, _variables) =
            global_source_fixture();
        let original_globals_len = globals.len();
        let (sources, id) = source(source_text);

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
        .expect_err("failed VAR statement should reject the source");

        match source_text {
            "VAR SCORE EXTRA" => {
                assert_eq!(
                    error,
                    SourceProcessorError::SourceWord(SourceWordError::VarSyntax {
                        span: span(sources.view(), id, 10, 15),
                        kind: VarSyntaxErrorKind::TrailingToken {
                            kind: TokenKind::Name,
                        },
                    })
                );
                assert_eq!(bindings.get(&name("SCORE")), None);
            }
            "VAR A" => {
                assert_eq!(
                    error,
                    SourceProcessorError::SourceWord(SourceWordError::VarNameConflict {
                        span: span(sources.view(), id, 4, 5),
                    })
                );
                assert!(matches!(
                    bindings.get(&name("A")),
                    Some(Binding::Variable(_))
                ));
            }
            _ => unreachable!(),
        }
        assert_eq!(
            globals.len(),
            original_globals_len,
            "{source_text:?} should not allocate storage"
        );
    }
}

#[test]
fn source_to_vm_e2e_reuses_global_storage_across_fresh_executions_only() {
    let (mut words, mut primitives, operators, source_words, mut bindings, mut globals, variables) =
        global_source_fixture();
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("runtime word should register");

    let (_sources, _id, first_result) = run_with_source_words_operators_and_mut_globals(
        "PUSH7\nLET A = 10",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(first_result.data_stack(), [value(7)]);
    assert_eq!(globals.view().read(variables[0]), Ok(value(10)));

    let (_sources, _id, second_result) = run_with_source_words_operators_and_mut_globals(
        "LET B = A + 1",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(second_result.outcome(), RunOutcome::Halted);
    assert_eq!(second_result.data_stack(), []);
    assert_eq!(globals.view().read(variables[0]), Ok(value(10)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(11)));
}

#[test]
fn source_to_vm_e2e_rhs_runtime_failure_maps_operator_and_preserves_target() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();
    let (sources, id) = source("LET A = 7\nLET A = 32767 + 1");

    let error = run_source(
        sources.view(),
        id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_mut_globals(globals.view_mut()),
    )
    .expect_err("checked arithmetic overflow should fail before StoreVar");

    let SourceProcessorError::Runtime(error) = error else {
        panic!("expected runtime error");
    };
    assert_eq!(
        error.source_span(),
        Ok(Some(span(sources.view(), id, 24, 25)))
    );
    assert!(matches!(
        error.vm().kind(),
        crate::vm::VmErrorKind::PrimitiveFailed {
            source: PrimitiveError::Failed,
            ..
        }
    ));
    assert_eq!(globals.view().read(variables[0]), Ok(value(7)));
}

#[test]
fn let_rhs_variable_load_mapping_uses_reference_name_span() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = register_builtin_global_variables(&mut globals, &mut bindings)
        .expect("A-Z variables should bootstrap");
    let (sources, id) = source("LET A = B + 1");

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
        Ok(&Instruction::LoadVar(variables[1]))
    );
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(sources.view(), id, 8, 9)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 3)),
        Ok(Some(span(sources.view(), id, 4, 5)))
    );
}

#[test]
fn let_rejects_target_resolution_and_syntax_errors_at_primary_span() {
    let mut source_words = SourceWordRegistry::new();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    register_builtin_global_variables(&mut globals, &mut bindings)
        .expect("A-Z variables should bootstrap");
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("runtime word should register");

    let (sources, id) = source("LET MISSING = 1");
    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("LET unresolved target should fail");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::LetTarget {
            span: span(sources.view(), id, 4, 11),
            source: ExpressionVariableErrorKind::UndefinedName,
        })
    );

    for (source_text, start, end, source_kind) in [
        (
            "LET PUSH7 = 1",
            4,
            9,
            ExpressionVariableErrorKind::TargetIsNotVariable,
        ),
        (
            "LET VAR = 1",
            4,
            7,
            ExpressionVariableErrorKind::TargetIsNotVariable,
        ),
    ] {
        let (sources, id) = source(source_text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect_err("LET non-variable target should fail");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::LetTarget {
                span: span(sources.view(), id, start, end),
                source: source_kind,
            }),
            "{source_text:?} should reject non-variable target"
        );
    }

    for (source_text, start, end, kind) in [
        ("LET", 0, 3, LetSyntaxErrorKind::Target),
        ("LET 123 = 1", 4, 7, LetSyntaxErrorKind::Target),
        ("LET A 1", 6, 7, LetSyntaxErrorKind::Equal),
        ("LET A =", 6, 7, LetSyntaxErrorKind::Rhs),
    ] {
        let (sources, id) = source(source_text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect_err("LET syntax should fail");

        assert_eq!(
            error,
            SourceProcessorError::SourceWord(SourceWordError::LetSyntax {
                span: span(sources.view(), id, start, end),
                kind,
            }),
            "{source_text:?} should report LET syntax error"
        );
    }
}

#[test]
fn failed_let_expression_does_not_compile_prior_rhs_instructions() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = register_builtin_global_variables(&mut globals, &mut bindings)
        .expect("A-Z variables should bootstrap");
    let (sources, id) = source("LET A = B + MISSING");

    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("later unresolved RHS name should fail LET");

    let SourceProcessorError::SourceWord(SourceWordError::Expression {
        source: ExpressionError::Variable(error),
    }) = error
    else {
        panic!("expected LET RHS variable resolution error");
    };
    assert_eq!(error.span(), span(sources.view(), id, 12, 19));
    assert_eq!(error.kind(), ExpressionVariableErrorKind::UndefinedName);
    assert_eq!(globals.view().read(variables[0]), Ok(value(0)));
    assert_eq!(globals.view().read(variables[1]), Ok(value(0)));
}

#[test]
fn line_number_prefixed_let_jumps_to_rhs_start_and_runs_store_var() {
    let (words, primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("built-in source words should bootstrap");
    let variables = register_builtin_global_variables(&mut globals, &mut bindings)
        .expect("A-Z variables should bootstrap");

    globals
        .view_mut()
        .write(variables[0], value(5))
        .expect("A should be writable");
    run_with_source_words_operators_and_mut_globals(
        "BIF 0, 100\nLET A = 1\n100 LET A = A + 1",
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
fn bif_condition_variable_reads_from_global_storage_at_runtime() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let variables = register_builtin_global_variables(&mut globals, &mut bindings)
        .expect("A-Z variables should bootstrap");
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    globals
        .view_mut()
        .write(variables[0], value(0))
        .expect("A should be writable");
    let (_sources, _id, zero_result) = run_with_bindings_operators_and_globals(
        "BIF A, 100\nPUSH7\n100 PUSH7",
        &bindings,
        &globals,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(zero_result.data_stack(), [value(7)]);

    globals
        .view_mut()
        .write(variables[0], value(1))
        .expect("A should be writable");
    let (_sources, _id, nonzero_result) = run_with_bindings_operators_and_globals(
        "BIF a, 100\nPUSH7\n100 PUSH7",
        &bindings,
        &globals,
        &words,
        &primitives,
        operators.lookup(),
    );
    assert_eq!(nonzero_result.data_stack(), [value(7), value(7)]);
}

#[test]
fn bif_load_var_runtime_failure_maps_to_name_span() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let mut globals = GlobalVariables::new();
    let variables = register_builtin_global_variables(&mut globals, &mut bindings)
        .expect("A-Z variables should bootstrap");
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");
    let (sources, id) = source("BIF A, 100\n100 PUSH7");

    let error = run_source(
        sources.view(),
        id,
        SourceExecutionContext::with_operators(
            &bindings,
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("LoadVar without execution globals should fail at runtime");

    let SourceProcessorError::Runtime(runtime) = error else {
        panic!("expected runtime error");
    };
    assert_eq!(
        runtime.source_span(),
        Ok(Some(span(sources.view(), id, 4, 5)))
    );
    assert_eq!(runtime.vm().address(), address(0));
    assert!(matches!(
        runtime.vm().kind(),
        crate::vm::VmErrorKind::InvalidGlobalVarId {
            source: crate::global_variable::GlobalVariableError::InvalidGlobalVarId { id }
        } if id == variables[0]
    ));
}

#[test]
fn bif_name_primary_resolution_failures_are_compile_errors_at_name_span() {
    let mut source_words = SourceWordRegistry::new();
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let data = arrays.allocate(2);
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("source words should register");
    bindings
        .insert_new(name("DATA"), Binding::Array(data))
        .expect("array should register");

    let cases = [
        (
            "BIF MISSING, 100\n100 PUSH7",
            4,
            11,
            ExpressionVariableErrorKind::UndefinedName,
        ),
        (
            "BIF PUSH7, 100\n100 PUSH7",
            4,
            9,
            ExpressionVariableErrorKind::TargetIsNotVariable,
        ),
        (
            "BIF VAR, 100\n100 PUSH7",
            4,
            7,
            ExpressionVariableErrorKind::TargetIsNotVariable,
        ),
        (
            "BIF DATA, 100\n100 PUSH7",
            4,
            8,
            ExpressionVariableErrorKind::TargetIsNotVariable,
        ),
    ];

    for (source_text, start, end, source_kind) in cases {
        let (sources, id) = source(source_text);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_source_words_and_operators(
                &bindings,
                source_words.lookup(),
                operators.lookup(),
            ),
        )
        .expect_err("non-variable expression name should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, start, end),
                kind: CompileErrorKind::ExpressionVariable {
                    source: source_kind
                },
            }),
            "{source_text:?} should reject expression name without fallback"
        );
    }
}

#[test]
fn bif_expression_name_resolution_failure_does_not_partially_commit() {
    let (_operator_words, _operator_primitives, operators) = operator_fixture();
    let mut bindings = Bindings::new();
    let mut globals = GlobalVariables::new();
    let a = globals.allocate();
    bindings
        .insert_new(name("A"), Binding::Variable(a))
        .expect("variable should register");
    let (sources, id) = source("A + MISSING");
    let mut lexer = Lexer::new(sources.view(), id).expect("lexer should build");
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token().expect("source should lex");
        if token.kind() == TokenKind::Eof {
            break;
        }
        tokens.push(token);
    }
    let mut code = SourceMappedCode::new();
    let mut builder = BlockCodeBuilder::new(&mut code);

    let error = compile_expression_tokens(
        sources.view(),
        id,
        &tokens,
        &bindings,
        operators.lookup(),
        None,
        &mut builder,
    )
    .expect_err("later unresolved name should fail the expression");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 4, 11),
            kind: CompileErrorKind::ExpressionVariable {
                source: ExpressionVariableErrorKind::UndefinedName
            },
        })
    );
    assert_eq!(code.len(), 0);
    assert_eq!(code.source_mapping().len(), 0);
}

#[test]
fn bif_expression_arithmetic_failure_maps_runtime_error_to_operator_span() {
    let (sources, id) = source("BIF 1 / 0, 100\n100 PUSH7");
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let operators = register_operator_primitives(&mut primitives, &mut words);
    let push7_id = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7_id)
        .expect("primitive should register");

    let error = run_source(
        sources.view(),
        id,
        SourceExecutionContext::with_operators(
            &bindings,
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("division by zero should fail at runtime");

    let SourceProcessorError::Runtime(error) = error else {
        panic!("expected runtime error");
    };
    assert_eq!(
        error.source_span(),
        Ok(Some(span(sources.view(), id, 6, 7)))
    );
    assert_eq!(error.vm().address(), address(2));
}

#[test]
fn malformed_bif_expression_is_span_compile_error_without_runtime_start() {
    let (sources, id, error) = compile_with_operators_error("BIF 1 +, 100\n100");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 7, 7),
            kind: CompileErrorKind::Expression {
                source: ExpressionSyntaxErrorKind::MissingOperand,
            },
        })
    );
}

#[test]
fn bif_rejects_missing_comma_target_and_trailing_tokens_as_compile_errors() {
    let cases = [
        ("BIF 0 200", 0, 3, BifSyntaxErrorKind::MissingComma),
        ("BIF 0,", 5, 6, BifSyntaxErrorKind::MissingTarget),
        (
            "BIF 0, 200 300",
            11,
            14,
            BifSyntaxErrorKind::TrailingToken {
                kind: TokenKind::IntegerLiteral,
            },
        ),
    ];

    for (source, start, end, source_kind) in cases {
        let (sources, id, error) = compile_with_operators_error(source);
        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, start, end),
                kind: CompileErrorKind::BifSyntax {
                    source: source_kind
                },
            }),
            "{source:?} should fail as malformed BIF"
        );
    }
}

#[test]
fn bif_rejects_missing_condition_as_compile_error() {
    let (sources, id, error) = compile_with_operators_error("BIF , 200");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 3),
            kind: CompileErrorKind::BifSyntax {
                source: BifSyntaxErrorKind::MissingCondition
            },
        })
    );
}

#[test]
fn undefined_bif_line_number_is_compile_error_at_target_operand() {
    let (sources, id, error) = compile_with_operators_error("BIF 0, 200");
    let SourceProcessorError::Compile(error) = error else {
        panic!("expected compile error");
    };

    assert_eq!(error.span(), span(sources.view(), id, 7, 10));
    assert_eq!(
        error.kind(),
        CompileErrorKind::LineNumber {
            source: Box::new(LineNumberError::Undefined {
                line_number: LocalLineNumber::new(200),
                span: span(sources.view(), id, 7, 10),
            })
        }
    );
}

#[test]
fn duplicate_line_number_is_compile_error_at_duplicate_span() {
    let (sources, id, error) =
        compile_with_operators_error("100 BIF 1, 200\n100 BIF 1, 200\n200 BIF 1, 200");
    let SourceProcessorError::Compile(error) = error else {
        panic!("expected compile error");
    };

    assert_eq!(error.span(), span(sources.view(), id, 15, 18));
    assert_eq!(
        error.kind(),
        CompileErrorKind::LineNumber {
            source: Box::new(LineNumberError::Duplicate {
                line_number: LocalLineNumber::new(100),
                original_span: span(sources.view(), id, 0, 3),
                duplicate_span: span(sources.view(), id, 15, 18),
            })
        }
    );
}

#[test]
fn colon_prefixed_line_number_syntax_is_not_accepted_as_local_line_number() {
    let (sources, id, error) = compile_with_operators_error("100: BIF 0, 100");

    assert_eq!(
        error,
        SourceProcessorError::Lex(LexError::InvalidCharacter {
            span: span(sources.view(), id, 3, 4),
            character: ':',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        })
    );
}

#[test]
fn run_leaves_data_stack_snapshot_in_source_order() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let push1 = primitives.register(push_1);
    let push2 = primitives.register(push_2);
    let push3 = primitives.register(push_3);
    register_primitive(&mut words, &mut bindings, name("PUSH1"), push1)
        .expect("primitive should register");
    register_primitive(&mut words, &mut bindings, name("PUSH2"), push2)
        .expect("primitive should register");
    register_primitive(&mut words, &mut bindings, name("PUSH3"), push3)
        .expect("primitive should register");
    let (sources, id) = source("PUSH1\nPUSH2\r\nPUSH3");
    let result = run_source(
        sources.view(),
        id,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("source should run");

    assert_eq!(result.outcome(), RunOutcome::Halted);
    assert_eq!(result.data_stack(), [value(1), value(2), value(3)]);
    assert_eq!(result.instruction_count(), 4);
}

#[test]
fn each_run_uses_fresh_vm_state() {
    let (mut sources, first) = source("PUSH1\nPUSH2");
    let second = sources.register("", "test.tbx");
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let push1 = primitives.register(push_1);
    let push2 = primitives.register(push_2);
    register_primitive(&mut words, &mut bindings, name("PUSH1"), push1)
        .expect("primitive should register");
    register_primitive(&mut words, &mut bindings, name("PUSH2"), push2)
        .expect("primitive should register");
    let first_result = run_source(
        sources.view(),
        first,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("first source should run");
    let second_result = run_source(
        sources.view(),
        second,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("second source should run");

    assert_eq!(first_result.data_stack(), [value(1), value(2)]);
    assert_eq!(second_result.data_stack(), []);
    assert_eq!(first_result.outcome(), RunOutcome::Halted);
    assert_eq!(second_result.outcome(), RunOutcome::Halted);
}

#[test]
fn standalone_out_of_range_integer_is_rejected_as_statement() {
    for source in ["32768", "999999999999999999999999999999"] {
        let (sources, id, error) = compile_error(source);

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 0, source.len()),
                kind: CompileErrorKind::BareExpression,
            }),
            "{source:?} should reject standalone integer statement"
        );
    }
}

#[test]
fn unsupported_name_and_minus_are_compile_errors_with_spans() {
    let (sources, id, name_error) = compile_error("RUN");
    assert_eq!(
        name_error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 3),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName
            },
        })
    );

    let (sources, id, minus_error) = compile_error("-1");
    assert_eq!(
        minus_error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 1),
            kind: CompileErrorKind::BareExpression,
        })
    );
}

#[test]
fn lexer_errors_are_not_reclassified_as_compile_errors() {
    let (sources, id, error) = compile_error("!");

    assert_eq!(
        error,
        SourceProcessorError::Lex(LexError::InvalidCharacter {
            span: span(sources.view(), id, 0, 1),
            character: '!',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        })
    );
}

#[test]
fn completed_statement_compile_error_takes_precedence_over_later_lexical_error() {
    let (sources, id, error) = compile_error("MISSING\n!");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 7),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName
            },
        })
    );
}

#[test]
fn successful_completed_statements_do_not_publish_partial_unit_before_lexical_error() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let known = primitives.register(push_7);
    register_primitive(&mut words, &mut bindings, name("KNOWN"), known)
        .expect("primitive should register");
    let (sources, id) = source("KNOWN\n!");

    let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
        .expect_err("lexical failure should prevent partial unit publication");

    assert_eq!(
        error,
        SourceProcessorError::Lex(LexError::InvalidCharacter {
            span: span(sources.view(), id, 6, 7),
            character: '!',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        })
    );
}

#[test]
fn incomplete_tail_is_not_preanalyzed_or_compiled_before_lexical_error() {
    let (sources, id, error) = compile_error("100 !");

    assert_eq!(
        error,
        SourceProcessorError::Lex(LexError::InvalidCharacter {
            span: span(sources.view(), id, 4, 5),
            character: '!',
            reason: InvalidCharacterReason::UnsupportedPunctuation,
        })
    );
}

#[test]
fn invalid_source_id_is_reported_at_source_boundary() {
    let (sources, valid) = source("RUN");
    let invalid = valid.test_next_slot();
    let words = PublishedWords::new();
    let bindings = Bindings::new();
    let primitives = PrimitiveRegistry::new();

    assert_eq!(
        compile_source(
            sources.view(),
            invalid,
            SourceCompileContext::new(&bindings)
        )
        .expect_err("invalid source should fail"),
        SourceProcessorError::Lex(LexError::Source(SourceError::InvalidSourceId {
            id: invalid
        }))
    );
    assert_eq!(
        run_source(
            sources.view(),
            invalid,
            SourceExecutionContext::new(
                &bindings,
                PublishedWordLookup::new(&words),
                primitives.lookup()
            )
        )
        .expect_err("invalid source should fail"),
        SourceProcessorError::Lex(LexError::Source(SourceError::InvalidSourceId {
            id: invalid
        }))
    );
}
use super::*;
