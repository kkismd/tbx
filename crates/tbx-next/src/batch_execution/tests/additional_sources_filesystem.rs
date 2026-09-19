use super::*;

fn request_source_word(
    context: &mut crate::source_word::NativeSourceWordContext<'_, '_>,
) -> Result<(), crate::source_word::SourceWordError> {
    let specification = context.statement_reader_mut().read_name().map_err(|_| {
        crate::source_word::SourceWordError::UnsupportedSourceWord {
            span: context.source_word_token().span(),
        }
    })?;
    context.process_additional_source(specification.span())?;
    context.statement_reader_mut().finish().map_err(|_| {
        crate::source_word::SourceWordError::UnsupportedSourceWord {
            span: context.source_word_token().span(),
        }
    })
}

fn noop_source_word(
    _context: &mut crate::source_word::NativeSourceWordContext<'_, '_>,
) -> Result<(), crate::source_word::SourceWordError> {
    Ok(())
}

fn register_request_word_for_session(session: &mut SourceProcessingSession) {
    crate::bootstrap::register_native_source_word(
        &mut session.environment.source_words,
        &mut session.environment.bindings,
        name("REQUEST"),
        request_source_word,
    )
    .expect("test source word should register");
}

#[test]
fn native_source_word_requests_nested_sources_and_returns_to_each_caller_form() {
    let mut sources = SourceTexts::new();
    let source_id = sources.register("REQUEST B\nNOOP", "main.tbx");
    let mut session =
        SourceProcessingSession::new(sources, source_id).expect("processing session should build");
    crate::bootstrap::register_native_source_word(
        &mut session.environment.source_words,
        &mut session.environment.bindings,
        name("REQUEST"),
        request_source_word,
    )
    .expect("test source word should register");
    crate::bootstrap::register_native_source_word(
        &mut session.environment.source_words,
        &mut session.environment.bindings,
        name("NOOP"),
        noop_source_word,
    )
    .expect("test no-op source word should register");
    let mut writer = RecordingWriter::default();
    let mut added_b = false;
    let mut added_c = false;
    let mut order_ids = Vec::new();
    let mut hook = |sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        order_ids.push(request.span.source_id());
        match request.specification.as_ref() {
            "B" if !added_b => {
                added_b = true;
                Ok(Some(sources.register_with_acquisition(
                    "REQUEST C\nNOOP",
                    "nested-display.tbx",
                    crate::source::SourceAcquisition::FileSystem {
                        canonical_path: "/canonical/nested.tbx".into(),
                    },
                )))
            }
            "C" if !added_c => {
                added_c = true;
                Ok(Some(sources.register("NOOP", "leaf.tbx")))
            }
            specification => panic!("unexpected additional source: {specification}"),
        }
    };

    session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect("nested source should complete");
    assert_eq!(writer.text(), "");
    assert_eq!(order_ids.len(), 2);
    assert_eq!(order_ids.first(), Some(&source_id));
    assert_ne!(order_ids.get(1), Some(&source_id));
    assert_eq!(session.sources().len(), 3);
}

#[test]
fn builtin_use_source_word_requests_nested_source_with_literal_specification() {
    let mut sources = SourceTexts::new();
    let source_id = sources.register("USE \"B\"\nNOOP", "main.tbx");
    let mut session =
        SourceProcessingSession::new(sources, source_id).expect("processing session should build");
    crate::bootstrap::register_native_source_word(
        &mut session.environment.source_words,
        &mut session.environment.bindings,
        name("NOOP"),
        noop_source_word,
    )
    .expect("test no-op source word should register");
    let mut writer = RecordingWriter::default();
    let mut requested = Vec::new();
    let mut hook = |sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        requested.push((request.specification, request.span.source_id()));
        Ok(Some(sources.register("NOOP", "nested.tbx")))
    };

    session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect("USE should complete through the normal source-word path");

    assert_eq!(requested, vec![("B".into(), source_id)]);
    assert_eq!(session.sources().len(), 2);
}

#[test]
fn additional_source_capability_is_unavailable_inside_definition_body() {
    let mut sources = SourceTexts::new();
    let source_id = sources.register("DEF FOO\nREQUEST B\nEND", "main.tbx");
    let mut session =
        SourceProcessingSession::new(sources, source_id).expect("processing session should build");
    crate::bootstrap::register_native_source_word(
        &mut session.environment.source_words,
        &mut session.environment.bindings,
        name("REQUEST"),
        request_source_word,
    )
    .expect("test source word should register");
    let mut writer = RecordingWriter::default();
    let mut hook = |_sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    _request: AdditionalSourceRequest| {
        panic!("definition body must not receive additional source capability")
    };

    let error = session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect_err("definition body request should be unavailable");
    assert!(
        matches!(
            &error,
            crate::source_processor::SourceProcessorError::SourceWord(
                crate::source_word::SourceWordError::DefBodyCompile { .. }
            )
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn builtin_use_in_definition_body_cannot_request_additional_source() {
    let mut sources = SourceTexts::new();
    let source_id = sources.register("DEF FOO\nUSE \"B\"\nEND", "main.tbx");
    let mut session =
        SourceProcessingSession::new(sources, source_id).expect("processing session should build");
    let mut writer = RecordingWriter::default();
    let mut hook = |_sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    _request: AdditionalSourceRequest| {
        panic!("definition body must not receive additional source capability")
    };

    let error = session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect_err("USE in a definition body should be unavailable");
    assert!(matches!(
        error,
        crate::source_processor::SourceProcessorError::SourceWord(
            crate::source_word::SourceWordError::DefBodyCompile { .. }
        )
    ));
}

#[test]
fn filesystem_hook_resolves_nested_relative_sources_and_keeps_display_names() {
    let root = std::path::PathBuf::from(".tmp").join(format!("issue-1651-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).expect("fixture directory should be created");
    std::fs::create_dir_all(root.join("shared")).expect("fixture directory should be created");
    let main_path = root.join("main.tbx");
    let nested_path = root.join("sub/b.tbx");
    let leaf_path = root.join("shared/c.tbx");
    std::fs::write(&main_path, "REQUEST sub/b.tbx\nNOOP").expect("main fixture should be written");
    std::fs::write(&nested_path, "REQUEST ../shared/c.tbx\nNOOP")
        .expect("nested fixture should be written");
    std::fs::write(&leaf_path, "NOOP").expect("leaf fixture should be written");

    let main_canonical = std::fs::canonicalize(&main_path).expect("main path should resolve");
    let leaf_canonical = std::fs::canonicalize(&leaf_path).expect("leaf path should resolve");
    let mut sources = SourceTexts::new();
    let source_id = sources.register_with_acquisition(
        "REQUEST sub/b.tbx\nNOOP",
        "requested/main.tbx",
        crate::source::SourceAcquisition::FileSystem {
            canonical_path: main_canonical.clone(),
        },
    );
    let span = sources
        .view()
        .span(source_id, 0, 1)
        .expect("request span should be valid");
    let absolute_id = acquire_filesystem_source(
        &mut sources,
        AdditionalSourceRequest {
            specification: leaf_canonical
                .to_string_lossy()
                .into_owned()
                .into_boxed_str(),
            span,
        },
    )
    .expect("absolute source should be acquired");
    let nested_id = acquire_filesystem_source(
        &mut sources,
        AdditionalSourceRequest {
            specification: "sub/b.tbx".into(),
            span,
        },
    )
    .expect("nested source should be acquired");
    let nested_span = sources
        .view()
        .span(nested_id, 0, 1)
        .expect("nested request span should be valid");
    let leaf_id = acquire_filesystem_source(
        &mut sources,
        AdditionalSourceRequest {
            specification: "../shared/c.tbx".into(),
            span: nested_span,
        },
    )
    .expect("leaf source should be acquired");

    let view = sources.view();
    assert_eq!(
        view.display_name(absolute_id),
        Ok(leaf_canonical.to_string_lossy().as_ref())
    );
    assert_eq!(view.source(absolute_id), Ok("NOOP"));
    assert_eq!(
        view.acquisition(absolute_id),
        Ok(&crate::source::SourceAcquisition::FileSystem {
            canonical_path: leaf_canonical.clone(),
        })
    );
    assert_eq!(view.display_name(nested_id), Ok("sub/b.tbx"));
    assert_eq!(view.display_name(leaf_id), Ok("../shared/c.tbx"));
    assert_eq!(
        view.acquisition(nested_id),
        Ok(&crate::source::SourceAcquisition::FileSystem {
            canonical_path: std::fs::canonicalize(&nested_path)
                .expect("nested path should resolve"),
        })
    );
    assert_eq!(
        view.acquisition(leaf_id),
        Ok(&crate::source::SourceAcquisition::FileSystem {
            canonical_path: leaf_canonical.clone(),
        })
    );
    assert_eq!(view.source(leaf_id), Ok("NOOP"));
    std::fs::remove_dir_all(root).expect("fixture directory should be removed");
}

#[test]
fn filesystem_hook_rejects_relative_request_from_non_file_source() {
    let mut sources = SourceTexts::new();
    let source_id = sources.register("REQUEST child.tbx", "<stdin>");
    let span = sources
        .view()
        .span(source_id, 0, "REQUEST child.tbx".len())
        .expect("request span should be valid");
    let error = acquire_filesystem_source(
        &mut sources,
        AdditionalSourceRequest {
            specification: "child.tbx".into(),
            span,
        },
    )
    .expect_err("stdin relative request must fail");
    assert!(matches!(
        error,
        SourceProcessorError::AdditionalSourceAcquisition {
            span: actual,
            specification,
            kind: AdditionalSourceAcquisitionError::RelativePathRequiresFileSource,
        } if actual == span && specification.as_ref() == "child.tbx"
    ));
}

#[test]
fn filesystem_acquisition_skips_completed_identity_and_rejects_processing_identity() {
    let root =
        std::path::PathBuf::from(".tmp").join(format!("issue-1658-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("fixture directory should be created");
    let path = root.join("child.tbx");
    std::fs::write(&path, "NOOP").expect("fixture source should be written");
    let canonical_path = std::fs::canonicalize(&path).expect("fixture path should resolve");

    let mut sources = SourceTexts::new();
    let parent_id = sources.register("REQUEST child.tbx", "main.tbx");
    let span = sources
        .view()
        .span(parent_id, 0, 1)
        .expect("request span should be valid");
    let request = |specification: &str| AdditionalSourceRequest {
        specification: specification.into(),
        span,
    };

    let mut states = SourceAcquisitionStates::default();
    states
        .begin(&canonical_path)
        .expect("identity should begin");
    let cycle = acquire_filesystem_source_with_states(
        &mut sources,
        &mut states,
        request(canonical_path.to_string_lossy().as_ref()),
    )
    .expect_err("processing identity should be a cycle");
    assert!(matches!(
        cycle,
        SourceProcessorError::AdditionalSourceAcquisition {
            span: actual,
            specification,
            kind: AdditionalSourceAcquisitionError::Cycle,
        } if actual == span && specification.as_ref() == canonical_path.to_string_lossy().as_ref()
    ));
    assert_eq!(sources.len(), 1, "cycle must not register a source");

    states.complete(&SourceAcquisition::FileSystem {
        canonical_path: canonical_path.clone(),
    });
    let completed = acquire_filesystem_source_with_states(
        &mut sources,
        &mut states,
        request(canonical_path.to_string_lossy().as_ref()),
    )
    .expect("completed identity should be a no-op");
    assert_eq!(completed, None);
    assert_eq!(
        sources.len(),
        1,
        "completed identity must not register a source"
    );
    std::fs::remove_dir_all(root).expect("fixture directory should be removed");
}

#[test]
fn top_level_filesystem_source_is_completed_only_after_its_frame_finishes() {
    let canonical_path = std::path::PathBuf::from(".tmp")
        .join(format!("issue-1658-top-level-{}.tbx", std::process::id()));
    let mut sources = SourceTexts::new();
    let source_id = sources.register_with_acquisition(
        "",
        "main.tbx",
        SourceAcquisition::FileSystem {
            canonical_path: canonical_path.clone(),
        },
    );
    let mut session =
        SourceProcessingSession::new(sources, source_id).expect("processing session should build");
    assert_eq!(
        session.acquisition_states.states.get(&canonical_path),
        Some(&SourceIdentityState::Processing)
    );
    let mut writer = RecordingWriter::default();
    let mut hook = |_sources: &mut SourceTexts,
                    _states: &mut SourceAcquisitionStates,
                    _request: AdditionalSourceRequest| {
        unreachable!("source has no additional request")
    };
    session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect("source should complete");
    assert_eq!(
        session.acquisition_states.states.get(&canonical_path),
        Some(&SourceIdentityState::Completed)
    );
}

#[test]
fn filesystem_session_detects_indirect_cycle_before_registering_cycle_source() {
    let root =
        std::path::PathBuf::from(".tmp").join(format!("issue-1658-cycle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("fixture directory should be created");
    let a_path = root.join("A");
    let b_path = root.join("B");
    std::fs::write(&a_path, "REQUEST B").expect("A fixture should be written");
    std::fs::write(&b_path, "REQUEST A").expect("B fixture should be written");
    let a_canonical = std::fs::canonicalize(&a_path).expect("A path should resolve");

    let mut sources = SourceTexts::new();
    let a_id = sources.register_with_acquisition(
        "REQUEST B",
        "A",
        SourceAcquisition::FileSystem {
            canonical_path: a_canonical.clone(),
        },
    );
    let mut session =
        SourceProcessingSession::new(sources, a_id).expect("processing session should build");
    register_request_word_for_session(&mut session);
    let mut writer = RecordingWriter::default();
    let mut hook = |sources: &mut SourceTexts,
                    states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        acquire_filesystem_source_with_states(sources, states, request)
    };

    let error = session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect_err("A -> B -> A should be a cycle");
    assert!(matches!(
        error,
        SourceProcessorError::AdditionalSourceAcquisition {
            specification,
            kind: AdditionalSourceAcquisitionError::Cycle,
            ..
        } if specification.as_ref() == "A"
    ));
    assert_eq!(session.sources().len(), 2, "cycle source must not register");
    assert_eq!(
        session.acquisition_states.states.get(&a_canonical),
        Some(&SourceIdentityState::Processing)
    );
    std::fs::remove_dir_all(root).expect("fixture directory should be removed");
}

#[test]
#[cfg(unix)]
fn filesystem_session_does_not_reprocess_completed_canonical_alias() {
    let root =
        std::path::PathBuf::from(".tmp").join(format!("issue-1658-alias-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("fixture directory should be created");
    let a_path = root.join("A");
    let b_path = root.join("B");
    let alias_path = root.join("C");
    std::fs::write(&a_path, "REQUEST B\nREQUEST C\nFOO").expect("A fixture should be written");
    std::fs::write(&b_path, "DEF FOO\nEND").expect("B fixture should be written");
    std::os::unix::fs::symlink("B", &alias_path).expect("alias should be created");
    let a_canonical = std::fs::canonicalize(&a_path).expect("A path should resolve");

    let mut sources = SourceTexts::new();
    let a_id = sources.register_with_acquisition(
        "REQUEST B\nREQUEST C\nFOO",
        "A",
        SourceAcquisition::FileSystem {
            canonical_path: a_canonical,
        },
    );
    let mut session =
        SourceProcessingSession::new(sources, a_id).expect("processing session should build");
    register_request_word_for_session(&mut session);
    let mut writer = RecordingWriter::default();
    let mut hook = |sources: &mut SourceTexts,
                    states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        acquire_filesystem_source_with_states(sources, states, request)
    };

    session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect("completed canonical alias should be a no-op");
    assert_eq!(session.sources().len(), 2, "alias must not register twice");
    std::fs::remove_dir_all(root).expect("fixture directory should be removed");
}

#[test]
fn filesystem_session_leaves_failed_source_uncompleted_and_cannot_resume() {
    let root =
        std::path::PathBuf::from(".tmp").join(format!("issue-1658-failure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("fixture directory should be created");
    let a_path = root.join("A");
    let b_path = root.join("B");
    std::fs::write(&a_path, "REQUEST B").expect("A fixture should be written");
    std::fs::write(&b_path, "UNKNOWN").expect("B fixture should be written");
    let a_canonical = std::fs::canonicalize(&a_path).expect("A path should resolve");
    let b_canonical = std::fs::canonicalize(&b_path).expect("B path should resolve");

    let mut sources = SourceTexts::new();
    let a_id = sources.register_with_acquisition(
        "REQUEST B",
        "A",
        SourceAcquisition::FileSystem {
            canonical_path: a_canonical,
        },
    );
    let mut session =
        SourceProcessingSession::new(sources, a_id).expect("processing session should build");
    register_request_word_for_session(&mut session);
    let mut writer = RecordingWriter::default();
    let mut hook = |sources: &mut SourceTexts,
                    states: &mut SourceAcquisitionStates,
                    request: AdditionalSourceRequest| {
        acquire_filesystem_source_with_states(sources, states, request)
    };

    session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect_err("B compilation should fail");
    assert_eq!(
        session.acquisition_states.states.get(&b_canonical),
        Some(&SourceIdentityState::Processing)
    );
    let resume_error = session
        .run_with_hook(&mut writer, &mut hook, None)
        .expect_err("failed session must not be resumed");
    assert_eq!(resume_error, SourceProcessorError::ProcessingSessionFailed);
    std::fs::remove_dir_all(root).expect("fixture directory should be removed");
}
