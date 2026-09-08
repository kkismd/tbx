use std::path::{Path, PathBuf};

use crate::source::{SourceError, SourceId, SourceSpan, SourceTexts, SourceView};

/// Acquisition identity is separate from the user-facing source display name.
/// In particular, a canonical path must not replace the path spelling shown in
/// diagnostics (#1643).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceAcquisition {
    Filesystem { canonical_path: Option<PathBuf> },
    NoFilesystemLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceSessionError {
    Source(SourceError),
    MissingAcquisition { source_id: SourceId },
}

/// Owns all source text and its acquisition metadata for one processing run.
///
/// Registration is intentionally the only way to add a source: this keeps the
/// `SourceId`/text/metadata lifetime aligned while later source processing may
/// append sources to the same session.
#[derive(Debug)]
pub(crate) struct SourceProcessingSession {
    sources: SourceTexts,
    acquisitions: Vec<SourceAcquisition>,
}

impl SourceProcessingSession {
    pub(crate) fn new() -> Self {
        Self {
            sources: SourceTexts::new(),
            acquisitions: Vec::new(),
        }
    }

    pub(crate) fn register(
        &mut self,
        text: impl Into<Box<str>>,
        display_name: impl Into<Box<str>>,
        acquisition: SourceAcquisition,
    ) -> SourceId {
        let source_id = self.sources.register(text, display_name);
        debug_assert_eq!(self.sources.len(), self.acquisitions.len() + 1);
        self.acquisitions.push(acquisition);
        source_id
    }

    pub(crate) fn sources(&self) -> &SourceTexts {
        &self.sources
    }

    /// Returns a source snapshot that can be processed while this session is
    /// mutably borrowed by an acquisition callback. The source owner remains
    /// unchanged, so existing `SourceId` and `SourceSpan` values stay valid.
    pub(crate) fn snapshot_sources(&self) -> SourceTexts {
        self.sources.clone()
    }

    pub(crate) fn source_view(&self) -> SourceView<'_> {
        self.sources.view()
    }

    pub(crate) fn acquisition(
        &self,
        source_id: SourceId,
    ) -> Result<&SourceAcquisition, SourceSessionError> {
        let slot = source_id.slot();
        self.sources
            .view()
            .source(source_id)
            .map_err(SourceSessionError::Source)?;
        self.acquisitions
            .get(slot)
            .ok_or(SourceSessionError::MissingAcquisition { source_id })
    }

    pub(crate) fn acquisition_for_span(
        &self,
        span: SourceSpan,
    ) -> Result<&SourceAcquisition, SourceSessionError> {
        self.acquisition(span.source_id())
    }
}

impl SourceAcquisition {
    pub(crate) fn filesystem(canonical_path: impl Into<PathBuf>) -> Self {
        Self::Filesystem {
            canonical_path: Some(canonical_path.into()),
        }
    }

    pub(crate) fn filesystem_pending() -> Self {
        Self::Filesystem {
            canonical_path: None,
        }
    }

    pub(crate) fn canonical_path(&self) -> Option<&Path> {
        match self {
            Self::Filesystem { canonical_path } => canonical_path.as_deref(),
            Self::NoFilesystemLocation => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_keeps_display_name_separate_from_canonical_path() {
        let mut session = SourceProcessingSession::new();
        let source_id = session.register(
            "PRINT 1",
            "./src/main.tbx",
            SourceAcquisition::filesystem("/workspace/project/src/main.tbx"),
        );

        assert_eq!(
            session.source_view().display_name(source_id),
            Ok("./src/main.tbx")
        );
        assert_eq!(
            session.acquisition(source_id),
            Ok(&SourceAcquisition::filesystem(
                "/workspace/project/src/main.tbx"
            ))
        );
    }

    #[test]
    fn source_span_resolves_to_filesystem_or_no_filesystem_acquisition() {
        let mut session = SourceProcessingSession::new();
        let file = session.register(
            "PRINT 1",
            "program.tbx",
            SourceAcquisition::filesystem("/workspace/program.tbx"),
        );
        let stdin = session.register(
            "PRINT 2",
            "<stdin>",
            SourceAcquisition::NoFilesystemLocation,
        );

        let file_span = session.source_view().span(file, 0, 5).unwrap();
        let stdin_span = session.source_view().span(stdin, 0, 5).unwrap();
        assert!(session
            .acquisition_for_span(file_span)
            .unwrap()
            .canonical_path()
            .is_some());
        assert!(session
            .acquisition_for_span(stdin_span)
            .unwrap()
            .canonical_path()
            .is_none());
    }

    #[test]
    fn missing_or_foreign_acquisition_is_an_error_without_fallback() {
        let mut session = SourceProcessingSession::new();
        let source_id = session.register(
            "PRINT 1",
            "program.tbx",
            SourceAcquisition::NoFilesystemLocation,
        );
        let invalid = source_id.test_next_slot();
        assert_eq!(
            session.acquisition(invalid),
            Err(SourceSessionError::Source(SourceError::InvalidSourceId {
                id: invalid
            }))
        );

        let mut other = SourceProcessingSession::new();
        let foreign = other.register(
            "PRINT 2",
            "other.tbx",
            SourceAcquisition::NoFilesystemLocation,
        );
        assert_eq!(
            session.acquisition(foreign),
            Err(SourceSessionError::Source(SourceError::InvalidSourceId {
                id: foreign
            }))
        );
    }

    #[test]
    fn source_processing_can_register_while_an_existing_source_is_in_flight() {
        let mut session = SourceProcessingSession::new();
        let source_a = session.register(
            "PRINT A",
            "a.tbx",
            SourceAcquisition::filesystem("/workspace/a.tbx"),
        );
        let in_flight = session.snapshot_sources();
        let span_a = in_flight.view().span(source_a, 0, 5).unwrap();

        // This models the synchronous acquisition callback invoked while A is
        // being processed. It mutates the session, not A's borrowed snapshot.
        let source_b = session.register(
            "PRINT B",
            "b.tbx",
            SourceAcquisition::filesystem("/workspace/b.tbx"),
        );

        assert_eq!(in_flight.view().source(span_a.source_id()), Ok("PRINT A"));
        let after_callback = session.snapshot_sources();
        assert_eq!(after_callback.view().source(source_b), Ok("PRINT B"));
        assert_eq!(
            session
                .acquisition_for_span(after_callback.view().span(source_a, 0, 5).unwrap())
                .unwrap()
                .canonical_path(),
            Some(Path::new("/workspace/a.tbx"))
        );
        assert_eq!(
            session.acquisition(source_b).unwrap().canonical_path(),
            Some(Path::new("/workspace/b.tbx"))
        );
    }
}
