/// Limited runtime output capability supplied by the host.
///
/// ADR #1528 keeps concrete I/O ownership outside the VM. Runtime words pass
/// only output text whose language-level formatting has already been decided.
pub(crate) trait RuntimeOutput {
    fn write(&mut self, text: &str) -> Result<(), RuntimeOutputError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeOutputError {
    Unavailable,
    Failed,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct TestOutput {
    chunks: Vec<String>,
    fail_next: Option<RuntimeOutputError>,
}

#[cfg(test)]
impl TestOutput {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn fail_next_write(&mut self, error: RuntimeOutputError) {
        self.fail_next = Some(error);
    }

    pub(crate) fn chunks(&self) -> &[String] {
        &self.chunks
    }
}

#[cfg(test)]
impl RuntimeOutput for TestOutput {
    fn write(&mut self, text: &str) -> Result<(), RuntimeOutputError> {
        if let Some(error) = self.fail_next.take() {
            return Err(error);
        }

        self.chunks.push(text.to_owned());
        Ok(())
    }
}
