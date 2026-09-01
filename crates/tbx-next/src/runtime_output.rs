use std::io::{self, Write};

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
    Io {
        operation: RuntimeOutputIoOperation,
        kind: io::ErrorKind,
    },
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeOutputIoOperation {
    Write,
    Flush,
}

#[derive(Debug)]
pub(crate) struct WriteRuntimeOutput<W> {
    writer: W,
}

impl<W> WriteRuntimeOutput<W> {
    pub(crate) const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W> RuntimeOutput for WriteRuntimeOutput<W>
where
    W: Write,
{
    fn write(&mut self, text: &str) -> Result<(), RuntimeOutputError> {
        self.writer
            .write_all(text.as_bytes())
            .map_err(|source| RuntimeOutputError::Io {
                operation: RuntimeOutputIoOperation::Write,
                kind: source.kind(),
            })?;
        self.writer
            .flush()
            .map_err(|source| RuntimeOutputError::Io {
                operation: RuntimeOutputIoOperation::Flush,
                kind: source.kind(),
            })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, Default)]
    struct RecordingWriter {
        chunks: Vec<Vec<u8>>,
        flushes: usize,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.chunks.push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct FailingWriteWriter;

    impl Write for FailingWriteWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct FailingFlushWriter {
        chunks: Vec<Vec<u8>>,
    }

    impl Write for FailingFlushWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.chunks.push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::Interrupted, "flush failed"))
        }
    }

    #[derive(Debug)]
    struct PartialThenFailWriter {
        first_call: Cell<bool>,
        chunks: Vec<Vec<u8>>,
    }

    impl Default for PartialThenFailWriter {
        fn default() -> Self {
            Self {
                first_call: Cell::new(true),
                chunks: Vec::new(),
            }
        }
    }

    impl Write for PartialThenFailWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.first_call.replace(false) {
                let written = buf.len().min(2);
                self.chunks.push(buf[..written].to_vec());
                Ok(written)
            } else {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"))
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_runtime_output_writes_single_completed_chunk() {
        let writer = RecordingWriter::default();
        let mut output = WriteRuntimeOutput::new(writer);

        output
            .write("PRINT text")
            .expect("completed output should write");

        assert_eq!(output.writer.chunks, [b"PRINT text".to_vec()]);
        assert_eq!(output.writer.flushes, 1);
    }

    #[test]
    fn write_runtime_output_preserves_multiple_write_order() {
        let writer = RecordingWriter::default();
        let mut output = WriteRuntimeOutput::new(writer);

        output.write("42").expect("first output should write");
        output.write("\n").expect("CR output should write");
        output.write("-7").expect("third output should write");

        assert_eq!(
            output.writer.chunks,
            [b"42".to_vec(), b"\n".to_vec(), b"-7".to_vec()]
        );
        assert_eq!(output.writer.flushes, 3);
    }

    #[test]
    fn write_runtime_output_does_not_format_completed_text() {
        let writer = RecordingWriter::default();
        let mut output = WriteRuntimeOutput::new(writer);

        output
            .write("001 +7 value\n")
            .expect("adapter should deliver text exactly");

        assert_eq!(output.writer.chunks, [b"001 +7 value\n".to_vec()]);
    }

    #[test]
    fn write_runtime_output_structures_write_failure() {
        let mut output = WriteRuntimeOutput::new(FailingWriteWriter);

        let error = output
            .write("unwritten")
            .expect_err("write failure should propagate");

        assert_eq!(
            error,
            RuntimeOutputError::Io {
                operation: RuntimeOutputIoOperation::Write,
                kind: io::ErrorKind::BrokenPipe
            }
        );
    }

    #[test]
    fn write_runtime_output_structures_flush_failure() {
        let writer = FailingFlushWriter::default();
        let mut output = WriteRuntimeOutput::new(writer);

        let error = output
            .write("written before flush failure")
            .expect_err("flush failure should propagate");

        assert_eq!(
            error,
            RuntimeOutputError::Io {
                operation: RuntimeOutputIoOperation::Flush,
                kind: io::ErrorKind::Interrupted
            }
        );
        assert_eq!(
            output.writer.chunks,
            [b"written before flush failure".to_vec()]
        );
    }

    #[test]
    fn write_runtime_output_does_not_rollback_partial_external_write() {
        let writer = PartialThenFailWriter::default();
        let mut output = WriteRuntimeOutput::new(writer);

        let error = output
            .write("abcdef")
            .expect_err("second write_all attempt should fail");

        assert_eq!(
            error,
            RuntimeOutputError::Io {
                operation: RuntimeOutputIoOperation::Write,
                kind: io::ErrorKind::BrokenPipe
            }
        );
        assert_eq!(output.writer.chunks, [b"ab".to_vec()]);
    }
}
