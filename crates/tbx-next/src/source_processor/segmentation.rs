use crate::lexer::LexError;
use crate::lexer::{Lexer, Token, TokenKind};
use crate::source::{SourceError, SourceId, SourceSpan, SourceView};
use crate::source_word::{
    SourceBlockCursor, SourceBlockRead, SourceBlockStatement, SourceBlockTerminal, SourceWordError,
};

use super::SourceProcessorError;

// Segmentation is the source of truth for top-level statement boundaries.
// Lexical failure keeps already completed statements visible while leaving the
// unbounded tail unavailable to semantic compilation and source-wide analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SegmentedSource {
    completed_statements: Vec<LogicalStatement>,
    incomplete_tail: Vec<Token>,
    terminal: Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LogicalStatement {
    tokens: Vec<Token>,
}

pub(super) trait LogicalStatementView {
    fn tokens(&self) -> &[Token];
    fn span(&self, view: SourceView<'_>, source_id: SourceId) -> Result<SourceSpan, SourceError>;
}

#[derive(Debug)]
pub(super) struct LogicalStatementCursor<'source, 'statements, S> {
    view: SourceView<'source>,
    source_id: SourceId,
    statements: &'statements [S],
    terminal: Terminal,
    pub(super) position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Terminal {
    Eof { span: SourceSpan },
    LexError(LexError),
}

impl SegmentedSource {
    pub(super) fn collect(
        view: SourceView<'_>,
        source_id: SourceId,
    ) -> Result<Self, SourceProcessorError> {
        let mut collector = LogicalStatementCollector::new();
        let mut lexer = Lexer::new(view, source_id)?;

        loop {
            match lexer.next_token() {
                Ok(token) if token.kind() == TokenKind::Eof => {
                    return Ok(collector.finish(Terminal::Eof { span: token.span() }));
                }
                Ok(token) => {
                    if collector.push_token(view, token)? == CollectorAction::SkipLineComment {
                        let token = lexer.skip_line_comment()?;
                        if token.kind() == TokenKind::Eof {
                            return Ok(collector.finish(Terminal::Eof { span: token.span() }));
                        }
                        collector.push_token(view, token)?;
                    }
                }
                Err(error) => return Ok(collector.finish(Terminal::LexError(error))),
            }
        }
    }

    pub(super) fn completed_statements(&self) -> &[LogicalStatement] {
        &self.completed_statements
    }

    pub(super) fn terminal(&self) -> Terminal {
        self.terminal
    }

    #[cfg(test)]
    pub(super) fn incomplete_tail(&self) -> &[Token] {
        &self.incomplete_tail
    }
}

impl LogicalStatement {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens }
    }

    pub(super) fn tokens(&self) -> &[Token] {
        &self.tokens
    }
}

impl LogicalStatementView for LogicalStatement {
    fn tokens(&self) -> &[Token] {
        self.tokens()
    }

    fn span(&self, view: SourceView<'_>, source_id: SourceId) -> Result<SourceSpan, SourceError> {
        let first = self
            .tokens
            .first()
            .expect("logical statements are never empty");
        let last = self
            .tokens
            .last()
            .expect("logical statements are never empty");
        view.span(source_id, first.span().start(), last.span().end())
    }
}

impl LogicalStatementView for SourceBlockStatement<'_> {
    fn tokens(&self) -> &[Token] {
        SourceBlockStatement::tokens(*self)
    }

    fn span(&self, _view: SourceView<'_>, _source_id: SourceId) -> Result<SourceSpan, SourceError> {
        Ok(SourceBlockStatement::span(*self))
    }
}

impl<'source, 'statements, S> LogicalStatementCursor<'source, 'statements, S> {
    pub(super) fn new(
        view: SourceView<'source>,
        source_id: SourceId,
        statements: &'statements [S],
        terminal: Terminal,
    ) -> Self {
        Self {
            view,
            source_id,
            statements,
            terminal,
            position: 0,
        }
    }

    pub(super) fn next_completed_statement(&mut self) -> Option<&'statements S> {
        let statement = self.statements.get(self.position)?;
        self.position += 1;
        Some(statement)
    }
}

impl<'statements, S> SourceBlockCursor<'statements> for LogicalStatementCursor<'_, 'statements, S>
where
    S: LogicalStatementView + 'statements,
{
    fn read_next_block_statement(
        &mut self,
    ) -> Result<SourceBlockRead<'statements>, SourceWordError> {
        let Some(statement) = self.next_completed_statement() else {
            return Ok(SourceBlockRead::Terminal(match self.terminal {
                Terminal::Eof { span } => SourceBlockTerminal::Eof { span },
                Terminal::LexError(error) => SourceBlockTerminal::LexError { error },
            }));
        };

        let span = statement
            .span(self.view, self.source_id)
            .map_err(|source| SourceWordError::Source { source })?;
        Ok(SourceBlockRead::Statement(SourceBlockStatement::new(
            statement.tokens(),
            span,
        )))
    }
}

#[derive(Debug, Default)]
struct LogicalStatementCollector {
    completed_statements: Vec<LogicalStatement>,
    current_tokens: Vec<Token>,
    depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectorAction {
    Continue,
    SkipLineComment,
}

impl LogicalStatementCollector {
    fn new() -> Self {
        Self {
            completed_statements: Vec::new(),
            current_tokens: Vec::new(),
            depth: 0,
        }
    }

    fn push_token(
        &mut self,
        view: SourceView<'_>,
        token: Token,
    ) -> Result<CollectorAction, SourceProcessorError> {
        if self.is_statement_leading_rem(view, token)? {
            return Ok(CollectorAction::SkipLineComment);
        }

        match token.kind() {
            TokenKind::LParen => {
                self.depth = self.depth.saturating_add(1);
                self.current_tokens.push(token);
            }
            TokenKind::RParen => {
                self.depth = self.depth.saturating_sub(1);
                self.current_tokens.push(token);
            }
            TokenKind::LineBoundary if self.depth == 0 => self.finish_current_statement(),
            _ => self.current_tokens.push(token),
        }

        Ok(CollectorAction::Continue)
    }

    fn is_statement_leading_rem(
        &self,
        view: SourceView<'_>,
        token: Token,
    ) -> Result<bool, SourceProcessorError> {
        if token.kind() != TokenKind::Name || !self.current_tokens.is_empty() {
            return Ok(false);
        }

        Ok(view.slice(token.span())?.eq_ignore_ascii_case("REM"))
    }

    fn finish(mut self, terminal: Terminal) -> SegmentedSource {
        if matches!(terminal, Terminal::Eof { .. }) {
            self.finish_current_statement();
        }

        SegmentedSource {
            completed_statements: self.completed_statements,
            incomplete_tail: self.current_tokens,
            terminal,
        }
    }

    fn finish_current_statement(&mut self) {
        if self.current_tokens.is_empty() {
            return;
        }

        let tokens = std::mem::take(&mut self.current_tokens);
        self.completed_statements
            .push(LogicalStatement::new(tokens));
    }
}
