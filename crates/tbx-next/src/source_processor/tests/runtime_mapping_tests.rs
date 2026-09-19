use super::*;

#[test]
fn source_run_does_not_publish_temporary_code_or_reuse_vm_state() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let primitives = PrimitiveRegistry::new();
    let mut published_code = InstructionSequence::new();
    publish_initial(
        &mut words,
        &mut bindings,
        "USER_WORD",
        completed_compiled(&mut published_code, 8),
    );
    published_code.append(Instruction::Return);
    let original_word_count = words.len();
    let original_published_len = published_code.len();
    let (mut sources, first) = source("user_word\nuser_word");
    let second = sources.register("user_word", "test.tbx");
    let published_views = [published_code.view()];
    let first_result = run_source(
        sources.view(),
        first,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("first source should run with a fresh VM");
    let second_result = run_source(
        sources.view(),
        second,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect("second source should run with a fresh VM");

    assert_eq!(first_result.data_stack(), [value(8), value(8)]);
    assert_eq!(second_result.data_stack(), [value(8)]);
    assert_eq!(words.len(), original_word_count);
    assert_eq!(published_code.len(), original_published_len);
}

#[test]
fn primitive_failure_reports_call_address_through_vm_boundary() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let push7 = primitives.register(push_7);
    let primitive = primitives.register(fail_after_partial_stack_update);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
        .expect("primitive should register");
    register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
        .expect("primitive should register");
    let (sources, source_id) = source("PUSH7\nfail");

    let error = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("primitive failure should fail source run");
    let SourceProcessorError::Runtime(error) = error else {
        panic!("expected runtime error");
    };
    assert_eq!(
        error.source_span(),
        Ok(Some(span(sources.view(), source_id, 6, 10)))
    );
    let error = error.vm();

    assert_eq!(error.address(), address(1));
    match error.kind() {
        crate::vm::VmErrorKind::PrimitiveFailed {
            primitive: actual, ..
        } => assert_eq!(actual, primitive),
        other => panic!("unexpected VM error kind: {other:?}"),
    }
}

#[test]
fn temporary_runtime_error_maps_to_temporary_source_span() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let push7 = primitives.register(push_7);
    let primitive = primitives.register(fail_after_partial_stack_update);
    register_primitive(&mut words, &mut bindings, name("PUSH7"), push7)
        .expect("primitive should register");
    register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
        .expect("primitive should register");
    let (sources, source_id, unit) = compile_with_bindings("PUSH7\nfail", &bindings);

    let error = run_unit(
        &unit,
        SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("primitive failure should fail source run");

    assert_runtime_error(
        error,
        location(&unit, 1),
        Ok(Some(span(sources.view(), source_id, 6, 10))),
    );
}

#[test]
fn published_runtime_error_maps_to_published_source_span() {
    let mut sources = SourceTexts::new();
    let published_source = sources.register("fail", "test.tbx");
    let temporary_source = sources.register("bad", "test.tbx");
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(fail_after_partial_stack_update);
    let fail = publish_initial(
        &mut words,
        &mut bindings,
        "FAIL",
        CompletedWordDefinition::primitive(primitive),
    );
    let mut published_code = InstructionSequence::new();
    let published_entry = published_code.append(Instruction::Call(fail));
    let published_return = published_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "BAD",
        completed_compiled_at(&published_code, published_entry),
    );
    let published_span = span(sources.view(), published_source, 0, 4);
    let published_mapping = mapping_for(
        &published_code,
        &[
            (published_entry, Some(published_span)),
            (published_return, None),
        ],
    );
    let published_views = [published_code.view()];
    let mapping_views = [published_mapping.view()];

    let error = run_source(
        sources.view(),
        temporary_source,
        SourceExecutionContext::with_code_spaces_and_mappings(
            &bindings,
            &published_views,
            &mapping_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("published primitive failure should fail source run");

    assert_runtime_error(
        error,
        published_code.view().location(published_entry),
        Ok(Some(published_span)),
    );
}

#[test]
fn nested_published_runtime_error_uses_deepest_callee_mapping() {
    let mut sources = SourceTexts::new();
    let inner_source = sources.register("inner_fail", "test.tbx");
    let middle_source = sources.register("middle_call", "test.tbx");
    let outer_source = sources.register("outer_call", "test.tbx");
    let temporary_source = sources.register("outer", "test.tbx");
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(fail_after_partial_stack_update);
    let fail = publish_initial(
        &mut words,
        &mut bindings,
        "FAIL",
        CompletedWordDefinition::primitive(primitive),
    );

    let mut inner_code = InstructionSequence::new();
    let inner_entry = inner_code.append(Instruction::Call(fail));
    let inner_return = inner_code.append(Instruction::Return);
    let inner = publish_initial(
        &mut words,
        &mut bindings,
        "INNER",
        completed_compiled_at(&inner_code, inner_entry),
    );

    let mut middle_code = InstructionSequence::new();
    let middle_entry = middle_code.append(Instruction::Call(inner));
    let middle_return = middle_code.append(Instruction::Return);
    let middle = publish_initial(
        &mut words,
        &mut bindings,
        "MIDDLE",
        completed_compiled_at(&middle_code, middle_entry),
    );

    let mut outer_code = InstructionSequence::new();
    let outer_entry = outer_code.append(Instruction::Call(middle));
    let outer_return = outer_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "OUTER",
        completed_compiled_at(&outer_code, outer_entry),
    );

    let inner_span = span(sources.view(), inner_source, 0, 10);
    let middle_span = span(sources.view(), middle_source, 0, 11);
    let outer_span = span(sources.view(), outer_source, 0, 10);
    let inner_mapping = mapping_for(
        &inner_code,
        &[(inner_entry, Some(inner_span)), (inner_return, None)],
    );
    let middle_mapping = mapping_for(
        &middle_code,
        &[(middle_entry, Some(middle_span)), (middle_return, None)],
    );
    let outer_mapping = mapping_for(
        &outer_code,
        &[(outer_entry, Some(outer_span)), (outer_return, None)],
    );
    let published_views = [inner_code.view(), middle_code.view(), outer_code.view()];
    let mapping_views = [
        inner_mapping.view(),
        middle_mapping.view(),
        outer_mapping.view(),
    ];

    let error = run_source(
        sources.view(),
        temporary_source,
        SourceExecutionContext::with_code_spaces_and_mappings(
            &bindings,
            &published_views,
            &mapping_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("nested published primitive failure should fail source run");

    assert_eq!(inner_entry.as_index(), middle_entry.as_index());
    assert_eq!(middle_entry.as_index(), outer_entry.as_index());
    assert_runtime_error(
        error,
        inner_code.view().location(inner_entry),
        Ok(Some(inner_span)),
    );
}

#[test]
fn published_runtime_error_without_mapping_is_unknown_space() {
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(fail_after_partial_stack_update);
    let fail = publish_initial(
        &mut words,
        &mut bindings,
        "FAIL",
        CompletedWordDefinition::primitive(primitive),
    );
    let mut published_code = InstructionSequence::new();
    let published_entry = published_code.append(Instruction::Call(fail));
    published_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "BAD",
        completed_compiled_at(&published_code, published_entry),
    );
    let (sources, source_id) = source("bad");
    let published_views = [published_code.view()];

    let error = run_source(
        sources.view(),
        source_id,
        SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        ),
    )
    .expect_err("published primitive failure should fail source run");

    assert_runtime_error(
        error,
        published_code.view().location(published_entry),
        Err(SourceMappingLookupError::UnknownCodeSpace {
            code_space: published_code.code_space(),
        }),
    );
}

#[test]
fn runtime_error_mapping_distinguishes_end_out_of_range_and_unmapped() {
    let mut sources = SourceTexts::new();
    let temporary_source = sources.register("bad", "test.tbx");
    let mut words = PublishedWords::new();
    let mut bindings = Bindings::new();
    let mut primitives = PrimitiveRegistry::new();
    let primitive = primitives.register(fail_after_partial_stack_update);
    let fail = publish_initial(
        &mut words,
        &mut bindings,
        "FAIL",
        CompletedWordDefinition::primitive(primitive),
    );

    let mut end_code = InstructionSequence::new();
    let end_entry = end_code.append(Instruction::Call(fail));
    end_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "ENDFAIL",
        completed_compiled_at(&end_code, end_entry),
    );
    let end_mapping = InstructionSourceMapping::new(end_code.code_space());

    let mut out_of_range_code = InstructionSequence::new();
    let out_of_range_padding = out_of_range_code.append(Instruction::Push(value(1)));
    out_of_range_code.append(Instruction::Push(value(2)));
    let out_of_range_entry = out_of_range_code.append(Instruction::Call(fail));
    out_of_range_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "RANGEFAIL",
        completed_compiled_at(&out_of_range_code, out_of_range_entry),
    );
    let out_of_range_mapping = mapping_for(
        &out_of_range_code,
        &[(
            out_of_range_padding,
            Some(span(sources.view(), temporary_source, 0, 3)),
        )],
    );

    let mut unmapped_code = InstructionSequence::new();
    let unmapped_entry = unmapped_code.append(Instruction::Call(fail));
    unmapped_code.append(Instruction::Return);
    publish_initial(
        &mut words,
        &mut bindings,
        "UNMAPPEDFAIL",
        completed_compiled_at(&unmapped_code, unmapped_entry),
    );
    let unmapped_mapping = mapping_for(&unmapped_code, &[(unmapped_entry, None)]);

    let published_views = [
        end_code.view(),
        out_of_range_code.view(),
        unmapped_code.view(),
    ];
    let mapping_views = [
        end_mapping.view(),
        out_of_range_mapping.view(),
        unmapped_mapping.view(),
    ];
    let end_source = sources.register("endfail", "test.tbx");
    let out_of_range_source = sources.register("rangefail", "test.tbx");
    let unmapped_source = sources.register("unmappedfail", "test.tbx");

    assert_runtime_error(
        run_source(
            sources.view(),
            end_source,
            SourceExecutionContext::with_code_spaces_and_mappings(
                &bindings,
                &published_views,
                &mapping_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("end mapping should fail"),
        end_code.view().location(end_entry),
        Err(SourceMappingLookupError::Address {
            source: crate::instruction::InstructionAddressError::EndAddress { address: end_entry },
        }),
    );
    assert_runtime_error(
        run_source(
            sources.view(),
            out_of_range_source,
            SourceExecutionContext::with_code_spaces_and_mappings(
                &bindings,
                &published_views,
                &mapping_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("out-of-range mapping should fail"),
        out_of_range_code.view().location(out_of_range_entry),
        Err(SourceMappingLookupError::Address {
            source: crate::instruction::InstructionAddressError::InvalidAddress {
                address: out_of_range_entry,
            },
        }),
    );
    assert_runtime_error(
        run_source(
            sources.view(),
            unmapped_source,
            SourceExecutionContext::with_code_spaces_and_mappings(
                &bindings,
                &published_views,
                &mapping_views,
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect_err("unmapped location should preserve VM failure"),
        unmapped_code.view().location(unmapped_entry),
        Ok(None),
    );
}

#[test]
fn compile_failure_does_not_return_partial_execution_unit() {
    let (sources, id) = source("RUN");
    let bindings = Bindings::new();
    let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
        .expect_err("source should fail");

    assert_eq!(
        error,
        SourceProcessorError::Compile(CompileError {
            span: span(sources.view(), id, 0, 3),
            kind: CompileErrorKind::WordResolution {
                source: WordResolutionError::UndefinedName
            },
        })
    );
}

#[test]
fn compile_error_accessors_expose_primary_span_and_kind() {
    let (sources, id, error) = compile_error("32768");
    let SourceProcessorError::Compile(error) = error else {
        panic!("expected compile error");
    };

    assert_eq!(error.span(), span(sources.view(), id, 0, 5));
    assert_eq!(error.kind(), CompileErrorKind::BareExpression);
}
