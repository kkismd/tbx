use std::io::{self, Write};

use super::*;
use crate::binding::Binding;
use crate::runtime_input::TestInput;
use crate::source::{SourceAcquisition, SourceTexts};
use crate::user_facing::UserFacingFailureClass;
use crate::value::Value;

fn name(value: &str) -> crate::name::NormalizedName {
    crate::name::NormalizedName::new(value).expect("test name should be valid")
}

#[derive(Debug, Default)]
struct RecordingWriter {
    bytes: Vec<u8>,
    fail_after_writes: Option<usize>,
    writes: usize,
}

impl RecordingWriter {
    fn failing_after(successful_writes: usize) -> Self {
        Self {
            fail_after_writes: Some(successful_writes),
            ..Self::default()
        }
    }

    fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes).expect("runtime output should be UTF-8")
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_after_writes == Some(self.writes) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }
        self.writes += 1;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn source(text: &str, display_name: &str) -> (SourceTexts, crate::source::SourceId) {
    let mut sources = SourceTexts::new();
    let source_id = sources.register(text, display_name);
    (sources, source_id)
}

fn sources_with_standard_library(
    standard_library: &str,
    source: &str,
) -> (SourceTexts, SourceId, SourceId) {
    let mut sources = SourceTexts::new();
    let standard_library_id = sources.register(standard_library, "<tbx-next-stdlib>");
    let source_id = if source.contains("USE \"state.tbx\"") {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("docs/next/examples/sttr1/main.tbx");
        let canonical_path = std::fs::canonicalize(path)
            .expect("STTR1 entry point should have a canonical filesystem path");
        sources.register_with_acquisition(
            source,
            "docs/next/examples/sttr1/main.tbx",
            SourceAcquisition::FileSystem { canonical_path },
        )
    } else {
        sources.register(source, "program.tbx")
    };
    (sources, standard_library_id, source_id)
}

fn success(result: BatchExecutionResult) -> crate::source_processor::SourceRunResult {
    match result {
        BatchExecutionResult::Success(result) => result,
        BatchExecutionResult::Failure(failure) => {
            panic!("expected success, got {failure:?}")
        }
    }
}

fn failure(result: BatchExecutionResult) -> BatchExecutionFailure {
    match result {
        BatchExecutionResult::Failure(failure) => *failure,
        BatchExecutionResult::Success(result) => {
            panic!("expected failure, got {result:?}")
        }
    }
}

mod additional_sources_filesystem;
mod batch_source_processing;
mod builtin_user_source_words;
mod embedded_standard_library_control;
mod examples_seeded_session;
