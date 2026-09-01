use crate::global_variable::GlobalVarId;
use crate::instruction::Instruction;
use crate::instruction_builder::{InstructionBuildError, InstructionBuildTarget};
use crate::lexer::{Token, TokenKind};
use crate::operator::{OperatorLookup, OperatorSemantic};
use crate::source::{SourceError, SourceSpan, SourceView};
use crate::value::Value;
use crate::word::WordId;

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
    Call(ExpressionCallError),
    InstructionBuild(InstructionBuildError),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpressionCallError {
    span: SourceSpan,
    kind: ExpressionCallErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpressionCallErrorKind {
    InvalidName,
    UndefinedName,
    TargetIsNotRuntimeWord,
}

pub(crate) trait ExpressionVariableResolver {
    fn resolve_variable(
        &self,
        source_name: &str,
    ) -> Result<GlobalVarId, ExpressionVariableErrorKind>;
}

pub(crate) trait ExpressionRuntimeWordResolver {
    fn resolve_runtime_word(&self, source_name: &str) -> Result<WordId, ExpressionCallErrorKind>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedExpression {
    contains_comparison: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BinaryOperator {
    semantic: Option<OperatorSemantic>,
    precedence: u8,
    comparison: bool,
}

#[derive(Clone, Copy)]
struct ExpressionParser<'a, 'r> {
    view: SourceView<'a>,
    tokens: &'a [Token],
    operators: OperatorLookup,
    variables: &'r dyn ExpressionVariableResolver,
    runtime_words: &'r dyn ExpressionRuntimeWordResolver,
    position: usize,
}

const PRECEDENCE_COMMA: u8 = 0;
const PRECEDENCE_COMPARISON: u8 = 1;
const PRECEDENCE_ADDITIVE: u8 = 2;
const PRECEDENCE_MULTIPLICATIVE: u8 = 3;
const PRECEDENCE_PREFIX: u8 = 4;

pub(crate) fn parse_expression(
    view: SourceView<'_>,
    tokens: &[Token],
    operators: OperatorLookup,
    variables: &dyn ExpressionVariableResolver,
    runtime_words: &dyn ExpressionRuntimeWordResolver,
) -> Result<ExpressionStaging, ExpressionError> {
    let mut parser = ExpressionParser::new(view, tokens, operators, variables, runtime_words);
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

impl<F> ExpressionRuntimeWordResolver for F
where
    F: Fn(&str) -> Result<WordId, ExpressionCallErrorKind>,
{
    fn resolve_runtime_word(&self, source_name: &str) -> Result<WordId, ExpressionCallErrorKind> {
        self(source_name)
    }
}

impl ExpressionStaging {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn append_mapped_instruction(&mut self, instruction: Instruction, span: SourceSpan) {
        self.entries.push(StagedInstruction { instruction, span });
    }

    pub(crate) fn commit_to(
        &self,
        target: &mut dyn InstructionBuildTarget,
    ) -> Result<(), ExpressionError> {
        for entry in &self.entries {
            target
                .append_mapped(entry.instruction, entry.span)
                .map_err(ExpressionError::InstructionBuild)?;
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

impl ExpressionCallError {
    pub(crate) const fn span(self) -> SourceSpan {
        self.span
    }

    pub(crate) const fn kind(self) -> ExpressionCallErrorKind {
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
        runtime_words: &'r dyn ExpressionRuntimeWordResolver,
    ) -> Self {
        Self {
            view,
            tokens,
            operators,
            variables,
            runtime_words,
            position: 0,
        }
    }

    fn parse_complete(&mut self) -> Result<ExpressionStaging, ExpressionError> {
        let mut staging = ExpressionStaging::new();
        self.parse_infix_expression(&mut staging, PRECEDENCE_COMMA)?;

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
            if let Some(semantic) = operator.semantic {
                staging.append_mapped_instruction(
                    Instruction::Call(self.operators.resolve(semantic)),
                    operator_token.span(),
                );
            }
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
            staging.append_mapped_instruction(
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
        let parsed = if self
            .peek()
            .is_some_and(|token| token.kind() == TokenKind::Name)
        {
            self.parse_name_primary_or_call(staging)?
        } else {
            self.parse_primary(staging)?
        };

        Ok(parsed)
    }

    fn parse_name_primary_or_call(
        &mut self,
        staging: &mut ExpressionStaging,
    ) -> Result<ParsedExpression, ExpressionError> {
        let name = self.advance();
        let source_name = self.view.slice(name.span())?;

        if self
            .peek()
            .is_some_and(|token| token.kind() == TokenKind::LParen)
        {
            let word = self
                .runtime_words
                .resolve_runtime_word(source_name)
                .map_err(|kind| {
                    ExpressionError::Call(ExpressionCallError {
                        span: name.span(),
                        kind,
                    })
                })?;
            let parsed = self.parse_call_arguments(staging)?;
            staging.append_mapped_instruction(Instruction::Call(word), name.span());
            return Ok(parsed);
        }

        let id = self
            .variables
            .resolve_variable(source_name)
            .map_err(|kind| {
                ExpressionError::Variable(ExpressionVariableError {
                    span: name.span(),
                    kind,
                })
            })?;
        staging.append_mapped_instruction(Instruction::LoadVar(id), name.span());
        Ok(ParsedExpression {
            contains_comparison: false,
        })
    }

    fn parse_call_arguments(
        &mut self,
        staging: &mut ExpressionStaging,
    ) -> Result<ParsedExpression, ExpressionError> {
        let lparen = self.advance();
        debug_assert_eq!(lparen.kind(), TokenKind::LParen);

        if self
            .peek()
            .is_some_and(|token| token.kind() == TokenKind::RParen)
        {
            self.advance();
            return Ok(ParsedExpression {
                contains_comparison: false,
            });
        }

        // ADR #1575 defines this as zero or one ordinary expression. Commas are
        // not argument separators, and value counts are deliberately not tracked.
        let parsed = self.parse_infix_expression(staging, PRECEDENCE_COMMA)?;
        self.consume_rparen(lparen)?;
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
                staging.append_mapped_instruction(
                    Instruction::Push(Value::integer(value)),
                    token.span(),
                );
                Ok(ParsedExpression {
                    contains_comparison: false,
                })
            }
            TokenKind::LParen => {
                let lparen = self.advance();
                let parsed = self.parse_infix_expression(staging, PRECEDENCE_COMMA)?;
                self.consume_rparen(lparen)?;
                Ok(parsed)
            }
            TokenKind::Eof | TokenKind::LineBoundary | TokenKind::RParen => {
                Err(self.syntax(token, ExpressionSyntaxErrorKind::MissingOperand))
            }
            _ => Err(self.syntax(token, unexpected_token(token))),
        }
    }

    fn consume_rparen(&mut self, lparen: Token) -> Result<(), ExpressionError> {
        match self.peek() {
            Some(token) if token.kind() == TokenKind::RParen => {
                self.advance();
                Ok(())
            }
            Some(token) if is_expression_terminator(token.kind()) => {
                Err(self.syntax(lparen, ExpressionSyntaxErrorKind::UnmatchedParenthesis))
            }
            Some(token) => Err(self.syntax(token, unexpected_token(token))),
            None => Err(self.syntax(lparen, ExpressionSyntaxErrorKind::UnmatchedParenthesis)),
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
        staging
            .append_mapped_instruction(Instruction::Push(Value::integer(i16::MIN)), token.span());
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
        TokenKind::Comma => Some(binary_without_instruction(PRECEDENCE_COMMA, false)),
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
        semantic: Some(semantic),
        precedence,
        comparison,
    }
}

const fn binary_without_instruction(precedence: u8, comparison: bool) -> BinaryOperator {
    BinaryOperator {
        semantic: None,
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
    use crate::block_code::BlockCodeBuilder;
    use crate::instruction::InstructionAddress;
    use crate::lexer::Lexer;
    use crate::operator::register_operator_primitives;
    use crate::primitive::PrimitiveRegistry;
    use crate::source::{SourceId, SourceTexts};
    use crate::source_mapping::SourceMappedCode;
    use crate::word::{PublishedWords, WordId};
    use std::cell::Cell;
    use std::collections::HashMap;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text, "test.tbx");
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
        parse_with_resolvers(text, variables, empty_runtime_words())
    }

    fn parse_with_resolvers(
        text: &str,
        variables: impl ExpressionVariableResolver,
        runtime_words: impl ExpressionRuntimeWordResolver,
    ) -> (SourceTexts, SourceId, ExpressionStaging) {
        let (sources, id) = source(text);
        let tokens = lex(sources.view(), id);
        let staging = parse_expression(
            sources.view(),
            &tokens,
            operators(),
            &variables,
            &runtime_words,
        )
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
        parse_error_with_resolvers(text, variables, empty_runtime_words())
    }

    fn parse_error_with_resolvers(
        text: &str,
        variables: impl ExpressionVariableResolver,
        runtime_words: impl ExpressionRuntimeWordResolver,
    ) -> (SourceTexts, SourceId, ExpressionError) {
        let (sources, id) = source(text);
        let tokens = lex(sources.view(), id);
        let error = parse_expression(
            sources.view(),
            &tokens,
            operators(),
            &variables,
            &runtime_words,
        )
        .expect_err("expression should fail");
        (sources, id, error)
    }

    fn empty_variables() -> impl ExpressionVariableResolver {
        |_source_name: &str| Err(ExpressionVariableErrorKind::UndefinedName)
    }

    fn empty_runtime_words() -> impl ExpressionRuntimeWordResolver {
        |_source_name: &str| Err(ExpressionCallErrorKind::UndefinedName)
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

    fn runtime_words(cases: &[(&str, WordId)]) -> impl ExpressionRuntimeWordResolver {
        let words = cases
            .iter()
            .map(|(name, id)| (name.to_ascii_uppercase(), *id))
            .collect::<HashMap<_, _>>();

        move |source_name: &str| {
            words
                .get(&source_name.to_ascii_uppercase())
                .copied()
                .ok_or(ExpressionCallErrorKind::UndefinedName)
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

    fn assert_call_error(
        error: ExpressionError,
        expected_span: SourceSpan,
        expected_kind: ExpressionCallErrorKind,
    ) {
        let ExpressionError::Call(error) = error else {
            panic!("expected call resolution error");
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
    fn comma_expression_evaluates_both_sides_without_emitting_separator_instruction() {
        let lookup = operators();
        let (sources, id, staging) = parse("1 + 2, 3 * 4");
        let view = sources.view();

        assert_eq!(
            instructions(&staging),
            [
                Instruction::Push(value(1)),
                Instruction::Push(value(2)),
                call(lookup, OperatorSemantic::Add),
                Instruction::Push(value(3)),
                Instruction::Push(value(4)),
                call(lookup, OperatorSemantic::Multiply),
            ]
        );
        assert_eq!(
            spans(&staging),
            [
                span(view, id, 0, 1),
                span(view, id, 4, 5),
                span(view, id, 2, 3),
                span(view, id, 7, 8),
                span(view, id, 11, 12),
                span(view, id, 9, 10),
            ]
        );
    }

    #[test]
    fn runtime_word_call_lowers_after_ordinary_pre_call_expression() {
        let lookup = operators();
        let a = GlobalVarId::test_invalid(0);
        let b = GlobalVarId::test_invalid(1);
        let foo = WordId::test_invalid(20);
        let g = WordId::test_invalid(21);
        let (sources, id, staging) = parse_with_resolvers(
            "FOO(G(A), B + 1) * 2",
            variables(&[("A", a), ("B", b)]),
            runtime_words(&[("FOO", foo), ("G", g)]),
        );
        let view = sources.view();

        assert_eq!(
            instructions(&staging),
            [
                Instruction::LoadVar(a),
                Instruction::Call(g),
                Instruction::LoadVar(b),
                Instruction::Push(value(1)),
                call(lookup, OperatorSemantic::Add),
                Instruction::Call(foo),
                Instruction::Push(value(2)),
                call(lookup, OperatorSemantic::Multiply),
            ]
        );
        assert_eq!(
            spans(&staging),
            [
                span(view, id, 6, 7),
                span(view, id, 4, 5),
                span(view, id, 10, 11),
                span(view, id, 14, 15),
                span(view, id, 12, 13),
                span(view, id, 0, 3),
                span(view, id, 19, 20),
                span(view, id, 17, 18),
            ]
        );
    }

    #[test]
    fn zero_argument_runtime_word_call_emits_only_call_at_name_span() {
        let foo = WordId::test_invalid(30);
        let (sources, id, staging) =
            parse_with_resolvers("FOO()", empty_variables(), runtime_words(&[("FOO", foo)]));

        assert_eq!(instructions(&staging), [Instruction::Call(foo)]);
        assert_eq!(spans(&staging), [span(sources.view(), id, 0, 3)]);
    }

    #[test]
    fn call_target_is_resolved_once_before_instruction_staging() {
        let old = WordId::test_invalid(40);
        let new = WordId::test_invalid(41);
        let current = Cell::new(old);
        let resolver = |source_name: &str| {
            assert_eq!(source_name, "FOO");
            let resolved = current.get();
            current.set(new);
            Ok(resolved)
        };
        let (_sources, _id, staging) =
            parse_with_resolvers("FOO() + FOO()", empty_variables(), resolver);

        assert_eq!(
            instructions(&staging),
            [
                Instruction::Call(old),
                Instruction::Call(new),
                call(operators(), OperatorSemantic::Add),
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
    fn runtime_word_call_rejects_undefined_and_non_runtime_targets_at_name_span() {
        let variable = GlobalVarId::test_invalid(4);
        let (sources, id, error) =
            parse_error_with_resolvers("FOO(1)", empty_variables(), |_source_name: &str| {
                Err(ExpressionCallErrorKind::UndefinedName)
            });

        assert_call_error(
            error,
            span(sources.view(), id, 0, 3),
            ExpressionCallErrorKind::UndefinedName,
        );

        let (sources, id, error) = parse_error_with_resolvers(
            "FOO(1)",
            variables(&[("FOO", variable)]),
            |_source_name: &str| Err(ExpressionCallErrorKind::TargetIsNotRuntimeWord),
        );

        assert_call_error(
            error,
            span(sources.view(), id, 0, 3),
            ExpressionCallErrorKind::TargetIsNotRuntimeWord,
        );
    }

    #[test]
    fn call_like_variable_name_sequence_resolves_as_runtime_call_target() {
        let variable = GlobalVarId::test_invalid(4);
        let word = WordId::test_invalid(50);
        let (_sources, _id, staging) = parse_with_resolvers(
            "FOO(1)",
            variables(&[("FOO", variable)]),
            runtime_words(&[("FOO", word)]),
        );

        assert_eq!(
            instructions(&staging),
            [Instruction::Push(value(1)), Instruction::Call(word)]
        );
    }

    #[test]
    fn commit_appends_staged_instructions_only_after_parse_success() {
        let (sources, id, staging) = parse("1+2");
        let view = sources.view();
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);

        staging
            .commit_to(&mut builder)
            .expect("staging should commit");
        builder.finish().expect("block should complete");

        assert_eq!(code.len(), 3);
        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(0)),
            Ok(&Instruction::Push(value(1)))
        );
        assert_eq!(
            code.instruction_view()
                .get(InstructionAddress::from_index(1)),
            Ok(&Instruction::Push(value(2)))
        );
        assert_eq!(
            code.source_mapping().source_span(
                code.instruction_view()
                    .location(InstructionAddress::from_index(2))
            ),
            Ok(Some(span(view, id, 1, 2)))
        );

        let (_sources, _id, error) = parse_error("1+");
        assert!(matches!(error, ExpressionError::Syntax(_)));
        assert_eq!(code.len(), 3);
    }
}
