#[test]
fn top_level_eval_leaves_constant_expression_result_on_data_stack() {
    let (words, primitives, operators, source_words, bindings, mut globals, _variables) =
        global_source_fixture();

    let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
        "EVAL 2",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.data_stack(), [value(2)]);
}

#[test]
fn top_level_eval_reuses_expression_variables_arithmetic_and_comparison() {
    let (words, primitives, operators, source_words, bindings, mut globals, variables) =
        global_source_fixture();
    globals
        .view_mut()
        .write(variables[0], value(2))
        .expect("A should be writable");

    let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
        "EVAL A + 1\nEVAL A < 3",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.data_stack(), [value(3), value(1)]);
}

#[test]
fn top_level_eval_executes_logical_expression_syntax() {
    let (words, primitives, operators, source_words, bindings, mut globals, _variables) =
        global_source_fixture();

    let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
        "EVAL 1 = 1 AND 2 = 2\nEVAL NOT 0 OR 0 AND 1",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.data_stack(), [value(1), value(1)]);
}

#[test]
fn logical_and_does_not_skip_a_failing_right_hand_side() {
    let (words, primitives, operators, source_words, bindings, mut globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("EVAL 0 AND 1 / 0");

    let error = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_mut_globals(globals.view_mut()),
    )
    .expect_err("AND must evaluate its right-hand side");

    assert!(matches!(error, SourceProcessorError::Runtime(_)));
}

#[test]
fn logical_or_does_not_skip_a_failing_right_hand_side() {
    let (words, primitives, operators, source_words, bindings, mut globals, _variables) =
        global_source_fixture();
    let (sources, source_id) = source("EVAL 1 OR 1 / 0");

    let error = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_mut_globals(globals.view_mut()),
    )
    .expect_err("OR must evaluate its right-hand side");

    assert!(matches!(error, SourceProcessorError::Runtime(_)));
}

#[test]
fn top_level_eval_result_is_available_to_following_runtime_word() {
    let (mut words, mut primitives, operators, source_words, mut bindings, mut globals, _variables) =
        global_source_fixture();
    let primitive = primitives.register(add_top_two);
    register_primitive(&mut words, &mut bindings, name("ADD"), primitive)
        .expect("ADD primitive should register");

    let (_sources, _id, result) = run_with_source_words_operators_and_mut_globals(
        "EVAL 2\nEVAL 5\nADD",
        &bindings,
        &mut globals,
        &source_words,
        &words,
        &primitives,
        operators.lookup(),
    );

    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn top_level_eval_can_call_primitive_runtime_word_inside_expression() {
    let mut session = RuntimeDefinitionSession::new();
    let add = session.register_primitive("ADD", add_top_two);
    let (sources, id) = source("EVAL ADD(2, 5)");

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("expression runtime primitive call should compile");
    let result = session
        .run_unit_with_published_code(&unit)
        .expect("expression runtime primitive call should run");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(2)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(5)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Call(add))
    );
    assert_eq!(
        unit.source_span(location(&unit, 2)),
        Ok(Some(span(sources.view(), id, 5, 8)))
    );
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn top_level_eval_rejects_array_as_runtime_word_call_target() {
    let (_words, _primitives, operators) = operator_fixture();
    let mut source_words = SourceWordRegistry::new();
    let mut arrays = crate::global_array::GlobalArrays::new();
    let mut bindings = Bindings::new();
    let data = arrays.allocate(2);
    register_builtin_source_words(&mut source_words, &mut bindings)
        .expect("source words should register");
    bindings
        .insert_new(name("DATA"), Binding::Array(data))
        .expect("array should register");
    let (sources, id) = source("EVAL DATA(1)");

    let error = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &bindings,
            source_words.lookup(),
            operators.lookup(),
        ),
    )
    .expect_err("array must not resolve as a runtime word call");

    let SourceProcessorError::SourceWord(SourceWordError::Expression {
        source: ExpressionError::Call(error),
    }) = error
    else {
        panic!("expected expression call target error");
    };
    assert_eq!(error.span(), span(sources.view(), id, 5, 9));
    assert_eq!(
        error.kind(),
        ExpressionCallErrorKind::TargetIsNotRuntimeWord
    );
}

#[test]
fn top_level_eval_can_call_compiled_runtime_word_inside_expression() {
    let mut session = RuntimeDefinitionSession::new();
    let inc_add = session.register_primitive("ADD", add_top_two);
    session.publish_def("DEF INC\nEVAL 1\nADD\nEND");
    let Some(Binding::Word(inc)) = session.bindings.get(&name("INC")).copied() else {
        panic!("INC should be published");
    };
    let (sources, id) = source("EVAL INC(6)");

    let unit = compile_source(
        sources.view(),
        id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("expression compiled runtime call should compile");
    let result = session
        .run_unit_with_published_code(&unit)
        .expect("expression compiled runtime call should run");

    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(6)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(inc))
    );
    assert_eq!(
        session.code.instruction_view().get(address(1)),
        Ok(&Instruction::Call(inc_add))
    );
    assert_eq!(result.data_stack(), [value(7)]);
}

#[test]
fn expression_runtime_call_keeps_early_bound_word_id_after_redefinition() {
    let mut session = RuntimeDefinitionSession::new();
    session.register_primitive("PUSH41", push_41);
    session.publish_def("DEF TARGET\nPUSH41\nEND");
    let Some(Binding::Word(old)) = session.bindings.get(&name("TARGET")).copied() else {
        panic!("TARGET should be published");
    };
    let (old_sources, old_id) = source("EVAL TARGET()");
    let old_unit = compile_source(
        old_sources.view(),
        old_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("old expression runtime call should compile");

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

    let (new_eval_sources, new_eval_id) = source("EVAL TARGET()");
    let new_unit = compile_source(
        new_eval_sources.view(),
        new_eval_id,
        SourceCompileContext::with_source_words_and_operators(
            &session.bindings,
            session.source_words.lookup(),
            session.operators.lookup(),
        ),
    )
    .expect("new expression runtime call should compile");
    let old_result = session
        .run_unit_with_published_code(&old_unit)
        .expect("old expression runtime call should run old body");
    let new_result = session
        .run_unit_with_published_code(&new_unit)
        .expect("new expression runtime call should run new body");

    assert_eq!(redefinition.previous(), old);
    assert_eq!(
        old_unit.instructions().get(address(0)),
        Ok(&Instruction::Call(redefinition.previous()))
    );
    assert_eq!(
        new_unit.instructions().get(address(0)),
        Ok(&Instruction::Call(redefinition.current()))
    );
    assert_eq!(old_result.data_stack(), [value(41)]);
    assert_eq!(new_result.data_stack(), [value(99)]);
}

#[test]
fn top_level_eval_reports_missing_expression_at_source_word_span() {
    let (words, primitives, operators, source_words, bindings, mut globals, _variables) =
        global_source_fixture();
    let (sources, id) = source("EVAL");

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
    .expect_err("EVAL without an expression should fail");

    assert_eq!(
        error,
        SourceProcessorError::SourceWord(SourceWordError::EvalSyntax {
            span: span(sources.view(), id, 0, 4),
            kind: EvalSyntaxErrorKind::MissingExpression,
        })
    );
}

#[test]
fn top_level_eval_preserves_existing_expression_name_diagnostic() {
    let (words, primitives, operators, source_words, bindings, mut globals, _variables) =
        global_source_fixture();
    let (sources, id) = source("EVAL MISSING");

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
    .expect_err("undefined expression name should fail");

    let SourceProcessorError::SourceWord(SourceWordError::Expression {
        source: ExpressionError::Variable(source),
    }) = error
    else {
        panic!("EVAL should preserve expression variable error");
    };
    assert_eq!(source.span(), span(sources.view(), id, 5, 12));
    assert_eq!(source.kind(), ExpressionVariableErrorKind::UndefinedName);
}

#[test]
fn source_word_case_variants_dispatch_to_same_handler() {
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
        let (sources, source_id) = source("source_marker\nSource_Marker\nSOURCE_MARKER");
        let unit = compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
        )
        .expect("source word case variants should compile");
        (sources, source_id, unit)
    };

    assert_eq!(unit.len(), 4);
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(99)))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Push(value(99)))
    );
    assert_eq!(
        unit.instructions().get(address(2)),
        Ok(&Instruction::Push(value(99)))
    );
}
use super::*;
