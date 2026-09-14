use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use crate::batch_execution::{
    execute_registered_sources_with_filesystem_and_seed, BatchExecutionResult,
};
use crate::cli_source::{acquire_initial_source_with_canonicalizer, CliSourceError};
use crate::diagnostic::{DiagnosticRenderer, RenderedDiagnostic, UserDiagnostic};
use crate::runtime_input::{BufReadRuntimeInput, RuntimeInput};
use crate::source::{SourceAcquisition, SourceTexts};

#[derive(Debug)]
struct RandomSeedError(getrandom::Error);

impl std::fmt::Display for RandomSeedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{0}", self.0)
    }
}

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

    run_with_io_and_canonicalizer(
        env::args_os().skip(1),
        &mut stdin,
        &mut stdout,
        &mut stderr,
        |path| fs::read_to_string(path),
        |path| fs::canonicalize(path),
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
    run_with_io_and_canonicalizer(args, stdin, stdout, stderr, read_file, |path| {
        Ok(path.to_path_buf())
    })
}

fn run_with_io_and_canonicalizer<I, S, R, O, E, F, C>(
    args: I,
    stdin: &mut R,
    stdout: &mut O,
    stderr: &mut E,
    read_file: F,
    canonicalize_file: C,
) -> ProcessStatus
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: Read,
    O: Write + ?Sized,
    E: Write + ?Sized,
    F: FnOnce(&Path) -> io::Result<String>,
    C: FnOnce(&Path) -> io::Result<std::path::PathBuf>,
{
    run_with_io_and_canonicalizer_with_seed_provider(
        args,
        stdin,
        stdout,
        stderr,
        read_file,
        canonicalize_file,
        acquire_random_seed,
    )
}

fn run_with_io_and_canonicalizer_with_seed_provider<I, S, R, O, E, F, C, G>(
    args: I,
    stdin: &mut R,
    stdout: &mut O,
    stderr: &mut E,
    read_file: F,
    canonicalize_file: C,
    seed_provider: G,
) -> ProcessStatus
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: Read,
    O: Write + ?Sized,
    E: Write + ?Sized,
    F: FnOnce(&Path) -> io::Result<String>,
    C: FnOnce(&Path) -> io::Result<std::path::PathBuf>,
    G: FnOnce() -> Result<u64, RandomSeedError>,
{
    let mut buffered_stdin = BufReader::new(stdin);
    let source = match acquire_initial_source_with_canonicalizer(
        args,
        &mut buffered_stdin,
        read_file,
        canonicalize_file,
    ) {
        Ok(source) => source,
        Err(error) => {
            let diagnostic = acquisition_diagnostic(&error);
            return write_diagnostic(stderr, &diagnostic);
        }
    };

    let (sources, stdlib_source_id, source_id) = source.into_parts();
    let file_source = matches!(
        sources.view().acquisition(source_id),
        Ok(SourceAcquisition::FileSystem { .. })
    );
    let seed = match seed_provider() {
        Ok(seed) => seed,
        Err(error) => {
            let diagnostic = UserDiagnostic::without_source(
                "execution environment",
                format!("failed to acquire random seed: {error}"),
            );
            let diagnostic = DiagnosticRenderer::new(SourceTexts::new().view())
                .render(&diagnostic)
                .expect("source-less seed diagnostic must render");
            return write_diagnostic(stderr, &diagnostic);
        }
    };
    let mut runtime_input = BufReadRuntimeInput::new(buffered_stdin);
    let input = file_source.then_some(&mut runtime_input as &mut dyn RuntimeInput);
    match execute_registered_sources_with_filesystem_and_seed(
        sources,
        stdlib_source_id,
        source_id,
        stdout,
        input,
        seed,
    ) {
        BatchExecutionResult::Success(_) => ProcessStatus::Success,
        BatchExecutionResult::Failure(failure) => write_diagnostic(stderr, failure.diagnostic()),
    }
}

fn acquire_random_seed() -> Result<u64, RandomSeedError> {
    let mut bytes = [0_u8; std::mem::size_of::<u64>()];
    getrandom::getrandom(&mut bytes).map_err(RandomSeedError)?;
    Ok(u64::from_ne_bytes(bytes))
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
        CliSourceError::CanonicalizeFile {
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
        let mut stdin = "EVAL 7\nPUTDEC\nCR".as_bytes();
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
                Ok("EVAL 5\nPUTDEC".to_owned())
            },
        );

        assert_eq!(status, ProcessStatus::Success);
        assert!(file_reader_called.get());
        assert_eq!(stdout.text(), "5");
        assert_eq!(stderr.text(), "");
    }

    #[test]
    fn processing_session_receives_one_host_seed_for_file_execution() {
        let mut stdin = io::empty();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();
        let seed_calls = Cell::new(0);

        let status = run_with_io_and_canonicalizer_with_seed_provider(
            ["relative/program.tbx"],
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| Ok("PUTDEC RND(10)".to_owned()),
            |path| Ok(path.to_path_buf()),
            || {
                seed_calls.set(seed_calls.get() + 1);
                Ok(42)
            },
        );

        assert_eq!(status, ProcessStatus::Success);
        assert_eq!(seed_calls.get(), 1);
        let value = stdout
            .text()
            .parse::<i16>()
            .expect("RND output should be an integer");
        assert!((1..=10).contains(&value));
        assert_eq!(stderr.text(), "");
    }

    #[test]
    fn seed_acquisition_failure_is_an_environment_failure() {
        let mut stdin = io::empty();
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io_and_canonicalizer_with_seed_provider(
            Vec::<OsString>::new(),
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| panic!("source acquisition must not be reached"),
            |path| Ok(path.to_path_buf()),
            || Err(RandomSeedError(getrandom::Error::UNSUPPORTED)),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("execution environment"));
        assert!(stderr.text().contains("failed to acquire random seed"));
    }

    #[test]
    fn file_runtime_input_failure_is_reported_as_a_runtime_failure() {
        let mut stdin = FailingReader;
        let mut stdout = RecordingWriter::default();
        let mut stderr = RecordingWriter::default();

        let status = run_with_io(
            ["relative/program.tbx"],
            &mut stdin,
            &mut stdout,
            &mut stderr,
            |_| Ok("INPUT?".to_owned()),
        );

        assert_eq!(status, ProcessStatus::Failure);
        assert_eq!(stdout.text(), "");
        assert!(stderr.text().contains("runtime error"), "{}", stderr.text());
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
        let mut stdin = "EVAL 1\nPUTDEC".as_bytes();
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
