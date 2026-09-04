use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use crate::batch_execution::{execute_registered_source, BatchExecutionResult};
use crate::cli_source::{acquire_initial_source, CliSourceError};
use crate::diagnostic::{DiagnosticRenderer, RenderedDiagnostic, UserDiagnostic};
use crate::source::SourceTexts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessStatus {
    Success,
    Failure,
}

impl ProcessStatus {
    const fn exit_code(self) -> ExitCode {
        match self {
            Self::Success => ExitCode::SUCCESS,
            Self::Failure => ExitCode::FAILURE,
        }
    }
}

pub(crate) fn run_from_env() -> ExitCode {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stderr = io::stderr();
    let mut stderr = stderr.lock();

    run_with_io(
        env::args_os().skip(1),
        &mut stdin,
        &mut stdout,
        &mut stderr,
        |path| fs::read_to_string(path),
    )
    .exit_code()
}

fn run_with_io<I, S, R, O, E, F>(
    args: I,
    stdin: &mut R,
    stdout: &mut O,
    stderr: &mut E,
    read_file: F,
) -> ProcessStatus
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: Read,
    O: Write + ?Sized,
    E: Write + ?Sized,
    F: FnOnce(&Path) -> io::Result<String>,
{
    let source = match acquire_initial_source(args, stdin, read_file) {
        Ok(source) => source,
        Err(error) => {
            let diagnostic = acquisition_diagnostic(&error);
            return write_diagnostic(stderr, &diagnostic);
        }
    };

    match execute_registered_source(source.sources(), source.source_id(), stdout) {
        BatchExecutionResult::Success(_) => ProcessStatus::Success,
        BatchExecutionResult::Failure(failure) => write_diagnostic(stderr, failure.diagnostic()),
    }
}

fn acquisition_diagnostic(error: &CliSourceError) -> RenderedDiagnostic {
    let diagnostic = match error {
        CliSourceError::Usage => {
            UserDiagnostic::without_source("invalid arguments", "expected at most one source file")
        }
        CliSourceError::ReadFile {
            display_name,
            source,
        } => UserDiagnostic::without_source(
            format!("failed to read `{display_name}`"),
            source.to_string(),
        ),
        CliSourceError::ReadStdin { source } => UserDiagnostic::without_source(
            "standard input",
            format!("failed to read source: {source}"),
        ),
    };

    let sources = SourceTexts::new();
    DiagnosticRenderer::new(sources.view())
        .render(&diagnostic)
        .expect("source-less acquisition diagnostic must render")
}

fn write_diagnostic<W>(stderr: &mut W, diagnostic: &RenderedDiagnostic) -> ProcessStatus
where
    W: Write + ?Sized,
{
    if write!(stderr, "{diagnostic}").is_err() {
        return ProcessStatus::Failure;
    }

    if stderr.flush().is_err() {
        return ProcessStatus::Failure;
    }

    ProcessStatus::Failure
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, Default)]
    struct RecordingWriter {
        bytes: Vec<u8>,
        fail_write: bool,
    }

    impl RecordingWriter {
        fn failing() -> Self {
            Self {
                fail_write: true,
                ..Self::default()
            }
        }

        fn text(&self) -> &str {
            std::str::from_utf8(&self.bytes).expect("process output should be UTF-8")
        }
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_write {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("stdin failed"))
        }
    }

    #[test]
    fn zero_args_reads_stdin_and_routes_runtime_output_to_stdout() {
        let mut stdin = "EVAL 7\nPRINT\nCR".as_bytes();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io(
            Vec::<OsString>::new(),
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| panic!("file reader must not be used for stdin"),
        );

        assert_eq!(status, ProcessStatus::Success);
        assert_eq!(stdout.text(), "7\n");
        assert_eq!(stderr.text(), "");
    }

    #[test]
    fn one_arg_reads_file_and_uses_the_same_batch_execution_path() {
        let mut stdin = "must not be read".as_bytes();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();
        let file_reader_called = Cell::new(false);

        let status = run_with_io(
            ["relative/program.tbx"],
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |path| {
                file_reader_called.set(true);
                assert_eq!(path, Path::new("relative/program.tbx"));
                Ok("EVAL 5\nPRINT".to_owned())
            },
        );

        assert_eq!(status, ProcessStatus::Success);
        assert!(file_reader_called.get());
        assert_eq!(stdout.text(), "5");
        assert_eq!(stderr.text(), "");
    }

    #[test]
    fn two_or_more_args_emit_source_less_usage_diagnostic_to_stderr() {
        let mut stdin = "must not be read".as_bytes();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();
        let file_reader_called = Cell::new(false);

        let status = run_with_io(
            ["a.tbx", "b.tbx"],
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| {
                file_reader_called.set(true);
                Ok("must not be read".to_owned())
            },
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert!(!file_reader_called.get());
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("invalid arguments"));
        assert!(stderr.text().contains("expected at most one source file"));
        assert!(!stderr.text().contains(":1:1"));
    }

    #[test]
    fn file_acquisition_failure_emits_source_less_diagnostic_to_stderr() {
        let mut stdin = io::empty();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io(
            ["missing.tbx"],
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| Err(io::Error::new(io::ErrorKind::NotFound, "not found")),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("failed to read `missing.tbx`"));
        assert!(stderr.text().contains("not found"));
        assert!(!stderr.text().contains("missing.tbx:1:1"));
    }

    #[test]
    fn stdin_acquisition_failure_emits_source_less_diagnostic_to_stderr() {
        let mut stdin = FailingReader;
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io(
            Vec::<OsString>::new(),
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| panic!("file reader must not be used for stdin"),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("standard input"));
        assert!(stderr.text().contains("failed to read source"));
        assert!(!stderr.text().contains("<stdin>:1:1"));
    }

    #[test]
    fn batch_execution_diagnostic_is_routed_to_stderr() {
        let mut stdin = "UNKNOWN".as_bytes();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io(
            Vec::<OsString>::new(),
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| panic!("file reader must not be used for stdin"),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("<stdin>:1:1"));
        assert!(stderr.text().contains("compile error"));
    }

    #[test]
    fn runtime_output_failure_is_nonzero_and_diagnostic_stays_on_stderr() {
        let mut stdin = "EVAL 1\nPRINT".as_bytes();
        let mut stdout = RecordingWriter::failing();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io(
            Vec::<OsString>::new(),
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| panic!("file reader must not be used for stdin"),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("runtime output failed"));
    }

    #[test]
    fn diagnostic_write_failure_is_nonzero_without_panic() {
        let mut stdin = "UNKNOWN".as_bytes();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::failing();

        let status = run_with_io(
            Vec::<OsString>::new(),
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| panic!("file reader must not be used for stdin"),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
    }
}
