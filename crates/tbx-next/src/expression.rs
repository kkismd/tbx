use crate::global_variable::GlobalVarId;
use crate::instruction::{Instruction, InstructionSequence};
use crate::lexer::{Token, TokenKind};
use crate::operator::{OperatorLookup, OperatorSemantic};
use crate::source::{SourceError, SourceSpan, SourceView};
use crate::source_mapping::{InstructionSourceMapping, SourceMappingAppendError};
use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpressionStaging {
    entries: Vec<StagedInstruction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StagedInstruction {
    instruction: Instruction,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpressionError {
    Source(SourceError),
    Syntax(ExpressionSyntaxError),
    Variable(ExpressionVariableError),
    SourceMappingAppend(SourceMappingAppendError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpressionSyntaxError {
    span: SourceSpan,
    kind: ExpressionSyntaxErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpressionSyntaxErrorKind {
    UnexpectedToken { kind: TokenKind },
    MissingOperand,
    UnmatchedParenthesis,
    IntegerLiteralOutOfRange,
    IntegerLiteralConversion,
    ComparisonChain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpressionVariableError {
    span: SourceSpan,
    kind: ExpressionVariableErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpressionVariableErrorKind {
    InvalidName,
    UndefinedName,
    TargetIsNotVariable,
}

pub(crate) trait ExpressionVariableResolver {
    fn resolve_variable(
        &self,
        source_name: &str,
    ) -> Result<GlobalVarId, ExpressionVariableErrorKind>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedExpression {
    contains_comparison: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BinaryOperator {
    semantic: OperatorSemantic,
    precedence: u8,
    comparison: bool,
}

#[derive(Clone, Copy)]
struct ExpressionParser<'a, 'r> {
    view: SourceView<'a>,
    tokens: &'a [Token],
    operators: OperatorLookup,
    variables: &'r dyn ExpressionVariableResolver,
    position: usize,
}

const PRECEDENCE_COMPARISON: u8 = 1;
const PRECEDENCE_ADDITIVE: u8 = 2;
const PRECEDENCE_MULTIPLICATIVE: u8 = 3;
const PRECEDENCE_PREFIX: u8 = 4;

pub(crate) fn parse_expression(
    view: SourceView<'_>,
    tokens: &[Token],
    operators: OperatorLookup,
    variables: &dyn ExpressionVariableResolver,
) -> Result<ExpressionStaging, ExpressionError> {
    let mut parser = ExpressionParser::new(view, tokens, operators, variables);
    parser.parse_complete()
}

impl<F> ExpressionVariableResolver for F
where
    F: Fn(&str) -> Result<GlobalVarId, ExpressionVariableErrorKind>,
{
    fn resolve_variable(
        &self,
        source_name: &str,
    ) -> Result<GlobalVarId, ExpressionVariableErrorKind> {
        self(source_name)
    }
}

impl ExpressionStaging {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(&mut self, instruction: Instruction, span: SourceSpan) {
        self.entries.push(StagedInstruction { instruction, span });
    }

    pub(crate) fn commit_to(
        &self,
        instructions: &mut InstructionSequence,
        mapping: &mut InstructionSourceMapping,
    ) -> Result<(), ExpressionError> {
        for entry in &self.entries {
            let address = instructions.append(entry.instruction);
            mapping
                .append_mapped(address, entry.span)
                .map_err(ExpressionError::SourceMappingAppend)?;
        }

        Ok(())
    }

    pub(crate) fn entries(&self) -> &[StagedInstruction] {
        &self.entries
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

impl StagedInstruction {
    pub(crate) const fn instruction(self) -> Instruction {
        self.instruction
    }

    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }
}

impl ExpressionSyntaxError {
    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }

    pub(crate) const fn kind(self) -> ExpressionSyntaxErrorKind {
        self.kind
    }
}

impl ExpressionVariableError {
    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }

    pub(crate) const fn kind(self) -> ExpressionVariableErrorKind {
        self.kind
    }
}

impl From<SourceError> for ExpressionError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl<'a, 'r> ExpressionParser<'a, 'r> {
    fn new(
        view: SourceView<'a>,
        tokens: &'a [Token],
        operators: OperatorLookup,
        variables: &'r dyn ExpressionVariableResolver,
    ) -> Self {
        Self {
            view,
            tokens,
            operators,
            variables,
            position: 0,
        }
    }

    fn parse_complete(&mut self) -> Result<ExpressionStaging, ExpressionError> {
        let mut staging = ExpressionStaging::new();
        self.parse_infix_expression(&mut staging, PRECEDENCE_COMPARISON)?;

        match self.peek() {
            Some(token) if is_expression_terminator(token.kind()) => Ok(staging),
            Some(token) => Err(self.syntax(token, unexpected_token(token))),
            None => Ok(staging),
        }
    }

    fn parse_infix_expression(
        &mut self,
        staging: &mut ExpressionStaging,
        min_precedence: u8,
    ) -> Result<ParsedExpression, ExpressionError> {
        let mut parsed = self.parse_prefix(staging)?;

        while let Some(token) = self.peek() {
            let Some(operator) = binary_operator(token.kind()) else {
                break;
            };
            if operator.precedence < min_precedence {
                break;
            }
            if operator.comparison && parsed.contains_comparison {
                return Err(self.syntax(token, ExpressionSyntaxErrorKind::ComparisonChain));
            }

            let operator_token = self.advance();
            let rhs = self.parse_infix_expression(staging, operator.precedence + 1)?;
            parsed.contains_comparison |= operator.comparison || rhs.contains_comparison;
            staging.push(
                Instruction::Call(self.operators.resolve(operator.semantic)),
                operator_token.span(),
            );
        }

        Ok(parsed)
    }

    fn parse_prefix(
        &mut self,
        staging: &mut ExpressionStaging,
    ) -> Result<ParsedExpression, ExpressionError> {
        let token = self.peek().ok_or_else(|| self.missing_at_last_token())?;

        if token.kind() == TokenKind::Minus {
            let minus = self.advance();
            if self.try_compile_min_integer_literal(staging)? {
                return Ok(ParsedExpression {
                    contains_comparison: false,
                });
            }

            let parsed = self.parse_infix_expression(staging, PRECEDENCE_PREFIX)?;
            staging.push(
                Instruction::Call(self.operators.resolve(OperatorSemantic::Negate)),
                minus.span(),
            );
            return Ok(parsed);
        }

        self.parse_postfix(staging)
    }

    fn parse_postfix(
        &mut self,
        staging: &mut ExpressionStaging,
    ) -> Result<ParsedExpression, ExpressionError> {
        let parsed = self.parse_primary(staging)?;

        // Future call syntax belongs here, after a name/callable primary and
        // before infix binding. Grouping `(` is handled only by `parse_primary`.
        Ok(parsed)
    }

    fn parse_primary(
        &mut self,
        staging: &mut ExpressionStaging,
    ) -> Result<ParsedExpression, ExpressionError> {
        let token = self.peek().ok_or_else(|| self.missing_at_last_token())?;

        match token.kind() {
            TokenKind::IntegerLiteral => {
                let token = self.advance();
                let value = self.parse_unsigned_i16(token)?;
                staging.push(Instruction::Push(Value::integer(value)), token.span());
                Ok(ParsedExpression {
                    contains_comparison: false,
                })
            }
            TokenKind::Name => {
                let token = self.advance();
                let source_name = self.view.slice(token.span())?;
                let id = self
                    .variables
                    .resolve_variable(source_name)
                    .map_err(|kind| {
                        ExpressionError::Variable(ExpressionVariableError {
                            span: token.span(),
                            kind,
                        })
                    })?;
                staging.push(Instruction::LoadVar(id), token.span());
                Ok(ParsedExpression {
                    contains_comparison: false,
                })
            }
            TokenKind::LParen => {
                let lparen = self.advance();
                let parsed = self.parse_infix_expression(staging, PRECEDENCE_COMPARISON)?;
                match self.peek() {
                    Some(token) if token.kind() == TokenKind::RParen => {
                        self.advance();
                        Ok(parsed)
                    }
                    Some(token) if is_expression_terminator(token.kind()) => {
                        Err(self.syntax(lparen, ExpressionSyntaxErrorKind::UnmatchedParenthesis))
                    }
                    Some(token) => Err(self.syntax(token, unexpected_token(token))),
                    None => {
                        Err(self.syntax(lparen, ExpressionSyntaxErrorKind::UnmatchedParenthesis))
                    }
                }
            }
            TokenKind::Eof | TokenKind::LineBoundary | TokenKind::RParen => {
                Err(self.syntax(token, ExpressionSyntaxErrorKind::MissingOperand))
            }
            _ => Err(self.syntax(token, unexpected_token(token))),
        }
    }

    fn try_compile_min_integer_literal(
        &mut self,
        staging: &mut ExpressionStaging,
    ) -> Result<bool, ExpressionError> {
        let Some(token) = self.peek() else {
            return Ok(false);
        };
        if token.kind() != TokenKind::IntegerLiteral {
            return Ok(false);
        }

        let source = self.view.slice(token.span())?;
        if parse_unsigned_i32(source, token.span())? != 32768 {
            return Ok(false);
        }

        let token = self.advance();
        staging.push(Instruction::Push(Value::integer(i16::MIN)), token.span());
        Ok(true)
    }

    fn parse_unsigned_i16(&self, token: Token) -> Result<i16, ExpressionError> {
        let source = self.view.slice(token.span())?;
        let value = parse_unsigned_i32(source, token.span())?;
        i16::try_from(value)
            .map_err(|_| self.syntax(token, ExpressionSyntaxErrorKind::IntegerLiteralOutOfRange))
    }

    fn peek(self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn advance(&mut self) -> Token {
        let token = self
            .peek()
            .expect("parser should advance only after peeking a token");
        self.position += 1;
        token
    }

    fn missing_at_last_token(&self) -> ExpressionError {
        match self.tokens.last().copied() {
            Some(token) => self.syntax(token, ExpressionSyntaxErrorKind::MissingOperand),
            None => panic!("expression parser requires at least one EOF token"),
        }
    }

    fn syntax(&self, token: Token, kind: ExpressionSyntaxErrorKind) -> ExpressionError {
        ExpressionError::Syntax(ExpressionSyntaxError {
            span: token.span(),
            kind,
        })
    }
}

fn parse_unsigned_i32(source: &str, span: SourceSpan) -> Result<i32, ExpressionError> {
    let mut value: i32 = 0;
    let mut saw_digit = false;

    for byte in source.bytes() {
        let Some(digit) = byte.checked_sub(b'0').filter(|digit| *digit <= 9) else {
            return Err(ExpressionError::Syntax(ExpressionSyntaxError {
                span,
                kind: ExpressionSyntaxErrorKind::IntegerLiteralConversion,
            }));
        };

        saw_digit = true;
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(i32::from(digit)))
            .ok_or(ExpressionError::Syntax(ExpressionSyntaxError {
                span,
                kind: ExpressionSyntaxErrorKind::IntegerLiteralOutOfRange,
            }))?;
        if value > 32768 {
            return Err(ExpressionError::Syntax(ExpressionSyntaxError {
                span,
                kind: ExpressionSyntaxErrorKind::IntegerLiteralOutOfRange,
            }));
        }
    }

    if !saw_digit {
        return Err(ExpressionError::Syntax(ExpressionSyntaxError {
            span,
            kind: ExpressionSyntaxErrorKind::IntegerLiteralConversion,
        }));
    }

    Ok(value)
}

fn binary_operator(kind: TokenKind) -> Option<BinaryOperator> {
    match kind {
        TokenKind::Star => Some(binary(
            OperatorSemantic::Multiply,
            PRECEDENCE_MULTIPLICATIVE,
            false,
        )),
        TokenKind::Slash => Some(binary(
            OperatorSemantic::Divide,
            PRECEDENCE_MULTIPLICATIVE,
            false,
        )),
        TokenKind::Percent => Some(binary(
            OperatorSemantic::Remainder,
            PRECEDENCE_MULTIPLICATIVE,
            false,
        )),
        TokenKind::Plus => Some(binary(OperatorSemantic::Add, PRECEDENCE_ADDITIVE, false)),
        TokenKind::Minus => Some(binary(
            OperatorSemantic::Subtract,
            PRECEDENCE_ADDITIVE,
            false,
        )),
        TokenKind::Equal => Some(binary(OperatorSemantic::Equal, PRECEDENCE_COMPARISON, true)),
        TokenKind::NotEqual => Some(binary(
            OperatorSemantic::NotEqual,
            PRECEDENCE_COMPARISON,
            true,
        )),
        TokenKind::Less => Some(binary(OperatorSemantic::Less, PRECEDENCE_COMPARISON, true)),
        TokenKind::LessEqual => Some(binary(
            OperatorSemantic::LessEqual,
            PRECEDENCE_COMPARISON,
            true,
        )),
        TokenKind::Greater => Some(binary(
            OperatorSemantic::Greater,
            PRECEDENCE_COMPARISON,
            true,
        )),
        TokenKind::GreaterEqual => Some(binary(
            OperatorSemantic::GreaterEqual,
            PRECEDENCE_COMPARISON,
            true,
        )),
        _ => None,
    }
}

const fn binary(semantic: OperatorSemantic, precedence: u8, comparison: bool) -> BinaryOperator {
    BinaryOperator {
        semantic,
        precedence,
        comparison,
    }
}

const fn is_expression_terminator(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::LineBoundary | TokenKind::Eof)
}

const fn unexpected_token(token: Token) -> ExpressionSyntaxErrorKind {
    ExpressionSyntaxErrorKind::UnexpectedToken { kind: token.kind() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::InstructionAddress;
    use crate::lexer::Lexer;
    use crate::operator::register_operator_primitives;
    use crate::primitive::PrimitiveRegistry;
    use crate::source::{SourceId, SourceTexts};
    use crate::word::PublishedWords;
    use std::collections::HashMap;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn lex(view: SourceView<'_>, source_id: SourceId) -> Vec<Token> {
        let mut lexer = Lexer::new(view, source_id).expect("lexer should build");
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token().expect("source should lex");
            tokens.push(token);
            if token.kind() == TokenKind::Eof {
                break;
            }
        }

        tokens
    }

    fn operators() -> OperatorLookup {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        register_operator_primitives(&mut primitives, &mut words).lookup()
    }

    fn parse(text: &str) -> (SourceTexts, SourceId, ExpressionStaging) {
        parse_with_variables(text, empty_variables())
    }

    fn parse_with_variables(
        text: &str,
        variables: impl ExpressionVariableResolver,
    ) -> (SourceTexts, SourceId, ExpressionStaging) {
        let (sources, id) = source(text);
        let tokens = lex(sources.view(), id);
        let staging = parse_expression(sources.view(), &tokens, operators(), &variables)
            .expect("expression should parse");
        (sources, id, staging)
    }

    fn parse_error(text: &str) -> (SourceTexts, SourceId, ExpressionError) {
        parse_error_with_variables(text, empty_variables())
    }

    fn parse_error_with_variables(
        text: &str,
        variables: impl ExpressionVariableResolver,
    ) -> (SourceTexts, SourceId, ExpressionError) {
        let (sources, id) = source(text);
        let tokens = lex(sources.view(), id);
        let error = parse_expression(sources.view(), &tokens, operators(), &variables)
            .expect_err("expression should fail");
        (sources, id, error)
    }

    fn empty_variables() -> impl ExpressionVariableResolver {
        |_source_name: &str| Err(ExpressionVariableErrorKind::UndefinedName)
    }

    fn variables(cases: &[(&str, GlobalVarId)]) -> impl ExpressionVariableResolver {
        let variables = cases
            .iter()
            .map(|(name, id)| (name.to_ascii_uppercase(), *id))
            .collect::<HashMap<_, _>>();

        move |source_name: &str| {
            variables
                .get(&source_name.to_ascii_uppercase())
                .copied()
                .ok_or(ExpressionVariableErrorKind::UndefinedName)
        }
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn instructions(staging: &ExpressionStaging) -> Vec<Instruction> {
        staging
            .entries()
            .iter()
            .map(|entry| entry.instruction())
            .collect()
    }

    fn spans(staging: &ExpressionStaging) -> Vec<SourceSpan> {
        staging.entries().iter().map(|entry| entry.span()).collect()
    }

    fn call(lookup: OperatorLookup, semantic: OperatorSemantic) -> Instruction {
        Instruction::Call(lookup.resolve(semantic))
    }

    fn assert_syntax_error(
        error: ExpressionError,
        expected_span: SourceSpan,
        expected_kind: ExpressionSyntaxErrorKind,
    ) {
        let ExpressionError::Syntax(error) = error else {
            panic!("expected syntax error");
        };

        assert_eq!(error.span(), expected_span);
        assert_eq!(error.kind(), expected_kind);
    }

    fn assert_variable_error(
        error: ExpressionError,
        expected_span: SourceSpan,
        expected_kind: ExpressionVariableErrorKind,
    ) {
        let ExpressionError::Variable(error) = error else {
            panic!("expected variable resolution error");
        };

        assert_eq!(error.span(), expected_span);
        assert_eq!(error.kind(), expected_kind);
    }

    #[test]
    fn arithmetic_precedence_and_left_associativity_emit_postfix_calls() {
        let lookup = operators();
        let (_sources, _id, staging) = parse("1+2*3-4");

        assert_eq!(
            instructions(&staging),
            [
                Instruction::Push(value(1)),
                Instruction::Push(value(2)),
                Instruction::Push(value(3)),
                call(lookup, OperatorSemantic::Multiply),
                call(lookup, OperatorSemantic::Add),
                Instruction::Push(value(4)),
                call(lookup, OperatorSemantic::Subtract),
            ]
        );
    }

    #[test]
    fn grouping_overrides_precedence_without_emitting_parenthesis_instructions() {
        let lookup = operators();
        let (sources, id, staging) = parse("(1+2)*3");
        let view = sources.view();

        assert_eq!(
            instructions(&staging),
            [
                Instruction::Push(value(1)),
                Instruction::Push(value(2)),
                call(lookup, OperatorSemantic::Add),
                Instruction::Push(value(3)),
                call(lookup, OperatorSemantic::Multiply),
            ]
        );
        assert_eq!(
            spans(&staging),
            [
                span(view, id, 1, 2),
                span(view, id, 3, 4),
                span(view, id, 2, 3),
                span(view, id, 6, 7),
                span(view, id, 5, 6),
            ]
        );
    }

    #[test]
    fn unary_minus_binds_tighter_than_multiplication() {
        let lookup = operators();
        let (sources, id, staging) = parse("-1*2");
        let view = sources.view();

        assert_eq!(
            instructions(&staging),
            [
                Instruction::Push(value(1)),
                call(lookup, OperatorSemantic::Negate),
                Instruction::Push(value(2)),
                call(lookup, OperatorSemantic::Multiply),
            ]
        );
        assert_eq!(
            spans(&staging),
            [
                span(view, id, 1, 2),
                span(view, id, 0, 1),
                span(view, id, 3, 4),
                span(view, id, 2, 3),
            ]
        );
    }

    #[test]
    fn all_binary_operator_kinds_lower_through_operator_lookup() {
        let cases = [
            ("1+2", OperatorSemantic::Add),
            ("1-2", OperatorSemantic::Subtract),
            ("1*2", OperatorSemantic::Multiply),
            ("1/2", OperatorSemantic::Divide),
            ("1%2", OperatorSemantic::Remainder),
            ("1=2", OperatorSemantic::Equal),
            ("1<>2", OperatorSemantic::NotEqual),
            ("1<2", OperatorSemantic::Less),
            ("1<=2", OperatorSemantic::LessEqual),
            ("1>2", OperatorSemantic::Greater),
            ("1>=2", OperatorSemantic::GreaterEqual),
        ];
        let lookup = operators();

        for (source, semantic) in cases {
            let (_sources, _id, staging) = parse(source);

            assert_eq!(
                instructions(&staging),
                [
                    Instruction::Push(value(1)),
                    Instruction::Push(value(2)),
                    call(lookup, semantic),
                ],
                "{source:?} should lower through operator lookup"
            );
        }
    }

    #[test]
    fn comparison_chain_is_rejected_without_rejecting_parenthesized_comparison() {
        let (sources, id, error) = parse_error("1<2<3");
        assert_syntax_error(
            error,
            span(sources.view(), id, 3, 4),
            ExpressionSyntaxErrorKind::ComparisonChain,
        );

        let (_sources, _id, staging) = parse("1<(2<3)");
        assert_eq!(staging.len(), 5);
    }

    #[test]
    fn min_integer_special_case_is_direct_literal_lowering() {
        let (sources, id, staging) = parse("-32768");

        assert_eq!(instructions(&staging), [Instruction::Push(value(i16::MIN))]);
        assert_eq!(spans(&staging), [span(sources.view(), id, 1, 6)]);
    }

    #[test]
    fn positive_32768_and_larger_negative_magnitude_are_compile_time_range_errors() {
        let (sources, id, error) = parse_error("32768");
        assert_syntax_error(
            error,
            span(sources.view(), id, 0, 5),
            ExpressionSyntaxErrorKind::IntegerLiteralOutOfRange,
        );

        let (sources, id, error) = parse_error("-32769");
        assert_syntax_error(
            error,
            span(sources.view(), id, 1, 6),
            ExpressionSyntaxErrorKind::IntegerLiteralOutOfRange,
        );
    }

    #[test]
    fn malformed_expressions_keep_primary_or_token_spans() {
        let (sources, id, error) = parse_error("1+");
        assert_syntax_error(
            error,
            span(sources.view(), id, 2, 2),
            ExpressionSyntaxErrorKind::MissingOperand,
        );

        let (sources, id, error) = parse_error("*1");
        assert_syntax_error(
            error,
            span(sources.view(), id, 0, 1),
            ExpressionSyntaxErrorKind::UnexpectedToken {
                kind: TokenKind::Star,
            },
        );

        let (sources, id, error) = parse_error("(1+2");
        assert_syntax_error(
            error,
            span(sources.view(), id, 0, 1),
            ExpressionSyntaxErrorKind::UnmatchedParenthesis,
        );

        let (sources, id, error) = parse_error("1)");
        assert_syntax_error(
            error,
            span(sources.view(), id, 1, 2),
            ExpressionSyntaxErrorKind::UnexpectedToken {
                kind: TokenKind::RParen,
            },
        );
    }

    #[test]
    fn name_primary_lowers_to_load_var_with_original_name_span() {
        let variable = GlobalVarId::test_invalid(12);
        let (sources, id, staging) = parse_with_variables("foo", variables(&[("FOO", variable)]));

        assert_eq!(instructions(&staging), [Instruction::LoadVar(variable)]);
        assert_eq!(spans(&staging), [span(sources.view(), id, 0, 3)]);
    }

    #[test]
    fn name_primary_resolution_is_case_insensitive_at_resolver_boundary() {
        let variable = GlobalVarId::test_invalid(3);

        for source in ["A", "a", "Mixed_Case", "mixed_case"] {
            let (_sources, _id, staging) = parse_with_variables(
                source,
                variables(&[("MIXED_CASE", variable), ("A", variable)]),
            );

            assert_eq!(instructions(&staging), [Instruction::LoadVar(variable)]);
        }
    }

    #[test]
    fn name_primary_preserves_postfix_order_with_operators_and_grouping() {
        let lookup = operators();
        let a = GlobalVarId::test_invalid(0);
        let b = GlobalVarId::test_invalid(1);
        let c = GlobalVarId::test_invalid(2);
        let (sources, id, staging) =
            parse_with_variables("(A + B) * C", variables(&[("A", a), ("B", b), ("C", c)]));
        let view = sources.view();

        assert_eq!(
            instructions(&staging),
            [
                Instruction::LoadVar(a),
                Instruction::LoadVar(b),
                call(lookup, OperatorSemantic::Add),
                Instruction::LoadVar(c),
                call(lookup, OperatorSemantic::Multiply),
            ]
        );
        assert_eq!(
            spans(&staging),
            [
                span(view, id, 1, 2),
                span(view, id, 5, 6),
                span(view, id, 3, 4),
                span(view, id, 10, 11),
                span(view, id, 8, 9),
            ]
        );
    }

    #[test]
    fn name_primary_combines_with_unary_and_comparison() {
        let lookup = operators();
        let a = GlobalVarId::test_invalid(0);
        let b = GlobalVarId::test_invalid(1);
        let (_sources, _id, staging) =
            parse_with_variables("-(A) < B", variables(&[("A", a), ("B", b)]));

        assert_eq!(
            instructions(&staging),
            [
                Instruction::LoadVar(a),
                call(lookup, OperatorSemantic::Negate),
                Instruction::LoadVar(b),
                call(lookup, OperatorSemantic::Less),
            ]
        );
    }

    #[test]
    fn unresolved_name_primary_is_structured_variable_error_at_name_span() {
        let (sources, id, error) = parse_error("FOO");

        assert_variable_error(
            error,
            span(sources.view(), id, 0, 3),
            ExpressionVariableErrorKind::UndefinedName,
        );
    }

    #[test]
    fn call_like_name_sequence_still_does_not_fallback_to_call_syntax() {
        let variable = GlobalVarId::test_invalid(4);
        let (sources, id, error) =
            parse_error_with_variables("FOO(1)", variables(&[("FOO", variable)]));

        assert_syntax_error(
            error,
            span(sources.view(), id, 3, 4),
            ExpressionSyntaxErrorKind::UnexpectedToken {
                kind: TokenKind::LParen,
            },
        );
    }

    #[test]
    fn commit_appends_staged_instructions_only_after_parse_success() {
        let (sources, id, staging) = parse("1+2");
        let view = sources.view();
        let mut instructions = InstructionSequence::new();
        let mut mapping = InstructionSourceMapping::new(instructions.code_space());

        staging
            .commit_to(&mut instructions, &mut mapping)
            .expect("staging should commit");

        assert_eq!(instructions.len(), 3);
        assert_eq!(
            instructions.view().get(InstructionAddress::from_index(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            instructions.view().get(InstructionAddress::from_index(1)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            mapping.view().source_span(
                instructions
                    .view()
                    .location(InstructionAddress::from_index(2))
            ),
            Ok(Some(span(view, id, 1, 2)))
        );

        let (_sources, _id, error) = parse_error("1+");
        assert!(matches!(error, ExpressionError::Syntax(_)));
        assert_eq!(instructions.len(), 3);
    }
}
