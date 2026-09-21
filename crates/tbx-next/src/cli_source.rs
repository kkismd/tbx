use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::source::{SourceAcquisition, SourceId, SourceTexts};

const STDIN_DISPLAY_NAME: &str = "<stdin>";
pub(crate) const STDLIB_DISPLAY_NAME: &str = "<tbx-next-stdlib>";
pub(crate) const STDLIB_SOURCE: &str = include_str!("../stdlib/basic.tbx");

pub(crate) fn register_embedded_standard_library(sources: &mut SourceTexts) -> SourceId {
    sources.register(STDLIB_SOURCE, STDLIB_DISPLAY_NAME)
}

#[derive(Debug)]
pub(crate) struct InitialSource {
    sources: SourceTexts,
    stdlib_source_id: SourceId,
    source_id: SourceId,
    seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InitialSourceInput {
    Stdin,
    File {
        path: PathBuf,
        display_name: Box<str>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitialSourceArgs {
    input: InitialSourceInput,
    seed: Option<u64>,
}

#[derive(Debug)]
pub(crate) enum CliSourceError {
    Usage(Box<str>),
    ReadFile {
        display_name: Box<str>,
        source: io::Error,
    },
    CanonicalizeFile {
        display_name: Box<str>,
        source: io::Error,
    },
    ReadStdin {
        source: io::Error,
    },
}

impl InitialSource {
    pub(crate) fn sources(&self) -> &SourceTexts {
        &self.sources
    }

    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub(crate) const fn stdlib_source_id(&self) -> SourceId {
        self.stdlib_source_id
    }

    pub(crate) const fn seed(&self) -> Option<u64> {
        self.seed
    }

    pub(crate) fn into_parts(self) -> (SourceTexts, SourceId, SourceId, Option<u64>) {
        (
            self.sources,
            self.stdlib_source_id,
            self.source_id,
            self.seed,
        )
    }
}

pub(crate) fn acquire_from_env() -> Result<InitialSource, CliSourceError> {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    acquire_initial_source(env::args_os().skip(1), &mut stdin, |path| {
        fs::read_to_string(path)
    })
}

pub(crate) fn acquire_initial_source<I, S, R, F>(
    args: I,
    stdin: &mut R,
    read_file: F,
) -> Result<InitialSource, CliSourceError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: Read,
    F: FnOnce(&Path) -> io::Result<String>,
{
    acquire_initial_source_with_canonicalizer(args, stdin, read_file, |path| Ok(path.to_path_buf()))
}

pub(crate) fn acquire_initial_source_with_canonicalizer<I, S, R, F, C>(
    args: I,
    stdin: &mut R,
    read_file: F,
    canonicalize_file: C,
) -> Result<InitialSource, CliSourceError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: Read,
    F: FnOnce(&Path) -> io::Result<String>,
    C: FnOnce(&Path) -> io::Result<PathBuf>,
{
    let InitialSourceArgs { input, seed } = parse_initial_source_args(args)?;
    let mut sources = SourceTexts::new();
    let stdlib_source_id = register_embedded_standard_library(&mut sources);

    let source_id = match input {
        InitialSourceInput::Stdin => {
            let mut text = String::new();
            stdin
                .read_to_string(&mut text)
                .map_err(|source| CliSourceError::ReadStdin { source })?;
            sources.register(text, STDIN_DISPLAY_NAME)
        }
        InitialSourceInput::File { path, display_name } => {
            let canonical_path =
                canonicalize_file(&path).map_err(|source| CliSourceError::CanonicalizeFile {
                    display_name: display_name.clone(),
                    source,
                })?;
            let text = read_file(&canonical_path).map_err(|source| CliSourceError::ReadFile {
                display_name: display_name.clone(),
                source,
            })?;
            sources.register_with_acquisition(
                text,
                display_name,
                SourceAcquisition::FileSystem { canonical_path },
            )
        }
    };

    Ok(InitialSource {
        sources,
        stdlib_source_id,
        source_id,
        seed,
    })
}

fn parse_initial_source_args<I, S>(args: I) -> Result<InitialSourceArgs, CliSourceError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let Some(first) = args.next() else {
        return Ok(InitialSourceArgs {
            input: InitialSourceInput::Stdin,
            seed: None,
        });
    };

    let (seed, source) = if first == "--seed" {
        let value = args.next().ok_or_else(|| {
            CliSourceError::Usage("expected a decimal value after `--seed`".into())
        })?;
        let seed = value.to_string_lossy().parse::<u64>().map_err(|_| {
            CliSourceError::Usage("seed must be a decimal integer in `0..=u64::MAX`".into())
        })?;
        (Some(seed), args.next())
    } else if first.to_string_lossy().starts_with("--") {
        return Err(CliSourceError::Usage("unknown option".into()));
    } else {
        (None, Some(first))
    };

    let Some(source) = source else {
        if args.next().is_some() {
            return Err(CliSourceError::Usage("duplicate `--seed` option".into()));
        }
        return Ok(InitialSourceArgs {
            input: InitialSourceInput::Stdin,
            seed,
        });
    };

    if args.next().is_some() {
        return Err(CliSourceError::Usage(
            "expected at most one source file; use `--seed <seed> [source]`".into(),
        ));
    }

    if source.to_string_lossy().starts_with("--") {
        return Err(CliSourceError::Usage(
            "seed option must precede the source file".into(),
        ));
    }

    let display_name = source.to_string_lossy().into_owned().into_boxed_str();
    Ok(InitialSourceArgs {
        input: InitialSourceInput::File {
            path: PathBuf::from(source),
            display_name,
        },
        seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug)]
    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("stdin failed"))
        }
    }

    #[test]
    fn zero_args_reads_stdin_to_eof_and_registers_stdin_display_name() {
        let mut stdin = "PUTDEC 1\nPUTDEC 2".as_bytes();

        let acquired = acquire_initial_source(Vec::<OsString>::new(), &mut stdin, |_| {
            panic!("file reader must not be used for stdin")
        })
        .expect("stdin source acquisition should succeed");

        let view = acquired.sources().view();
        let source_id = acquired.source_id();
        let stdlib_source_id = acquired.stdlib_source_id();
        assert_eq!(view.source(stdlib_source_id), Ok(STDLIB_SOURCE));
        assert_eq!(view.display_name(stdlib_source_id), Ok(STDLIB_DISPLAY_NAME));
        assert_eq!(view.source(source_id), Ok("PUTDEC 1\nPUTDEC 2"));
        assert_eq!(view.display_name(source_id), Ok(STDIN_DISPLAY_NAME));
    }

    #[test]
    fn one_arg_reads_file_and_preserves_input_path_display_name() {
        let mut stdin = io::empty();
        let path_seen = Cell::new(false);

        let acquired = acquire_initial_source(["relative/program.tbx"], &mut stdin, |path| {
            path_seen.set(true);
            assert_eq!(path, Path::new("relative/program.tbx"));
            Ok("PUTDEC 7".to_owned())
        })
        .expect("file source acquisition should succeed");

        let view = acquired.sources().view();
        let source_id = acquired.source_id();
        assert!(path_seen.get());
        assert_eq!(acquired.sources().len(), 2);
        assert_eq!(view.source(source_id), Ok("PUTDEC 7"));
        assert_eq!(view.display_name(source_id), Ok("relative/program.tbx"));
    }

    #[test]
    fn two_or_more_args_fail_before_source_acquisition() {
        let mut stdin = "must not be read".as_bytes();
        let file_reader_called = Cell::new(false);

        let result = acquire_initial_source(["a.tbx", "b.tbx"], &mut stdin, |_| {
            file_reader_called.set(true);
            Ok("must not be read".to_owned())
        });

        assert!(matches!(result, Err(CliSourceError::Usage(_))));
        assert!(!file_reader_called.get());
    }

    #[test]
    fn seed_option_is_parsed_before_a_file_or_stdin_source() {
        let mut stdin = io::empty();
        let acquired =
            acquire_initial_source(["--seed", "42", "program.tbx"], &mut stdin, |path| {
                assert_eq!(path, Path::new("program.tbx"));
                Ok("PUTDEC 1".to_owned())
            })
            .expect("seeded file source should be accepted");

        assert_eq!(acquired.seed(), Some(42));
    }

    #[test]
    fn invalid_seed_is_rejected_before_source_acquisition() {
        let mut stdin = "must not be read".as_bytes();
        let file_reader_called = Cell::new(false);

        let result = acquire_initial_source(
            ["--seed", "not-a-number", "program.tbx"],
            &mut stdin,
            |_| {
                file_reader_called.set(true);
                Ok("must not be read".to_owned())
            },
        );

        assert!(matches!(result, Err(CliSourceError::Usage(_))));
        assert!(!file_reader_called.get());
    }

    #[test]
    fn duplicate_seed_is_rejected_before_source_acquisition() {
        let mut stdin = io::empty();
        let result = acquire_initial_source(["--seed", "1", "--seed", "2"], &mut stdin, |_| {
            panic!("file reader must not be used")
        });

        assert!(matches!(result, Err(CliSourceError::Usage(_))));
    }

    #[test]
    fn file_read_failure_is_a_cli_acquisition_failure() {
        let mut stdin = io::empty();

        let result = acquire_initial_source(["missing.tbx"], &mut stdin, |_| {
            Err(io::Error::new(io::ErrorKind::NotFound, "not found"))
        });

        match result {
            Err(CliSourceError::ReadFile {
                display_name,
                source,
            }) => {
                assert_eq!(display_name.as_ref(), "missing.tbx");
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("expected file read failure, got {other:?}"),
        }
    }

    #[test]
    fn stdin_read_failure_is_a_cli_acquisition_failure() {
        let mut stdin = FailingReader;

        let result = acquire_initial_source(Vec::<OsString>::new(), &mut stdin, |_| {
            panic!("file reader must not be used for stdin")
        });

        match result {
            Err(CliSourceError::ReadStdin { source }) => {
                assert_eq!(source.kind(), io::ErrorKind::Other);
            }
            other => panic!("expected stdin read failure, got {other:?}"),
        }
    }

    #[test]
    fn dash_is_preserved_as_an_ordinary_file_path() {
        let mut stdin = "must not be read".as_bytes();

        let acquired = acquire_initial_source(["-"], &mut stdin, |path| {
            assert_eq!(path, Path::new("-"));
            Ok("PUTDEC 9".to_owned())
        })
        .expect("dash should be treated as a file path");

        let view = acquired.sources().view();
        let source_id = acquired.source_id();
        assert_eq!(view.source(source_id), Ok("PUTDEC 9"));
        assert_eq!(view.display_name(source_id), Ok("-"));
    }
}
