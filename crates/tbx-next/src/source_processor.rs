use crate::instruction::{Instruction, InstructionAddress, InstructionSequence};
use crate::lexer::{LexError, Lexer, Token, TokenKind};
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::value::Value;
use crate::vm::{RunOutcome, Vm, VmError};

#[derive(Debug)]
pub(crate) struct TemporaryExecutionUnit {
    instructions: InstructionSequence,
    spans: Vec<InstructionSource>,
    entry: InstructionAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InstructionSource {
    address: InstructionAddress,
    span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRunResult {
    outcome: RunOutcome,
    data_stack: Vec<Value>,
    instruction_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceProcessorError {
    Source(SourceError),
    Lex(LexError),
    Compile(CompileError),
    Vm(VmError),
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

impl From<VmError> for SourceProcessorError {
    fn from(error: VmError) -> Self {
        Self::Vm(error)
    }
}

pub(crate) fn compile_source(
    view: SourceView<'_>,
    source_id: SourceId,
) -> Result<TemporaryExecutionUnit, SourceProcessorError> {
    let mut lexer = Lexer::new(view, source_id)?;
    let mut instructions = InstructionSequence::new();
    let mut spans = Vec::new();

    loop {
        let token = lexer.next_token()?;

        match token.kind() {
            TokenKind::IntegerLiteral => {
                let value = compile_integer_literal(view, token)?;
                append_mapped(
                    &mut instructions,
                    &mut spans,
                    Instruction::Push(Value::integer(value)),
                    token.span(),
                );
            }
            TokenKind::LineBoundary => {}
            TokenKind::Eof => {
                append_mapped(
                    &mut instructions,
                    &mut spans,
                    Instruction::Halt,
                    token.span(),
                );
                break;
            }
            TokenKind::Name | TokenKind::Minus => {
                return Err(CompileError {
                    span: token.span(),
                    kind: CompileErrorKind::UnsupportedToken { kind: token.kind() },
                }
                .into());
            }
        }
    }

    let entry = InstructionAddress::from_index(0);
    Ok(TemporaryExecutionUnit {
        instructions,
        spans,
        entry,
    })
}

pub(crate) fn run_source(
    view: SourceView<'_>,
    source_id: SourceId,
) -> Result<SourceRunResult, SourceProcessorError> {
    let unit = compile_source(view, source_id)?;
    run_unit(&unit)
}

fn run_unit(unit: &TemporaryExecutionUnit) -> Result<SourceRunResult, SourceProcessorError> {
    let mut vm = Vm::new(unit.instructions.view(), unit.entry)?;
    let outcome = vm.run(unit.instructions.view())?;
    let data_stack = drain_data_stack(&mut vm);

    Ok(SourceRunResult {
        outcome,
        data_stack,
        instruction_count: unit.instructions.len(),
    })
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

fn append_mapped(
    instructions: &mut InstructionSequence,
    spans: &mut Vec<InstructionSource>,
    instruction: Instruction,
    span: SourceSpan,
) -> InstructionAddress {
    let address = instructions.append(instruction);
    spans.push(InstructionSource { address, span });
    address
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
        self.entry
    }

    pub(crate) fn instructions(&self) -> crate::instruction::InstructionView<'_> {
        self.instructions.view()
    }

    pub(crate) fn len(&self) -> usize {
        self.instructions.len()
    }

    pub(crate) fn source_span(&self, address: InstructionAddress) -> Option<SourceSpan> {
        self.spans
            .iter()
            .find(|source| source.address == address)
            .map(|source| source.span)
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
    use crate::lexer::InvalidCharacterReason;
    use crate::source::SourceTexts;

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
        let unit = compile_source(sources.view(), id).expect("source should compile");
        (sources, id, unit)
    }

    fn run(text: &str) -> (SourceTexts, SourceId, SourceRunResult) {
        let (sources, id) = source(text);
        let result = run_source(sources.view(), id).expect("source should run");
        (sources, id, result)
    }

    fn compile_error(text: &str) -> (SourceTexts, SourceId, SourceProcessorError) {
        let (sources, id) = source(text);
        let error = compile_source(sources.view(), id).expect_err("source should fail");
        (sources, id, error)
    }

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    #[test]
    fn empty_source_compiles_to_halt_only_and_runs() {
        let (sources, id, unit) = compile("");
        let view = sources.view();

        assert_eq!(unit.entry(), address(0));
        assert_eq!(unit.len(), 1);
        assert_eq!(unit.instructions().get(address(0)), Ok(&Instruction::Halt));
        assert_eq!(unit.source_span(address(0)), Some(span(view, id, 0, 0)));

        let result = run_unit(&unit).expect("halt-only unit should run");
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
                unit.source_span(address(0)),
                Some(span(sources.view(), id, eof, eof)),
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
        assert_eq!(unit.source_span(address(0)), Some(span(view, id, 0, 1)));
        assert_eq!(unit.source_span(address(1)), Some(span(view, id, 2, 3)));
        assert_eq!(unit.source_span(address(2)), Some(span(view, id, 4, 6)));
        assert_eq!(unit.source_span(address(3)), Some(span(view, id, 7, 12)));
        assert_eq!(unit.source_span(address(4)), Some(span(view, id, 12, 12)));
    }

    #[test]
    fn leading_zeroes_are_decimal_spelling_only() {
        let (_sources, _id, result) = run("000 00042 032767");

        assert_eq!(result.outcome(), RunOutcome::Halted);
        assert_eq!(result.data_stack(), [value(0), value(42), value(32767)]);
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

        let first_result = run_source(sources.view(), first).expect("first source should run");
        let second_result = run_source(sources.view(), second).expect("second source should run");

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
                kind: CompileErrorKind::UnsupportedToken {
                    kind: TokenKind::Name
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

        assert_eq!(
            compile_source(sources.view(), invalid).expect_err("invalid source should fail"),
            SourceProcessorError::Lex(LexError::Source(SourceError::InvalidSourceId {
                id: invalid
            }))
        );
        assert_eq!(
            run_source(sources.view(), invalid).expect_err("invalid source should fail"),
            SourceProcessorError::Lex(LexError::Source(SourceError::InvalidSourceId {
                id: invalid
            }))
        );
    }

    #[test]
    fn mapping_matches_instruction_addresses_in_order() {
        let (_sources, _id, unit) = compile("10 20");

        assert_eq!(
            unit.spans
                .iter()
                .map(|source| source.address)
                .collect::<Vec<_>>(),
            [address(0), address(1), address(2)]
        );
        assert_eq!(unit.len(), unit.spans.len());
        assert_eq!(
            unit.spans
                .iter()
                .enumerate()
                .map(|(index, source)| source.address.as_index() == index)
                .collect::<Vec<_>>(),
            [true, true, true]
        );
    }

    #[test]
    fn compile_failure_does_not_return_partial_execution_unit() {
        let (sources, id) = source("1 RUN 2");
        let error = compile_source(sources.view(), id).expect_err("source should fail");

        assert_eq!(
            error,
            SourceProcessorError::Compile(CompileError {
                span: span(sources.view(), id, 2, 5),
                kind: CompileErrorKind::UnsupportedToken {
                    kind: TokenKind::Name
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
