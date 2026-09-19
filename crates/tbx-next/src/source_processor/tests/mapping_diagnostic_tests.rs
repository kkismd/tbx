#[test]
fn mapping_matches_instruction_addresses_in_order() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    publish_initial(&mut words, &mut bindings, "PUSH1", completed_primitive(1));
    publish_initial(&mut words, &mut bindings, "PUSH2", completed_primitive(2));
    let (_sources, _id, unit) = compile_with_bindings("PUSH1\nPUSH2", &bindings);

    assert_eq!(
        unit.source_mapping().code_space(),
        unit.instructions().code_space()
    );
    assert_eq!(unit.source_mapping().len(), unit.len());
    assert_eq!(
        (0..unit.len())
            .map(|index| unit.source_span(location(&unit, index)).is_ok())
            .collect::<Vec<_>>(),
        [true, true, true]
    );
}

#[test]
fn temporary_mapping_location_uses_unit_code_space_identity() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    publish_initial(&mut words, &mut bindings, "PUSH1", completed_primitive(1));
    publish_initial(&mut words, &mut bindings, "PUSH2", completed_primitive(2));
    let (first_sources, first_source_id, first_unit) = compile_with_bindings("PUSH1", &bindings);
    let (second_sources, second_source_id, second_unit) = compile_with_bindings("PUSH2", &bindings);
    let first_span = span(first_sources.view(), first_source_id, 0, 5);
    let second_span = span(second_sources.view(), second_source_id, 0, 5);
    let mapping_views = [first_unit.source_mapping(), second_unit.source_mapping()];
    let lookup = SourceMappingLookup::new(&mapping_views).expect("unit mappings are distinct");

    assert_eq!(
        first_unit.source_mapping().code_space(),
        first_unit.instructions().code_space()
    );
    assert_eq!(
        first_unit
            .instructions()
            .location(address(0))
            .address()
            .as_index(),
        second_unit
            .instructions()
            .location(address(0))
            .address()
            .as_index()
    );
    assert_ne!(
        first_unit.instructions().code_space(),
        second_unit.instructions().code_space()
    );
    assert_eq!(
        lookup.source_span(first_unit.instructions().location(address(0))),
        Ok(Some(first_span))
    );
    assert_eq!(
        lookup.source_span(second_unit.instructions().location(address(0))),
        Ok(Some(second_span))
    );
}

#[test]
fn temporary_mapping_rejects_other_code_space_without_index_fallback() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    publish_initial(&mut words, &mut bindings, "PUSH1", completed_primitive(1));
    let (_sources, _source_id, unit) = compile_with_bindings("PUSH1", &bindings);
    let mut other_code = InstructionSequence::new();
    let other_address = other_code.append(Instruction::Halt);

    assert_eq!(
        unit.source_span(other_code.view().location(other_address)),
        Err(SourceMappingLookupError::Address {
            source: crate::instruction::InstructionAddressError::CodeSpaceMismatch {
                expected: unit.source_mapping().code_space(),
                actual: other_code.code_space(),
                address: other_address,
            }
        })
    );
}

#[test]
fn names_compile_to_call_in_source_order_with_source_spans() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut shared_code = InstructionSequence::new();
    let first = publish_initial(&mut words, &mut bindings, "ALPHA", completed_primitive(1));
    let second = publish_initial(
        &mut words,
        &mut bindings,
        "BETA?",
        completed_compiled(&mut shared_code, 9),
    );

    let (sources, id, unit) = compile_with_bindings("alpha\nbeta?", &bindings);
    let view = sources.view();

    assert_eq!(unit.entry(), address(0));
    assert_eq!(unit.len(), 3);
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Call(first))
    );
    assert_eq!(
        unit.instructions().get(address(1)),
        Ok(&Instruction::Call(second))
    );
    assert_eq!(unit.instructions().get(address(2)), Ok(&Instruction::Halt));
    assert_eq!(
        unit.source_span(location(&unit, 0)),
        Ok(Some(span(view, id, 0, 5)))
    );
    assert_eq!(
        unit.source_span(location(&unit, 1)),
        Ok(Some(span(view, id, 6, 11)))
    );
}

#[test]
fn statement_leading_source_word_dispatches_through_binding_resolution() {
    let mut source_words = SourceWordRegistry::new();
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut bindings = Bindings::new();
    let source_word = register_native_source_word(
        &mut source_words,
        &mut bindings,
        name("SOURCE_MARKER"),
        emit_source_word_marker,
    )
    .expect("source word should register");
    let (sources, source_id) = source("source_marker");

    let unit = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_words(&bindings, source_words.lookup()),
    )
    .expect("source word should compile");

    assert_eq!(words.len(), 0);
    assert_eq!(source_word.as_slot(), 0);
    assert_eq!(
        unit.instructions().get(address(0)),
        Ok(&Instruction::Push(value(99)))
    );
    assert_eq!(unit.instructions().get(address(1)), Ok(&Instruction::Halt));

    let result = run_unit(
        &unit,
        SourceExecutionContext::with_source_words(
            &bindings,
            source_words.lookup(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("source word unit should run");
    assert_eq!(result.data_stack(), [value(99)]);
}
use super::*;

#[test]
fn fixed_text_output_failure_preserves_source_mapping_in_user_diagnostic() {
    let (sources, source_id) = source("\"fixed\"");
    let fixed_span = span(sources.view(), source_id, 0, 7);
    let mut code = SourceMappedCode::new();
    let entry = code
        .append_mapped(Instruction::WriteFixedText(Rc::from("fixed")), fixed_span)
        .expect("fixed text instruction should be mapped");
    code.append_mapped(Instruction::Halt, fixed_span)
        .expect("halt instruction should be mapped");
    let unit = TemporaryExecutionUnit {
        entry: code.instruction_view().location(entry),
        code,
    };
    let bindings = Bindings::new();
    let words = PublishedWords::new();
    let primitives = PrimitiveRegistry::new();
    let mut output = TestOutput::new();
    output.fail_next_write(crate::runtime_output::RuntimeOutputError::Failed);

    let result = run_unit(
        &unit,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_output(&mut output),
    );
    let failure =
        crate::user_facing::UserFacingRunResult::from_source_result(sources.view(), result);
    let crate::user_facing::UserFacingRunResult::Failure(failure) = failure else {
        panic!("fixed text output should fail");
    };

    let rendered = crate::diagnostic::DiagnosticRenderer::new(sources.view())
        .render(failure.diagnostic())
        .expect("fixed text failure should render");
    let primary = rendered
        .primary()
        .expect("fixed text should retain its span");
    assert_eq!(primary.source_line(), "\"fixed\"");
    assert_eq!(primary.column_number(), 1);
    assert_eq!(primary.highlight_columns(), 7);
}
