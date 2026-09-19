use super::*;
pub(crate) fn syntax_source_word(
    context: &mut NativeSourceWordContext<'_, '_>,
) -> Result<(), SourceWordError> {
    let name_token = {
        let reader = context.statement_reader_mut();
        let name_token = reader.read_name().map_err(syntax_definition_reader_error)?;
        reader.finish().map_err(syntax_definition_reader_error)?;
        name_token
    };

    let source_name = context
        .view()
        .slice(name_token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let name = NormalizedName::new(source_name).map_err(|source| SourceWordError::SyntaxName {
        span: name_token.span(),
        source,
    })?;

    let kind = read_syntax_body_item(context, SyntaxDefinitionErrorKind::MissingKind)?;
    let SourceBlockItem::Marker(marker) = kind else {
        return Err(SourceWordError::SyntaxDefinition {
            span: syntax_item_span(kind, context.source_word_token().span()),
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let view = context.view();
    match marker.name().as_str() {
        "STATEMENT" => {
            publish_statement_syntax_definition(context, view, name, name_token.span(), marker)
        }
        "BLOCK" => publish_block_syntax_definition(context, view, name, name_token.span(), marker),
        _ => Err(SourceWordError::SyntaxDefinition {
            span: marker.span(),
            kind: SyntaxDefinitionErrorKind::UnsupportedKind,
        }),
    }
}

fn publish_statement_syntax_definition(
    context: &mut NativeSourceWordContext<'_, '_>,
    view: SourceView<'_>,
    name: NormalizedName,
    name_span: SourceSpan,
    kind_marker: SourceBlockMarker<'_>,
) -> Result<(), SourceWordError> {
    require_empty_syntax_marker_remainder(
        &kind_marker,
        SyntaxDefinitionErrorKind::UnsupportedKind,
    )?;
    let mut builder = SourceWordImplementationBuilder::new();
    loop {
        match read_syntax_body_item(context, SyntaxDefinitionErrorKind::MissingEnds)? {
            SourceBlockItem::Statement(statement) => {
                builder.push(parse_source_processing_statement(view, statement)?);
            }
            SourceBlockItem::Marker(marker) if marker.name().as_str() == "ENDS" => {
                require_empty_syntax_marker_remainder_with(&marker, |token| {
                    SyntaxDefinitionErrorKind::TrailingOperationToken { kind: token.kind() }
                })?;
                break;
            }
            SourceBlockItem::Marker(marker) => {
                return Err(SourceWordError::SyntaxDefinition {
                    span: marker.span(),
                    kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                });
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => {
                return Err(SourceWordError::SyntaxDefinition {
                    span,
                    kind: SyntaxDefinitionErrorKind::MissingEnds,
                });
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => {
                return Err(SourceWordError::DefLex { source: error });
            }
        }
    }

    let implementation = builder
        .complete()
        .map_err(|source| SourceWordError::SyntaxBuild { source })?;
    validate_statement_request_exit(implementation.instructions())?;
    context.publish_statement_source_word(name, name_span, implementation)?;
    Ok(())
}

fn publish_block_syntax_definition(
    context: &mut NativeSourceWordContext<'_, '_>,
    view: SourceView<'_>,
    name: NormalizedName,
    name_span: SourceSpan,
    kind_marker: SourceBlockMarker<'_>,
) -> Result<(), SourceWordError> {
    let exit_target = parse_block_exit_target(view, &kind_marker)?;
    let sections = read_block_syntax_sections(context, view)?;
    let artifacts = complete_block_syntax_sections(sections, kind_marker.span(), exit_target)?;
    context.publish_structured_source_word(
        name,
        name_span,
        artifacts.grammar,
        artifacts.syntax_markers,
        artifacts.implementation,
    )?;
    Ok(())
}

#[derive(Debug)]
struct BlockSyntaxArtifacts {
    grammar: StructuredGrammar,
    syntax_markers: Vec<SourceWordSyntaxMarker>,
    implementation: UserDefinedStructuredSourceWordImplementation,
}

fn parse_block_exit_target(
    view: SourceView<'_>,
    marker: &SourceBlockMarker<'_>,
) -> Result<bool, SourceWordError> {
    let Some(attribute) = marker.remaining_tokens().first().copied() else {
        return Ok(false);
    };
    if attribute.kind() != TokenKind::Name
        || !view
            .slice(attribute.span())
            .map_err(|source| SourceWordError::Source { source })?
            .eq_ignore_ascii_case("EXIT_TARGET")
    {
        return Err(SourceWordError::SyntaxDefinition {
            span: attribute.span(),
            kind: SyntaxDefinitionErrorKind::UnsupportedKind,
        });
    }
    if let Some(trailing) = marker.remaining_tokens().get(1).copied() {
        return Err(SourceWordError::SyntaxDefinition {
            span: trailing.span(),
            kind: SyntaxDefinitionErrorKind::TrailingOperationToken {
                kind: trailing.kind(),
            },
        });
    }
    Ok(true)
}

#[derive(Debug)]
struct BlockSyntaxSection {
    kind: BlockSyntaxSectionKind,
    header_span: SourceSpan,
    instructions: Vec<SourceProcessingInstruction>,
}

#[derive(Debug)]
enum BlockSyntaxSectionKind {
    Start,
    Marker {
        name: NormalizedName,
        cardinality: MarkerCardinality,
    },
    Last {
        name: NormalizedName,
    },
}

#[derive(Debug, Clone, Copy)]
struct BlockSectionLocalDefinition {
    section_index: usize,
    visible_outside_section: bool,
}

fn read_block_syntax_sections(
    context: &mut NativeSourceWordContext<'_, '_>,
    view: SourceView<'_>,
) -> Result<Vec<BlockSyntaxSection>, SourceWordError> {
    let mut sections: Vec<BlockSyntaxSection> = Vec::new();

    loop {
        match read_syntax_body_item(context, SyntaxDefinitionErrorKind::MissingEnds)? {
            SourceBlockItem::Statement(statement) => {
                let Some(section) = sections.last_mut() else {
                    return Err(SourceWordError::SyntaxDefinition {
                        span: statement.span(),
                        kind: SyntaxDefinitionErrorKind::MissingKind,
                    });
                };
                section
                    .instructions
                    .push(parse_source_processing_statement(view, statement)?);
            }
            SourceBlockItem::Marker(marker) if marker.name().as_str() == "ENDS" => {
                require_empty_syntax_marker_remainder_with(&marker, |token| {
                    SyntaxDefinitionErrorKind::TrailingOperationToken { kind: token.kind() }
                })?;
                break;
            }
            SourceBlockItem::Marker(marker) => {
                sections.push(parse_block_syntax_section_header(view, marker)?);
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => {
                return Err(SourceWordError::SyntaxDefinition {
                    span,
                    kind: SyntaxDefinitionErrorKind::MissingEnds,
                });
            }
            SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => {
                return Err(SourceWordError::DefLex { source: error });
            }
        }
    }

    Ok(sections)
}

fn parse_block_syntax_section_header(
    view: SourceView<'_>,
    marker: SourceBlockMarker<'_>,
) -> Result<BlockSyntaxSection, SourceWordError> {
    let kind = match marker.name().as_str() {
        "START" => {
            require_empty_syntax_marker_remainder(
                &marker,
                SyntaxDefinitionErrorKind::UnsupportedKind,
            )?;
            BlockSyntaxSectionKind::Start
        }
        "MARK" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::One,
        },
        "MARK_OPTIONAL" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::Optional,
        },
        "MARK_ANY" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::ZeroOrMore,
        },
        "MARK_SOME" => BlockSyntaxSectionKind::Marker {
            name: read_syntax_marker_name(view, &marker)?,
            cardinality: MarkerCardinality::OneOrMore,
        },
        "LAST" => BlockSyntaxSectionKind::Last {
            name: read_syntax_marker_name(view, &marker)?,
        },
        _ => {
            return Err(SourceWordError::SyntaxDefinition {
                span: marker.span(),
                kind: SyntaxDefinitionErrorKind::UnsupportedKind,
            });
        }
    };
    Ok(BlockSyntaxSection {
        kind,
        header_span: marker.span(),
        instructions: Vec::new(),
    })
}

fn read_syntax_marker_name(
    view: SourceView<'_>,
    marker: &SourceBlockMarker<'_>,
) -> Result<NormalizedName, SourceWordError> {
    let mut reader = SourceStatementReader::new(marker.remaining_tokens(), marker.token().span());
    let token = reader.read_name().map_err(syntax_operation_reader_error)?;
    let name = normalized_token(view, token)?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(name)
}

fn complete_block_syntax_sections(
    sections: Vec<BlockSyntaxSection>,
    fallback_span: SourceSpan,
    exit_target: bool,
) -> Result<BlockSyntaxArtifacts, SourceWordError> {
    let Some(BlockSyntaxSection {
        kind: BlockSyntaxSectionKind::Start,
        header_span,
        ..
    }) = sections.first()
    else {
        let span = sections
            .first()
            .map(|section| section.header_span)
            .unwrap_or(fallback_span);
        return Err(SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let start_header_span = *header_span;

    let control_value_ownership = validate_block_control_value_ownership(&sections)?;

    let mut validation = SourceWordImplementationBuilder::new();
    for instruction in sections
        .iter()
        .flat_map(|section| section.instructions.iter().cloned())
    {
        validation.push(instruction);
    }
    validation
        .complete()
        .map_err(|source| SourceWordError::SyntaxBuild { source })?;
    validate_block_section_local_visibility(&sections)
        .map_err(|source| SourceWordError::SyntaxBuild { source })?;

    let mut sections = sections.into_iter();
    let start = sections.next().expect("start section was validated above");
    let start = SourceWordImplementation::from_prevalidated_instructions(start.instructions);
    let mut groups = Vec::new();
    let mut syntax_markers = Vec::new();
    let mut markers = Vec::new();
    let mut terminator = None;
    let mut terminator_implementation = None;

    for section in sections {
        let section_span = section_origin_span(&section);
        match section.kind {
            BlockSyntaxSectionKind::Start => {
                return Err(SourceWordError::SyntaxDefinition {
                    span: section_span,
                    kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                });
            }
            BlockSyntaxSectionKind::Marker { name, cardinality } => {
                if terminator.is_some() {
                    return Err(SourceWordError::SyntaxDefinition {
                        span: section_span,
                        kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                    });
                }
                let marker = MarkerIdentity::new(name.clone());
                groups.push(MarkerGroup::new(marker, cardinality));
                syntax_markers.push(SourceWordSyntaxMarker::new(
                    name.clone(),
                    SourceWordSyntaxMarkerRole::BlockContinuation,
                ));
                markers.push(UserDefinedStructuredMarkerImplementation::new(
                    name,
                    SourceWordImplementation::from_prevalidated_instructions(section.instructions),
                ));
            }
            BlockSyntaxSectionKind::Last { name } => {
                if terminator.is_some() {
                    return Err(SourceWordError::SyntaxDefinition {
                        span: section_span,
                        kind: SyntaxDefinitionErrorKind::UnsupportedKind,
                    });
                }
                terminator = Some(MarkerIdentity::new(name.clone()));
                syntax_markers.push(SourceWordSyntaxMarker::new(
                    name.clone(),
                    SourceWordSyntaxMarkerRole::BlockTerminator,
                ));
                terminator_implementation =
                    Some(UserDefinedStructuredTerminatorImplementation::new(
                        name,
                        SourceWordImplementation::from_prevalidated_instructions(
                            section.instructions,
                        ),
                    ));
            }
        }
    }

    let Some(terminator_identity) = terminator else {
        return Err(SourceWordError::SyntaxDefinition {
            span: start_header_span,
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let Some(terminator_implementation) = terminator_implementation else {
        return Err(SourceWordError::SyntaxDefinition {
            span: start_header_span,
            kind: SyntaxDefinitionErrorKind::MissingKind,
        });
    };
    let grammar = StructuredGrammar::new(groups, Some(terminator_identity)).map_err(|_| {
        SourceWordError::SyntaxDefinition {
            span: terminator_implementation
                .implementation
                .instructions()
                .first()
                .map_or(start_header_span, |instruction| instruction.origin().span()),
            kind: SyntaxDefinitionErrorKind::UnsupportedKind,
        }
    })?;

    Ok(BlockSyntaxArtifacts {
        grammar,
        syntax_markers,
        implementation: UserDefinedStructuredSourceWordImplementation::new(
            start,
            markers,
            terminator_implementation,
            exit_target,
            control_value_ownership,
        ),
    })
}

fn validate_block_control_value_ownership(
    sections: &[BlockSyntaxSection],
) -> Result<usize, SourceWordError> {
    for section in sections {
        if let Some(instruction) = section.instructions.iter().find(|instruction| {
            matches!(
                instruction.operation(),
                SourceProcessingOperation::RequestExit
            )
        }) {
            return Err(SourceWordError::SyntaxDefinition {
                span: instruction.origin().span(),
                kind: SyntaxDefinitionErrorKind::RequestExitPlacement,
            });
        }
    }
    let start = &sections[0];
    let ownership = start
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation(),
                SourceProcessingOperation::EmitControlPush
            )
        })
        .count();
    if let Some(instruction) = start.instructions.iter().find(|instruction| {
        matches!(
            instruction.operation(),
            SourceProcessingOperation::EmitControlDrop
        )
    }) {
        return Err(SourceWordError::SyntaxDefinition {
            span: instruction.origin().span(),
            kind: SyntaxDefinitionErrorKind::ControlValueStartDrop,
        });
    }

    let Some(terminator_index) = sections
        .iter()
        .position(|section| matches!(section.kind, BlockSyntaxSectionKind::Last { .. }))
    else {
        return Ok(ownership);
    };
    for section in &sections[1..terminator_index] {
        if let Some(instruction) = section.instructions.iter().find(|instruction| {
            matches!(
                instruction.operation(),
                SourceProcessingOperation::EmitControlPush
                    | SourceProcessingOperation::EmitControlDrop
            )
        }) {
            return Err(SourceWordError::SyntaxDefinition {
                span: instruction.origin().span(),
                kind: SyntaxDefinitionErrorKind::ControlValueMarkerOperation,
            });
        }
    }

    let terminator = &sections[terminator_index];
    if let Some(instruction) = terminator.instructions.iter().find(|instruction| {
        matches!(
            instruction.operation(),
            SourceProcessingOperation::EmitControlPush
        )
    }) {
        return Err(SourceWordError::SyntaxDefinition {
            span: instruction.origin().span(),
            kind: SyntaxDefinitionErrorKind::ControlValueTerminatorPush,
        });
    }
    let drops = terminator
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation(),
                SourceProcessingOperation::EmitControlDrop
            )
        })
        .count();
    if drops != ownership {
        let span = terminator
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation(),
                    SourceProcessingOperation::EmitControlDrop
                )
            })
            .map_or(terminator.header_span, |instruction| {
                instruction.origin().span()
            });
        return Err(SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::ControlValueOwnershipMismatch,
        });
    }

    let mut cleanup_started = false;
    for instruction in &terminator.instructions {
        if matches!(
            instruction.operation(),
            SourceProcessingOperation::EmitControlDrop
        ) {
            cleanup_started = true;
        } else if cleanup_started {
            return Err(SourceWordError::SyntaxDefinition {
                span: instruction.origin().span(),
                kind: SyntaxDefinitionErrorKind::ControlValueCleanupOrder,
            });
        }
    }
    Ok(ownership)
}

fn validate_statement_request_exit(
    instructions: &[SourceProcessingInstruction],
) -> Result<(), SourceWordError> {
    let exits = instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation(),
                SourceProcessingOperation::RequestExit
            )
        })
        .collect::<Vec<_>>();
    let Some(exit) = exits.first() else {
        return Ok(());
    };
    if exits.len() != 1 || !std::ptr::eq(*exit, instructions.last().expect("non-empty exit list")) {
        return Err(SourceWordError::SyntaxDefinition {
            span: exit.origin().span(),
            kind: SyntaxDefinitionErrorKind::RequestExitPlacement,
        });
    }
    Ok(())
}

fn validate_block_section_local_visibility(
    sections: &[BlockSyntaxSection],
) -> Result<(), SourceWordBuildError> {
    let mut locals: HashMap<NormalizedName, BlockSectionLocalDefinition> = HashMap::new();

    for (section_index, section) in sections.iter().enumerate() {
        let visible_outside_section = block_section_locals_are_visible_outside(section);
        for instruction in &section.instructions {
            for reference in instruction.operation().consumed_local_references() {
                let Some(definition) = locals.get(reference.name()) else {
                    return Err(SourceWordBuildError::UndefinedLocal {
                        reference: reference.clone(),
                    });
                };
                if definition.section_index != section_index && !definition.visible_outside_section
                {
                    return Err(SourceWordBuildError::UndefinedLocal {
                        reference: reference.clone(),
                    });
                }
            }

            if let Some(binding) = instruction.operation().produced_binding_for_validation() {
                locals.insert(
                    binding.name().clone(),
                    BlockSectionLocalDefinition {
                        section_index,
                        visible_outside_section,
                    },
                );
            }
        }
    }

    Ok(())
}

fn block_section_locals_are_visible_outside(section: &BlockSyntaxSection) -> bool {
    match &section.kind {
        BlockSyntaxSectionKind::Start => true,
        BlockSyntaxSectionKind::Marker {
            cardinality: MarkerCardinality::One,
            ..
        } => true,
        BlockSyntaxSectionKind::Marker { .. } | BlockSyntaxSectionKind::Last { .. } => false,
    }
}

fn section_origin_span(section: &BlockSyntaxSection) -> SourceSpan {
    section
        .instructions
        .first()
        .map(|instruction| instruction.origin().span())
        .unwrap_or(section.header_span)
}

fn read_syntax_body_item<'source>(
    context: &mut NativeSourceWordContext<'source, '_>,
    eof_kind: SyntaxDefinitionErrorKind,
) -> Result<SourceBlockItem<'source>, SourceWordError> {
    let Some(reader) = context.block_reader_mut() else {
        return Err(SourceWordError::SyntaxPublicationContextUnavailable {
            span: context.source_word_token().span(),
        });
    };
    match reader.next_item()? {
        SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => {
            Err(SourceWordError::SyntaxDefinition {
                span,
                kind: eof_kind,
            })
        }
        SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => {
            Err(SourceWordError::DefLex { source: error })
        }
        item => Ok(item),
    }
}

fn require_empty_syntax_marker_remainder(
    marker: &SourceBlockMarker<'_>,
    kind: SyntaxDefinitionErrorKind,
) -> Result<(), SourceWordError> {
    require_empty_syntax_marker_remainder_with(marker, |_| kind)
}

fn require_empty_syntax_marker_remainder_with(
    marker: &SourceBlockMarker<'_>,
    error_kind: impl FnOnce(Token) -> SyntaxDefinitionErrorKind,
) -> Result<(), SourceWordError> {
    if let Some(token) = marker.remaining_tokens().first().copied() {
        return Err(SourceWordError::SyntaxDefinition {
            span: token.span(),
            kind: error_kind(token),
        });
    }
    Ok(())
}

fn syntax_item_span(item: SourceBlockItem<'_>, fallback: SourceSpan) -> SourceSpan {
    match item {
        SourceBlockItem::Statement(statement) => statement.span(),
        SourceBlockItem::Marker(marker) => marker.span(),
        SourceBlockItem::Terminal(SourceBlockTerminal::Eof { span }) => span,
        SourceBlockItem::Terminal(SourceBlockTerminal::LexError { error }) => match error {
            LexError::InvalidCharacter { span, .. } | LexError::InvalidLiteral { span, .. } => span,
            LexError::Source(_) => fallback,
        },
    }
}

fn parse_source_processing_statement(
    view: SourceView<'_>,
    statement: SourceBlockStatement<'_>,
) -> Result<SourceProcessingInstruction, SourceWordError> {
    let first = statement
        .tokens()
        .first()
        .copied()
        .ok_or(SourceWordError::SyntaxDefinition {
            span: statement.span(),
            kind: SyntaxDefinitionErrorKind::UnknownOperation,
        })?;
    let origin = SourceInstructionOrigin::new(first.span());
    let mut reader = SourceStatementReader::new(&statement.tokens()[1..], first.span());
    let operation_name = normalized_token(view, first)?;
    let operation = match operation_name.as_str() {
        "READ_NAME" => SourceProcessingOperation::ReadName {
            bind: read_as_binding(view, &mut reader)?,
        },
        "EXPECT" => SourceProcessingOperation::Expect {
            token: read_fixed_token(view, &mut reader)?,
        },
        "EXPECT_NAME" => SourceProcessingOperation::ExpectName {
            name: {
                let name = read_fixed_name(view, &mut reader)?;
                reader.finish().map_err(syntax_operation_reader_error)?;
                name
            },
        },
        "EXPECT_END" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::ExpectEnd
        }
        "READ_LINE_NUM" => SourceProcessingOperation::ReadLineNumber {
            bind: read_as_binding(view, &mut reader)?,
        },
        "READ_EXPR" => SourceProcessingOperation::ReadExpression {
            bind: read_as_binding(view, &mut reader)?,
        },
        "READ_EXPR_UNTIL" => {
            let delimiter = read_fixed_token(view, &mut reader)?;
            SourceProcessingOperation::ReadExpressionUntil {
                delimiter,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "READ_EXPR_UNTIL_NAME" => {
            let delimiter = read_fixed_name(view, &mut reader)?;
            SourceProcessingOperation::ReadExpressionUntilName {
                delimiter,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "RESOLVE_VAR" => {
            let name = read_local_reference(view, &mut reader)?;
            SourceProcessingOperation::ResolveVariable {
                name,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "RESOLVE_WORD" => {
            let name = read_local_reference(view, &mut reader)?;
            SourceProcessingOperation::ResolveWord {
                name,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "RESOLVE_WORD_LITERAL" => {
            let (name, name_span) = read_fixed_name_with_span(view, &mut reader)?;
            SourceProcessingOperation::ResolveWordLiteral {
                name,
                name_span,
                bind: read_as_binding(view, &mut reader)?,
            }
        }
        "EMIT_EXPR" => SourceProcessingOperation::EmitExpression {
            expression: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_STORE" => SourceProcessingOperation::EmitStore {
            target: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_CALL" => SourceProcessingOperation::EmitCall {
            target: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_LOAD" => SourceProcessingOperation::EmitLoad {
            target: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_INT" => SourceProcessingOperation::EmitInt {
            value: read_integer_literal(view, &mut reader)?,
        },
        "EMIT_CONTROL_PUSH" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitControlPush
        }
        "EMIT_CONTROL_COPY" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitControlCopy
        }
        "EMIT_CONTROL_DROP" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitControlDrop
        }
        "REQUEST_EXIT" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::RequestExit
        }
        "EMIT_RETURN" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitReturn
        }
        "POSITION" => SourceProcessingOperation::Position {
            bind: read_as_binding(view, &mut reader)?,
        },
        "EMIT_BRANCH" => SourceProcessingOperation::EmitBranch {
            destination: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_BRANCH_IF_FALSE" => SourceProcessingOperation::EmitBranchIfFalse {
            destination: read_only_local_reference(view, &mut reader)?,
        },
        "EMIT_BRANCH_FOLLOWING" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchFollowing
        }
        "EMIT_BRANCH_IF_FALSE_FOLLOWING" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchIfFalseFollowing
        }
        "EMIT_BRANCH_COMPLETE" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchComplete
        }
        "EMIT_BRANCH_IF_FALSE_COMPLETE" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchIfFalseComplete
        }
        "PATCH_FOLLOWING" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::PatchFollowing
        }
        "PATCH_COMPLETE" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::PatchComplete
        }
        "EMIT_BRANCH_COMPLETE_IF_FOLLOWING" => {
            reader.finish().map_err(syntax_operation_reader_error)?;
            SourceProcessingOperation::EmitBranchCompleteIfFollowing
        }
        _ => {
            return Err(SourceWordError::SyntaxDefinition {
                span: first.span(),
                kind: SyntaxDefinitionErrorKind::UnknownOperation,
            });
        }
    };
    Ok(SourceProcessingInstruction::new(operation, origin))
}

fn read_as_binding(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalBinding, SourceWordError> {
    let as_token = reader.read_name().map_err(syntax_operation_reader_error)?;
    require_name_token(view, as_token, "AS", SyntaxDefinitionErrorKind::ExpectedAs)?;
    let binding = read_local_binding(view, reader)?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(binding)
}

fn read_only_local_reference(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalReference, SourceWordError> {
    let reference = read_local_reference(view, reader)?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(reference)
}

fn read_local_binding(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalBinding, SourceWordError> {
    let token = reader.read_name().map_err(syntax_operation_reader_error)?;
    Ok(LocalBinding::new(
        normalized_token(view, token)?,
        token.span(),
    ))
}

fn read_local_reference(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<LocalReference, SourceWordError> {
    let token = reader.read_name().map_err(syntax_operation_reader_error)?;
    Ok(LocalReference::new(
        normalized_token(view, token)?,
        token.span(),
    ))
}

fn read_fixed_token(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<FixedToken, SourceWordError> {
    let token = reader
        .expect(TokenKind::FixedTokenLiteral)
        .map_err(|error| match error {
            SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
                span,
                kind: SyntaxDefinitionErrorKind::ExpectedFixedToken,
            },
            SourceStatementReaderError::Unexpected { actual, .. }
            | SourceStatementReaderError::TrailingToken { actual } => {
                SourceWordError::SyntaxDefinition {
                    span: actual.span(),
                    kind: SyntaxDefinitionErrorKind::ExpectedFixedToken,
                }
            }
        })?;
    let spelling = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    fixed_token_from_literal(spelling).ok_or(SourceWordError::SyntaxDefinition {
        span: token.span(),
        kind: SyntaxDefinitionErrorKind::ExpectedFixedToken,
    })
}

fn read_fixed_name(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<NormalizedName, SourceWordError> {
    Ok(read_fixed_name_with_span(view, reader)?.0)
}

fn read_fixed_name_with_span(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<(NormalizedName, SourceSpan), SourceWordError> {
    let token = reader.read_name().map_err(|error| match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::ExpectedName,
        },
        SourceStatementReaderError::Unexpected { actual, .. }
        | SourceStatementReaderError::TrailingToken { actual } => {
            SourceWordError::SyntaxDefinition {
                span: actual.span(),
                kind: SyntaxDefinitionErrorKind::ExpectedName,
            }
        }
    })?;
    let name = normalized_token(view, token)?;
    Ok((name, token.span()))
}

fn read_integer_literal(
    view: SourceView<'_>,
    reader: &mut SourceStatementReader<'_>,
) -> Result<i16, SourceWordError> {
    let negative = reader.peek_kind() == Some(TokenKind::Minus);
    if negative {
        reader
            .expect(TokenKind::Minus)
            .map_err(syntax_integer_reader_error)?;
    }
    let token = match reader.peek() {
        Some(token)
            if matches!(
                token.kind(),
                TokenKind::IntegerLiteral | TokenKind::HexIntegerLiteral
            ) =>
        {
            token
        }
        Some(token) => {
            return Err(SourceWordError::SyntaxDefinition {
                span: token.span(),
                kind: SyntaxDefinitionErrorKind::ExpectedIntegerLiteral,
            })
        }
        None => {
            return Err(SourceWordError::SyntaxDefinition {
                span: reader.missing_anchor,
                kind: SyntaxDefinitionErrorKind::ExpectedIntegerLiteral,
            })
        }
    };
    reader
        .expect(token.kind())
        .expect("peeked integer token must be present");
    let source = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    let digits = source.strip_prefix('$').unwrap_or(source);
    let magnitude = i32::from_str_radix(digits, if source.starts_with('$') { 16 } else { 10 })
        .map_err(|_| SourceWordError::SyntaxDefinition {
            span: token.span(),
            kind: SyntaxDefinitionErrorKind::IntegerLiteralConversion,
        })?;
    let signed = if negative { -magnitude } else { magnitude };
    let value = i16::try_from(signed).map_err(|_| SourceWordError::SyntaxDefinition {
        span: token.span(),
        kind: SyntaxDefinitionErrorKind::IntegerLiteralOutOfRange,
    })?;
    reader.finish().map_err(syntax_operation_reader_error)?;
    Ok(value)
}

fn syntax_integer_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    let span = match error {
        SourceStatementReaderError::Missing { span, .. } => span,
        SourceStatementReaderError::Unexpected { actual, .. }
        | SourceStatementReaderError::TrailingToken { actual } => actual.span(),
    };
    SourceWordError::SyntaxDefinition {
        span,
        kind: SyntaxDefinitionErrorKind::ExpectedIntegerLiteral,
    }
}

fn fixed_token_from_literal(spelling: &str) -> Option<FixedToken> {
    match spelling.strip_prefix('"')?.strip_suffix('"')? {
        "+" => Some(FixedToken::Plus),
        "-" => Some(FixedToken::Minus),
        "*" => Some(FixedToken::Star),
        "/" => Some(FixedToken::Slash),
        "%" => Some(FixedToken::Percent),
        "," => Some(FixedToken::Comma),
        "(" => Some(FixedToken::LeftParen),
        ")" => Some(FixedToken::RightParen),
        "=" => Some(FixedToken::Equal),
        "<>" => Some(FixedToken::NotEqual),
        "<" => Some(FixedToken::Less),
        "<=" => Some(FixedToken::LessEqual),
        ">" => Some(FixedToken::Greater),
        ">=" => Some(FixedToken::GreaterEqual),
        _ => None,
    }
}

fn require_name_token(
    view: SourceView<'_>,
    token: Token,
    expected: &str,
    error_kind: SyntaxDefinitionErrorKind,
) -> Result<(), SourceWordError> {
    let source_name = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    if source_name.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(SourceWordError::SyntaxDefinition {
            span: token.span(),
            kind: error_kind,
        })
    }
}

fn normalized_token(view: SourceView<'_>, token: Token) -> Result<NormalizedName, SourceWordError> {
    let source_name = view
        .slice(token.span())
        .map_err(|source| SourceWordError::Source { source })?;
    NormalizedName::new(source_name).map_err(|source| SourceWordError::SyntaxName {
        span: token.span(),
        source,
    })
}
fn syntax_definition_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::MissingName,
        },
        SourceStatementReaderError::Unexpected { actual, .. } => {
            SourceWordError::SyntaxDefinition {
                span: actual.span(),
                kind: SyntaxDefinitionErrorKind::MissingName,
            }
        }
        SourceStatementReaderError::TrailingToken { actual } => SourceWordError::SyntaxDefinition {
            span: actual.span(),
            kind: SyntaxDefinitionErrorKind::TrailingToken {
                kind: actual.kind(),
            },
        },
    }
}

fn syntax_operation_reader_error(error: SourceStatementReaderError) -> SourceWordError {
    match error {
        SourceStatementReaderError::Missing { span, .. } => SourceWordError::SyntaxDefinition {
            span,
            kind: SyntaxDefinitionErrorKind::MissingOperand,
        },
        SourceStatementReaderError::Unexpected { actual, .. } => {
            SourceWordError::SyntaxDefinition {
                span: actual.span(),
                kind: SyntaxDefinitionErrorKind::MissingOperand,
            }
        }
        SourceStatementReaderError::TrailingToken { actual } => SourceWordError::SyntaxDefinition {
            span: actual.span(),
            kind: SyntaxDefinitionErrorKind::TrailingOperationToken {
                kind: actual.kind(),
            },
        },
    }
}
