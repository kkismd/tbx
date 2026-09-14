use std::io::{self, BufRead};

/// Narrow host capability for one runtime input line.
///
/// The capability owns neither numeric interpretation nor VM state. `None`
/// represents EOF; an error represents an I/O failure.
pub(crate) trait RuntimeInput {
    fn read_line(&mut self) -> Result<Option<String>, RuntimeInputError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeInputError {
    Unavailable,
    Io { kind: io::ErrorKind },
    Failed,
}

#[derive(Debug)]
pub(crate) struct BufReadRuntimeInput<R> {
    reader: R,
}

impl<R> BufReadRuntimeInput<R> {
    pub(crate) const fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: BufRead> RuntimeInput for BufReadRuntimeInput<R> {
    fn read_line(&mut self) -> Result<Option<String>, RuntimeInputError> {
        let mut line = String::new();
        let bytes = self
            .reader
            .read_line(&mut line)
            .map_err(|source| RuntimeInputError::Io {
                kind: source.kind(),
            })?;
        if bytes == 0 {
            return Ok(None);
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(Some(line))
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct TestInput {
    lines: Vec<Result<Option<String>, RuntimeInputError>>,
    next: usize,
}

#[cfg(test)]
impl TestInput {
    pub(crate) fn new(
        lines: impl IntoIterator<Item = Result<Option<String>, RuntimeInputError>>,
    ) -> Self {
        Self {
            lines: lines.into_iter().collect(),
            next: 0,
        }
    }
}

#[cfg(test)]
impl RuntimeInput for TestInput {
    fn read_line(&mut self) -> Result<Option<String>, RuntimeInputError> {
        let result = self.lines.get(self.next).cloned().unwrap_or(Ok(None));
        self.next += 1;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn buf_read_input_removes_lf_and_crlf_but_preserves_content() {
        let mut input = BufReadRuntimeInput::new(Cursor::new(b"42\n-7\r\n"));
        assert_eq!(input.read_line(), Ok(Some("42".into())));
        assert_eq!(input.read_line(), Ok(Some("-7".into())));
        assert_eq!(input.read_line(), Ok(None));
    }
}
