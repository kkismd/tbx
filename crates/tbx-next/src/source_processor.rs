use crate::binding::Bindings;
use crate::expression::{parse_expression, ExpressionError, ExpressionSyntaxErrorKind};
use crate::instruction::{
    CodeLocation, CodeSpaceLookup, CodeSpaceLookupError, Instruction, InstructionAddress,
    InstructionSequence, InstructionView,
};
use crate::lexer::{LexError, Lexer, Token, TokenKind};
use crate::operator::OperatorLookup;
use crate::primitive::PrimitiveLookup;
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::source_mapping::{
    InstructionSourceMapping, InstructionSourceMappingView, SourceMappingAppendError,
    SourceMappingLookup, SourceMappingLookupError,
};
use crate::value::Value;
use crate::vm::{ExecutionView, RunOutcome, Vm, VmError};
use crate::word_lookup::PublishedWordLookup;
use crate::word_resolution::{resolve_word_name, WordResolutionError};

#[derive(Debug)]
pub(crate) struct TemporaryExecutionUnit {
    instructions: InstructionSequence,
    mapping: InstructionSourceMapping,
    entry: CodeLocation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceCompileContext<'a> {
    bindings: &'a Bindings,
    operators: Option<OperatorLookup>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceExecutionContext<'a> {
    compile: SourceCompileContext<'a>,
    code_spaces: &'a [InstructionView<'a>],
    source_mappings: &'a [InstructionSourceMappingView<'a>],
    words: PublishedWordLookup<'a>,
    primitives: PrimitiveLookup<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRunResult {
    outcome: RunOutcome,
    data_stack: Vec<Value>,
    instruction_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeError {
    vm: VmError,
    source_span: Result<Option<SourceSpan>, SourceMappingLookupError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceProcessorError {
    Source(SourceError),
    Lex(LexError),
    Compile(CompileError),
    CodeSpaceLookup(CodeSpaceLookupError),
    SourceMappingAppend(SourceMappingAppendError),
    SourceMappingLookup(SourceMappingLookupError),
    Runtime(RuntimeError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompileError {
    span: SourceSpan,
    kind: CompileErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompileErrorKind {
    UnsupportedToken { kind: TokenKind },
    IntegerLiteralOutOfRange,
    IntegerLiteralConversion,
    WordResolution { source: WordResolutionError },
    Expression { source: ExpressionSyntaxErrorKind },
}

impl From<SourceError> for SourceProcessorError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<LexError> for SourceProcessorError {
    fn from(error: LexError) -> Self {
        Self::Lex(error)
    }
}

impl From<CompileError> for SourceProcessorError {
    fn from(error: CompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<CodeSpaceLookupError> for SourceProcessorError {
    fn from(error: CodeSpaceLookupError) -> Self {
        Self::CodeSpaceLookup(error)
    }
}

impl From<SourceMappingAppendError> for SourceProcessorError {
    fn from(error: SourceMappingAppendError) -> Self {
        Self::SourceMappingAppend(error)
    }
}

impl From<SourceMappingLookupError> for SourceProcessorError {
    fn from(error: SourceMappingLookupError) -> Self {
        Self::SourceMappingLookup(error)
    }
}

pub(crate) fn compile_source(
    view: SourceView<'_>,
    source_id: SourceId,
    context: SourceCompileContext<'_>,
) -> Result<TemporaryExecutionUnit, SourceProcessorError> {
    let mut lexer = Lexer::new(view, source_id)?;
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token()?;
        let done = token.kind() == TokenKind::Eof;
        tokens.push(token);
        if done {
            break;
        }
    }

    let mut instructions = InstructionSequence::new();
    let mut mapping = InstructionSourceMapping::new(instructions.code_space());

    if source_requires_expression(&tokens) {
        let Some(operators) = context.operators() else {
            let token = first_expression_syntax_token(&tokens)
                .expect("expression input should contain expression syntax");
            return Err(CompileError {
                span: token.span(),
                kind: CompileErrorKind::UnsupportedToken { kind: token.kind() },
            }
            .into());
        };

        let expression_tokens = tokens
            .iter()
            .copied()
            .filter(|token| token.kind() != TokenKind::LineBoundary)
            .collect::<Vec<_>>();
        parse_expression(view, &expression_tokens, operators)
            .map_err(SourceProcessorError::from_expression_error)?
            .commit_to(&mut instructions, &mut mapping)
            .map_err(SourceProcessorError::from_expression_error)?;
    } else {
        for token in &tokens {
            match token.kind() {
                TokenKind::IntegerLiteral => {
                    let value = compile_integer_literal(view, *token)?;
                    append_mapped(
                        &mut instructions,
                        &mut mapping,
                        Instruction::Push(Value::integer(value)),
                        token.span(),
                    )?;
                }
                TokenKind::Name => {
                    let id = compile_word_reference(view, *token, context)?;
                    append_mapped(
                        &mut instructions,
                        &mut mapping,
                        Instruction::Call(id),
                        token.span(),
                    )?;
                }
                TokenKind::LineBoundary => {}
                TokenKind::Eof => {}
                TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::Comma
                | TokenKind::LParen
                | TokenKind::RParen
                | TokenKind::Equal
                | TokenKind::NotEqual
                | TokenKind::Less
                | TokenKind::LessEqual
                | TokenKind::Greater
                | TokenKind::GreaterEqual => {
                    return Err(CompileError {
                        span: token.span(),
                        kind: CompileErrorKind::UnsupportedToken { kind: token.kind() },
                    }
                    .into());
                }
            }
        }
    }

    let eof = tokens
        .last()
        .copied()
        .expect("lexer should always produce an EOF token");
    append_mapped(
        &mut instructions,
        &mut mapping,
        Instruction::Halt,
        eof.span(),
    )?;

    let entry = instructions
        .view()
        .location(InstructionAddress::from_index(0));
    Ok(TemporaryExecutionUnit {
        instructions,
        mapping,
        entry,
    })
}

pub(crate) fn run_source(
    view: SourceView<'_>,
    source_id: SourceId,
    context: SourceExecutionContext<'_>,
) -> Result<SourceRunResult, SourceProcessorError> {
    let unit = compile_source(view, source_id, context.compile())?;
    run_unit(&unit, context)
}

fn run_unit(
    unit: &TemporaryExecutionUnit,
    context: SourceExecutionContext<'_>,
) -> Result<SourceRunResult, SourceProcessorError> {
    let mut code_spaces = Vec::with_capacity(context.code_spaces().len() + 1);
    code_spaces.push(unit.instructions.view());
    code_spaces.extend_from_slice(context.code_spaces());
    let execution = ExecutionView::with_code_spaces(
        CodeSpaceLookup::new(&code_spaces)?,
        context.words(),
        context.primitives(),
    );
    let mut vm = Vm::new_at_location_in(execution, unit.entry)
        .map_err(|error| map_runtime_error(error, unit, context))?;
    let outcome = vm
        .run(execution)
        .map_err(|error| map_runtime_error(error, unit, context))?;
    let data_stack = drain_data_stack(&mut vm);

    Ok(SourceRunResult {
        outcome,
        data_stack,
        instruction_count: unit.instructions.len(),
    })
}

fn map_runtime_error(
    error: VmError,
    unit: &TemporaryExecutionUnit,
    context: SourceExecutionContext<'_>,
) -> SourceProcessorError {
    let mut mapping_views = Vec::with_capacity(context.source_mappings().len() + 1);
    mapping_views.push(unit.source_mapping());
    mapping_views.extend_from_slice(context.source_mappings());
    let source_span = SourceMappingLookup::new(&mapping_views)
        .and_then(|lookup| lookup.source_span(error.location()));

    SourceProcessorError::Runtime(RuntimeError {
        vm: error,
        source_span,
    })
}

fn compile_word_reference(
    view: SourceView<'_>,
    token: Token,
    context: SourceCompileContext<'_>,
) -> Result<crate::word::WordId, SourceProcessorError> {
    let source_name = view.slice(token.span())?;
    resolve_word_name(context.bindings(), source_name)
        .map_err(|source| CompileError {
            span: token.span(),
            kind: CompileErrorKind::WordResolution { source },
        })
        .map_err(SourceProcessorError::Compile)
}

fn compile_integer_literal(
    view: SourceView<'_>,
    token: Token,
) -> Result<i16, SourceProcessorError> {
    let source = view.slice(token.span())?;
    parse_unsigned_i16(source, token.span()).map_err(SourceProcessorError::Compile)
}

fn parse_unsigned_i16(source: &str, span: SourceSpan) -> Result<i16, CompileError> {
    let mut value: i32 = 0;
    let mut saw_digit = false;

    for byte in source.bytes() {
        let Some(digit) = byte.checked_sub(b'0').filter(|digit| *digit <= 9) else {
            return Err(CompileError {
                span,
                kind: CompileErrorKind::IntegerLiteralConversion,
            });
        };

        saw_digit = true;
        value = value * 10 + i32::from(digit);
        if value > i32::from(i16::MAX) {
            return Err(CompileError {
                span,
                kind: CompileErrorKind::IntegerLiteralOutOfRange,
            });
        }
    }

    if !saw_digit {
        return Err(CompileError {
            span,
            kind: CompileErrorKind::IntegerLiteralConversion,
        });
    }

    i16::try_from(value).map_err(|_| CompileError {
        span,
        kind: CompileErrorKind::IntegerLiteralOutOfRange,
    })
}

fn source_requires_expression(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .any(|token| is_expression_syntax_token(token.kind()))
}

fn first_expression_syntax_token(tokens: &[Token]) -> Option<Token> {
    tokens
        .iter()
        .copied()
        .find(|token| is_expression_syntax_token(token.kind()))
}

const fn is_expression_syntax_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::Equal
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual
    )
}

fn append_mapped(
    instructions: &mut InstructionSequence,
    mapping: &mut InstructionSourceMapping,
    instruction: Instruction,
    span: SourceSpan,
) -> Result<InstructionAddress, SourceMappingAppendError> {
    let address = instructions.append(instruction);
    mapping.append_mapped(address, span)?;
    Ok(address)
}

fn drain_data_stack(vm: &mut Vm) -> Vec<Value> {
    let mut values = Vec::with_capacity(vm.data_stack_depth());

    while let Ok(value) = vm.pop_data() {
        values.push(value);
    }

    values.reverse();
    values
}

impl TemporaryExecutionUnit {
    pub(crate) fn entry(&self) -> InstructionAddress {
        self.entry.address()
    }

    pub(crate) fn entry_location(&self) -> CodeLocation {
        self.entry
    }

    pub(crate) fn instructions(&self) -> crate::instruction::InstructionView<'_> {
        self.instructions.view()
    }

    pub(crate) fn source_mapping(&self) -> InstructionSourceMappingView<'_> {
        self.mapping.view()
    }

    pub(crate) fn len(&self) -> usize {
        self.instructions.len()
    }

    pub(crate) fn source_span(
        &self,
        location: CodeLocation,
    ) -> Result<Option<SourceSpan>, SourceMappingLookupError> {
        self.mapping.view().source_span(location)
    }
}

impl<'a> SourceCompileContext<'a> {
    pub(crate) const fn new(bindings: &'a Bindings) -> Self {
        Self {
            bindings,
            operators: None,
        }
    }

    pub(crate) const fn with_operators(bindings: &'a Bindings, operators: OperatorLookup) -> Self {
        Self {
            bindings,
            operators: Some(operators),
        }
    }

    pub(crate) const fn bindings(self) -> &'a Bindings {
        self.bindings
    }

    pub(crate) const fn operators(self) -> Option<OperatorLookup> {
        self.operators
    }
}

impl<'a> SourceExecutionContext<'a> {
    pub(crate) const fn new(
        bindings: &'a Bindings,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            compile: SourceCompileContext::new(bindings),
            code_spaces: &[],
            source_mappings: &[],
            words,
            primitives,
        }
    }

    pub(crate) const fn with_operators(
        bindings: &'a Bindings,
        operators: OperatorLookup,
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            compile: SourceCompileContext::with_operators(bindings, operators),
            code_spaces: &[],
            source_mappings: &[],
            words,
            primitives,
        }
    }

    pub(crate) const fn with_code_spaces(
        bindings: &'a Bindings,
        code_spaces: &'a [InstructionView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            compile: SourceCompileContext::new(bindings),
            code_spaces,
            source_mappings: &[],
            words,
            primitives,
        }
    }

    pub(crate) const fn with_code_spaces_and_operators(
        bindings: &'a Bindings,
        operators: OperatorLookup,
        code_spaces: &'a [InstructionView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            compile: SourceCompileContext::with_operators(bindings, operators),
            code_spaces,
            source_mappings: &[],
            words,
            primitives,
        }
    }

    pub(crate) const fn with_code_spaces_and_mappings(
        bindings: &'a Bindings,
        code_spaces: &'a [InstructionView<'a>],
        source_mappings: &'a [InstructionSourceMappingView<'a>],
        words: PublishedWordLookup<'a>,
        primitives: PrimitiveLookup<'a>,
    ) -> Self {
        Self {
            compile: SourceCompileContext::new(bindings),
            code_spaces,
            source_mappings,
            words,
            primitives,
        }
    }

    pub(crate) const fn compile(self) -> SourceCompileContext<'a> {
        self.compile
    }

    pub(crate) const fn code_spaces(self) -> &'a [InstructionView<'a>] {
        self.code_spaces
    }

    pub(crate) const fn source_mappings(self) -> &'a [InstructionSourceMappingView<'a>] {
        self.source_mappings
    }

    pub(crate) const fn words(self) -> PublishedWordLookup<'a> {
        self.words
    }

    pub(crate) const fn primitives(self) -> PrimitiveLookup<'a> {
        self.primitives
    }
}

impl SourceProcessorError {
    fn from_expression_error(error: ExpressionError) -> Self {
        match error {
            ExpressionError::Source(error) => Self::Source(error),
            ExpressionError::Syntax(error) => Self::Compile(CompileError {
                span: error.span(),
                kind: CompileErrorKind::Expression {
                    source: error.kind(),
                },
            }),
            ExpressionError::SourceMappingAppend(error) => Self::SourceMappingAppend(error),
        }
    }
}

impl SourceRunResult {
    pub(crate) fn outcome(&self) -> RunOutcome {
        self.outcome
    }

    pub(crate) fn data_stack(&self) -> &[Value] {
        &self.data_stack
    }

    pub(crate) fn instruction_count(&self) -> usize {
        self.instruction_count
    }
}

impl RuntimeError {
    pub(crate) const fn vm(self) -> VmError {
        self.vm
    }

    pub(crate) const fn source_span(self) -> Result<Option<SourceSpan>, SourceMappingLookupError> {
        self.source_span
    }
}

impl CompileError {
    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }

    pub(crate) const fn kind(self) -> CompileErrorKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::bootstrap::register_primitive;
    use crate::lexer::InvalidCharacterReason;
    use crate::name::NormalizedName;
    use crate::operator::{register_operator_primitives, OperatorSemantic, OperatorWords};
    use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
    use crate::redefinition::redefine_word;
    use crate::source::SourceTexts;
    use crate::source_mapping::{SourceMappingLookup, SourceMappingLookupError};
    use crate::word::{CompletedWordDefinition, PrimitiveId, PublishedWords, WordId};
    use crate::word_lookup::PublishedWordLookup;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn compile(text: &str) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let bindings = Bindings::new();
        let unit = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect("source should compile");
        (sources, id, unit)
    }

    fn compile_with_bindings(
        text: &str,
        bindings: &Bindings,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let unit = compile_source(sources.view(), id, SourceCompileContext::new(bindings))
            .expect("source should compile");
        (sources, id, unit)
    }

    fn compile_expression(
        text: &str,
        operators: OperatorWords,
    ) -> (SourceTexts, SourceId, TemporaryExecutionUnit) {
        let (sources, id) = source(text);
        let bindings = Bindings::new();
        let unit = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect("expression source should compile");
        (sources, id, unit)
    }

    fn run(text: &str) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let words = PublishedWords::new();
        let bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();
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
        (sources, id, result)
    }

    fn run_expression(text: &str) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let mut words = PublishedWords::new();
        let bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let result = run_source(
            sources.view(),
            id,
            SourceExecutionContext::with_operators(
                &bindings,
                operators.lookup(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            ),
        )
        .expect("expression source should run");
        (sources, id, result)
    }

    fn compile_error(text: &str) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let bindings = Bindings::new();
        let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect_err("source should fail");
        (sources, id, error)
    }

    fn compile_expression_error(text: &str) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let mut words = PublishedWords::new();
        let bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let error = compile_source(
            sources.view(),
            id,
            SourceCompileContext::with_operators(&bindings, operators.lookup()),
        )
        .expect_err("expression source should fail");
        (sources, id, error)
    }

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    fn location(unit: &TemporaryExecutionUnit, index: usize) -> CodeLocation {
        unit.instructions().location(address(index))
    }

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn completed_primitive(slot: usize) -> CompletedWordDefinition {
        CompletedWordDefinition::primitive(PrimitiveId::from_slot(slot))
    }

    fn operator_fixture() -> (PublishedWords, PrimitiveRegistry, OperatorWords) {
        let mut words = PublishedWords::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        (words, primitives, operators)
    }

    fn completed_compiled(code: &mut InstructionSequence, value: i16) -> CompletedWordDefinition {
        let entry = code.append(Instruction::Push(Value::integer(value)));
        CompletedWordDefinition::compiled(code.view().location(entry), code.view())
            .expect("test compiled entry should be valid")
    }

    fn completed_compiled_at(
        code: &InstructionSequence,
        entry: InstructionAddress,
    ) -> CompletedWordDefinition {
        CompletedWordDefinition::compiled(code.view().location(entry), code.view())
            .expect("test compiled entry should be valid")
    }

    fn mapping_for(
        code: &InstructionSequence,
        entries: &[(InstructionAddress, Option<SourceSpan>)],
    ) -> InstructionSourceMapping {
        let mut mapping = InstructionSourceMapping::new(code.code_space());
        for (address, span) in entries {
            match span {
                Some(span) => mapping
                    .append_mapped(*address, *span)
                    .expect("mapped instruction should append"),
                None => mapping
                    .append_unmapped(*address)
                    .expect("unmapped instruction should append"),
            }
        }
        mapping
    }

    fn assert_runtime_error(
        error: SourceProcessorError,
        expected_vm_location: CodeLocation,
        expected_span: Result<Option<SourceSpan>, SourceMappingLookupError>,
    ) -> RuntimeError {
        let SourceProcessorError::Runtime(error) = error else {
            panic!("expected runtime error");
        };

        assert_eq!(error.vm().location(), expected_vm_location);
        assert_eq!(error.source_span(), expected_span);
        error
    }

    fn publish_initial(
        words: &mut PublishedWords,
        bindings: &mut Bindings,
        input: &str,
        definition: CompletedWordDefinition,
    ) -> WordId {
        let id = words.add(definition);
        bindings
            .insert_new(name(input), Binding::Word(id))
            .expect("initial test binding should register");
        id
    }

    fn push_7(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        context.push(value(7));
        Ok(())
    }

    fn add_top_two(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
        let (lhs, rhs) = context.pop2()?;
        context.push(value(lhs.as_integer() + rhs.as_integer()));
        Ok(())
    }

    fn fail_after_partial_stack_update(
        context: &mut PrimitiveContext<'_>,
    ) -> Result<(), PrimitiveError> {
        context.pop()?;
        context.push(value(99));
        Err(PrimitiveError::Failed)
    }

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
    fn integer_literals_compile_to_push_in_source_order() {
        let (sources, id, unit) = compile("0 1 42\n32767");
        let view = sources.view();

        assert_eq!(unit.entry(), address(0));
        assert_eq!(unit.len(), 5);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Push(value(0)))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Push(value(42)))
        );
        assert_eq!(
            unit.instructions().get(address(3)),
            Ok(&Instruction::Push(value(32767)))
        );
        assert_eq!(unit.instructions().get(address(4)), Ok(&Instruction::Halt));
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(view, id, 0, 1)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 1)),
            Ok(Some(span(view, id, 2, 3)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 2)),
            Ok(Some(span(view, id, 4, 6)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 3)),
            Ok(Some(span(view, id, 7, 12)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 4)),
            Ok(Some(span(view, id, 12, 12)))
        );
    }

    #[test]
    fn leading_zeroes_are_decimal_spelling_only() {
        let (_sources, _id, result) = run("000 00042 032767");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(0), value(42), value(32767)]);
    }

    #[test]
    fn expression_precedence_runs_through_source_processor() {
        let (_sources, _id, result) = run_expression("1 + 2 * 3");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(7)]);
        assert_eq!(result.instruction_count(), 6);
    }

    #[test]
    fn expression_parenthesis_unary_and_comparison_run_through_source_processor() {
        let (_sources, _id, result) = run_expression("-(1 + 2) < -2");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(1)]);
    }

    #[test]
    fn expression_operator_calls_map_to_operator_source_spans() {
        let (_words, _primitives, operators) = operator_fixture();
        let (sources, id, unit) = compile_expression("1 + 2 * 3", operators);
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
            Ok(Some(span(view, id, 6, 7)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 4)),
            Ok(Some(span(view, id, 2, 3)))
        );
    }

    #[test]
    fn expression_arithmetic_failure_maps_runtime_error_to_operator_span() {
        let (sources, id) = source("1 / 0");
        let mut words = PublishedWords::new();
        let bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);

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
            Ok(Some(span(sources.view(), id, 2, 3)))
        );
        assert_eq!(error.vm().address(), address(2));
    }

    #[test]
    fn malformed_expression_is_span_compile_error_without_runtime_start() {
        let (sources, id, error) = compile_expression_error("1 +");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 3, 3),
                kind: CompileErrorKind::Expression {
                    source: ExpressionSyntaxErrorKind::MissingOperand,
                },
            })
        );
    }

    #[test]
    fn run_leaves_data_stack_snapshot_in_source_order() {
        let (_sources, _id, result) = run("1\n2\r\n3");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(1), value(2), value(3)]);
        assert_eq!(result.instruction_count(), 4);
    }

    #[test]
    fn each_run_uses_fresh_vm_state() {
        let (mut sources, first) = source("1 2");
        let second = sources.register("");
        let words = PublishedWords::new();
        let bindings = Bindings::new();
        let primitives = PrimitiveRegistry::new();
        let context = SourceExecutionContext::new(
            &bindings,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        );

        let first_result =
            run_source(sources.view(), first, context).expect("first source should run");
        let second_result =
            run_source(sources.view(), second, context).expect("second source should run");

        assert_eq!(first_result.data_stack(), [value(1), value(2)]);
        assert_eq!(second_result.data_stack(), []);
        assert_eq!(first_result.outcome(), RunOutcome::Halted);
        assert_eq!(second_result.outcome(), RunOutcome::Halted);
    }

    #[test]
    fn integer_range_error_keeps_literal_span_and_does_not_run() {
        for source in ["32768", "999999999999999999999999999999"] {
            let (sources, id, error) = compile_error(source);

            assert_eq!(
                error,
                SourceProcessorError::Compile(CompileError {
                    span: span(sources.view(), id, 0, source.len()),
                    kind: CompileErrorKind::IntegerLiteralOutOfRange,
                }),
                "{source:?} should reject out-of-range integer"
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
                kind: CompileErrorKind::UnsupportedToken {
                    kind: TokenKind::Minus
                },
            })
        );
    }

    #[test]
    fn lexer_errors_are_not_reclassified_as_compile_errors() {
        let (sources, id, error) = compile_error("@");

        assert_eq!(
            error,
            SourceProcessorError::Lex(LexError::InvalidCharacter {
                span: span(sources.view(), id, 0, 1),
                character: '@',
                reason: InvalidCharacterReason::UnsupportedPunctuation,
            })
        );
    }

    #[test]
    fn invalid_source_id_is_reported_at_source_boundary() {
        let (sources, valid) = source("1");
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

    #[test]
    fn mapping_matches_instruction_addresses_in_order() {
        let (_sources, _id, unit) = compile("10 20");

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
        let (first_sources, first_source_id, first_unit) = compile("10");
        let (second_sources, second_source_id, second_unit) = compile("20");
        let first_span = span(first_sources.view(), first_source_id, 0, 2);
        let second_span = span(second_sources.view(), second_source_id, 0, 2);
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
        let (_sources, _source_id, unit) = compile("10");
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

        let (sources, id, unit) = compile_with_bindings("alpha 12 beta?", &bindings);
        let view = sources.view();

        assert_eq!(unit.entry(), address(0));
        assert_eq!(unit.len(), 4);
        assert_eq!(
            unit.instructions().get(address(0)),
            Ok(&Instruction::Call(first))
        );
        assert_eq!(
            unit.instructions().get(address(1)),
            Ok(&Instruction::Push(value(12)))
        );
        assert_eq!(
            unit.instructions().get(address(2)),
            Ok(&Instruction::Call(second))
        );
        assert_eq!(unit.instructions().get(address(3)), Ok(&Instruction::Halt));
        assert_eq!(
            unit.source_span(location(&unit, 0)),
            Ok(Some(span(view, id, 0, 5)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 1)),
            Ok(Some(span(view, id, 6, 8)))
        );
        assert_eq!(
            unit.source_span(location(&unit, 2)),
            Ok(Some(span(view, id, 9, 14)))
        );
    }

    #[test]
    fn case_variants_resolve_to_same_word_id_during_compile() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let id = publish_initial(&mut words, &mut bindings, "ready?", completed_primitive(2));

        let (_sources, _source_id, unit) = compile_with_bindings("ready? Ready? READY?", &bindings);

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

        let (_sources, _source_id, unit) = compile_with_bindings("prim user_word", &bindings);

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
        let (sources, source_id) = source("known missing");

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
        let primitive = primitives.register(add_top_two);
        publish_initial(
            &mut words,
            &mut bindings,
            "USER_WORD",
            completed_compiled(&mut published_code, 5),
        );
        published_code.append(Instruction::Return);
        register_primitive(&mut words, &mut bindings, name("ADD"), primitive)
            .expect("primitive should register");
        let (sources, source_id) = source("2 user_word add");
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
        let (mut sources, first) = source("1 user_word");
        let second = sources.register("user_word");
        let published_views = [published_code.view()];
        let context = SourceExecutionContext::with_code_spaces(
            &bindings,
            &published_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        );

        let first_result = run_source(sources.view(), first, context)
            .expect("first source should run with a fresh VM");
        let second_result = run_source(sources.view(), second, context)
            .expect("second source should run with a fresh VM");

        assert_eq!(first_result.data_stack(), [value(1), value(8)]);
        assert_eq!(second_result.data_stack(), [value(8)]);
        assert_eq!(words.len(), original_word_count);
        assert_eq!(published_code.len(), original_published_len);
    }

    #[test]
    fn primitive_failure_reports_call_address_through_vm_boundary() {
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let primitive = primitives.register(fail_after_partial_stack_update);
        register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
            .expect("primitive should register");
        let (sources, source_id) = source("1 fail");

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
            Ok(Some(span(sources.view(), source_id, 2, 6)))
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
        let primitive = primitives.register(fail_after_partial_stack_update);
        register_primitive(&mut words, &mut bindings, name("FAIL"), primitive)
            .expect("primitive should register");
        let (sources, source_id, unit) = compile_with_bindings("1 fail", &bindings);

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
            Ok(Some(span(sources.view(), source_id, 2, 6))),
        );
    }

    #[test]
    fn published_runtime_error_maps_to_published_source_span() {
        let mut sources = SourceTexts::new();
        let published_source = sources.register("fail");
        let temporary_source = sources.register("bad");
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
        let inner_source = sources.register("inner_fail");
        let middle_source = sources.register("middle_call");
        let outer_source = sources.register("outer_call");
        let temporary_source = sources.register("outer");
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
        let temporary_source = sources.register("bad");
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
        let context = SourceExecutionContext::with_code_spaces_and_mappings(
            &bindings,
            &published_views,
            &mapping_views,
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        );

        let end_source = sources.register("endfail");
        let out_of_range_source = sources.register("rangefail");
        let unmapped_source = sources.register("unmappedfail");

        assert_runtime_error(
            run_source(sources.view(), end_source, context).expect_err("end mapping should fail"),
            end_code.view().location(end_entry),
            Err(SourceMappingLookupError::Address {
                source: crate::instruction::InstructionAddressError::EndAddress {
                    address: end_entry,
                },
            }),
        );
        assert_runtime_error(
            run_source(sources.view(), out_of_range_source, context)
                .expect_err("out-of-range mapping should fail"),
            out_of_range_code.view().location(out_of_range_entry),
            Err(SourceMappingLookupError::Address {
                source: crate::instruction::InstructionAddressError::InvalidAddress {
                    address: out_of_range_entry,
                },
            }),
        );
        assert_runtime_error(
            run_source(sources.view(), unmapped_source, context)
                .expect_err("unmapped location should preserve VM failure"),
            unmapped_code.view().location(unmapped_entry),
            Ok(None),
        );
    }

    #[test]
    fn compile_failure_does_not_return_partial_execution_unit() {
        let (sources, id) = source("1 RUN 2");
        let bindings = Bindings::new();
        let error = compile_source(sources.view(), id, SourceCompileContext::new(&bindings))
            .expect_err("source should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 2, 5),
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
        assert_eq!(error.kind(), CompileErrorKind::IntegerLiteralOutOfRange);
    }
}
