use std::io::Write;

use crate::binding::Bindings;
use crate::bootstrap::{
    register_builtin_global_variables, register_builtin_source_words, BuiltinGlobalBootstrapError,
    PrimitiveBootstrapError, SourceWordBootstrapError,
};
use crate::diagnostic::{DiagnosticRenderer, RenderedDiagnostic, UserDiagnostic};
use crate::global_variable::GlobalVariables;
use crate::operator::{register_named_operator_primitives, OperatorBootstrapError, OperatorWords};
use crate::output_primitive::register_output_primitives;
use crate::primitive::PrimitiveRegistry;
use crate::published_code::PublishedCode;
use crate::runtime_output::WriteRuntimeOutput;
use crate::source::{SourceId, SourceTexts};
use crate::source_processor::{
    compile_source, run_unit, SourceCompileContext, SourceExecutionContext, SourceRunResult,
};
use crate::source_word::SourceWordRegistry;
use crate::stack_primitive::register_stack_primitives;
use crate::user_facing::{UserFacingFailure, UserFacingFailureClass, UserFacingRunResult};
use crate::word::PublishedWords;
use crate::word_lookup::PublishedWordLookup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BatchExecutionResult {
    Success(SourceRunResult),
    Failure(Box<BatchExecutionFailure>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BatchExecutionFailure {
    cause: BatchExecutionFailureCause,
    diagnostic: RenderedDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BatchExecutionFailureCause {
    Setup(BatchSetupError),
    Source(Box<UserFacingFailure>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchSetupError {
    Operators(OperatorBootstrapError),
    Stack(PrimitiveBootstrapError),
    Output(PrimitiveBootstrapError),
    SourceWords(SourceWordBootstrapError),
    Globals(BuiltinGlobalBootstrapError),
}

struct BatchEnvironment {
    bindings: Bindings,
    primitives: PrimitiveRegistry,
    words: PublishedWords,
    operators: OperatorWords,
    source_words: SourceWordRegistry,
    globals: GlobalVariables,
    published_code: PublishedCode,
}

impl BatchExecutionFailure {
    pub(crate) fn class(&self) -> UserFacingFailureClass {
        match &self.cause {
            BatchExecutionFailureCause::Setup(_) => UserFacingFailureClass::Environment,
            BatchExecutionFailureCause::Source(failure) => failure.class(),
        }
    }

    pub(crate) const fn diagnostic(&self) -> &RenderedDiagnostic {
        &self.diagnostic
    }
}

impl BatchEnvironment {
    fn new() -> Result<Self, BatchSetupError> {
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let operators =
            register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                .map_err(BatchSetupError::Operators)?;
        register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Stack)?;
        register_output_primitives(&mut primitives, &mut words, &mut bindings)
            .map_err(BatchSetupError::Output)?;

        let mut source_words = SourceWordRegistry::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .map_err(BatchSetupError::SourceWords)?;

        let mut globals = GlobalVariables::new();
        register_builtin_global_variables(&mut globals, &mut bindings)
            .map_err(BatchSetupError::Globals)?;

        Ok(Self {
            bindings,
            primitives,
            words,
            operators,
            source_words,
            globals,
            published_code: PublishedCode::new(),
        })
    }
}

pub(crate) fn execute_registered_source<W>(
    sources: &SourceTexts,
    source_id: SourceId,
    writer: &mut W,
) -> BatchExecutionResult
where
    W: Write + ?Sized,
{
    let mut environment = match BatchEnvironment::new() {
        Ok(environment) => environment,
        Err(error) => return setup_failure(sources, error),
    };

    let compile_result = compile_source(
        sources.view(),
        source_id,
        SourceCompileContext::with_source_word_and_runtime_publication_and_operators(
            &mut environment.bindings,
            &mut environment.source_words,
            environment.operators.lookup(),
            &mut environment.globals,
            &mut environment.published_code,
            &mut environment.words,
        ),
    );

    let source_result = match compile_result {
        Ok(unit) => {
            let code_spaces = [environment.published_code.instruction_view()];
            let source_mappings = [environment.published_code.source_mapping()];
            let mut output = WriteRuntimeOutput::new(writer);
            let context = SourceExecutionContext::with_runtime_environment(
                &environment.bindings,
                environment.source_words.lookup(),
                environment.operators.lookup(),
                &code_spaces,
                &source_mappings,
                PublishedWordLookup::new(&environment.words),
                environment.primitives.lookup(),
            )
            .with_mut_globals(environment.globals.view_mut())
            .with_output(&mut output);
            run_unit(&unit, context)
        }
        Err(error) => Err(error),
    };

    user_facing_result(sources, source_result)
}

fn user_facing_result(
    sources: &SourceTexts,
    result: Result<SourceRunResult, crate::source_processor::SourceProcessorError>,
) -> BatchExecutionResult {
    match UserFacingRunResult::from_source_result(sources.view(), result) {
        UserFacingRunResult::Success(result) => BatchExecutionResult::Success(result),
        UserFacingRunResult::Failure(failure) => {
            // #1592 validates a primary span's source ownership and display
            // information before this boundary. An unresolved span is replaced
            // with a source-less diagnostic, so #1591 rendering cannot fail here.
            let diagnostic = DiagnosticRenderer::new(sources.view())
                .render(failure.diagnostic())
                .expect("user-facing classification must produce a renderable diagnostic");
            BatchExecutionResult::Failure(Box::new(BatchExecutionFailure {
                cause: BatchExecutionFailureCause::Source(failure),
                diagnostic,
            }))
        }
    }
}

fn setup_failure(sources: &SourceTexts, error: BatchSetupError) -> BatchExecutionResult {
    let diagnostic =
        UserDiagnostic::without_source("execution environment", "runtime environment setup failed");
    let diagnostic = DiagnosticRenderer::new(sources.view())
        .render(&diagnostic)
        .expect("source-less setup diagnostic must render");
    BatchExecutionResult::Failure(Box::new(BatchExecutionFailure {
        cause: BatchExecutionFailureCause::Setup(error),
        diagnostic,
    }))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::*;
    use crate::source::SourceTexts;
    use crate::user_facing::UserFacingFailureClass;
    use crate::value::Value;

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

    #[test]
    fn registered_file_and_stdin_sources_share_the_same_execution_path() {
        for display_name in ["program.tbx", "<stdin>"] {
            let (sources, source_id) = source("EVAL 2 + 3\nEVAL ADD(4, 5)", display_name);
            let mut writer = RecordingWriter::default();

            let result = success(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(result.data_stack(), [Value::integer(5), Value::integer(9)]);
            assert_eq!(writer.text(), "");
        }
    }

    #[test]
    fn m20_environment_supports_variables_definitions_stack_words_and_output() {
        let text = "LET A = 4\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(A)\nPRINT\nCR";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(4)]);
        assert_eq!(writer.text(), "4\n");
    }

    #[test]
    fn batch_top_level_can_publish_and_use_a_source_word_before_a_definition() {
        let text = "SYNTAX SLET\nSTATEMENT\nREAD_NAME AS name\nRESOLVE_VAR name AS target\nEXPECT \"=\"\nREAD_EXPR AS expr\nEMIT_EXPR expr\nEMIT_STORE target\nENDS\nLET A = 0\nSLET A = 7\nDEF DOUBLE\nDUP\nEND\nEVAL DOUBLE(A)";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(7), Value::integer(7)]);
    }

    #[test]
    fn batch_top_level_can_publish_and_use_a_block_source_word() {
        let text = "SYNTAX WRAP\nBLOCK\nSTART\nEXPECT_END\nLAST ENDWRAP\nEXPECT_END\nENDS\nWRAP\nENDWRAP\nEVAL 9";
        let (sources, source_id) = source(text, "program.tbx");
        let mut writer = RecordingWriter::default();

        let result = success(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(result.data_stack(), [Value::integer(9)]);
    }

    #[test]
    fn located_compile_failure_renders_registered_display_name_and_position() {
        for display_name in ["relative/program.tbx", "<stdin>"] {
            let (sources, source_id) = source("UNKNOWN", display_name);
            let mut writer = RecordingWriter::default();

            let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

            assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
            let primary = failure
                .diagnostic()
                .primary()
                .expect("compile failure should have a source position");
            assert_eq!(primary.display_name(), display_name);
            assert_eq!(primary.line_number(), 1);
            assert_eq!(primary.column_number(), 1);
        }
    }

    #[test]
    fn foreign_source_id_is_source_less_environment_failure() {
        let (sources, _) = source("EVAL 1", "program.tbx");
        let (foreign_sources, foreign_source_id) = source("EVAL 2", "other.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(
            &sources,
            foreign_source_id,
            &mut writer,
        ));
        drop(foreign_sources);

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure.diagnostic().primary().is_none());
        assert_eq!(failure.diagnostic().target(), Some("source program"));
    }

    #[test]
    fn runtime_failure_is_a_located_user_program_failure() {
        let (sources, source_id) = source("DUP", "program.tbx");
        let mut writer = RecordingWriter::default();

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::UserProgram);
        assert_eq!(
            failure
                .diagnostic()
                .primary()
                .map(|primary| primary.source_line()),
            Some("DUP")
        );
    }

    #[test]
    fn runtime_output_failure_is_a_located_environment_failure() {
        let (sources, source_id) = source("EVAL 7\nPRINT", "program.tbx");
        let mut writer = RecordingWriter::failing_after(0);

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert!(failure.diagnostic().primary().is_some());
        assert_eq!(writer.text(), "");
    }

    #[test]
    fn successful_runtime_output_is_not_rolled_back_by_a_later_failure() {
        let (sources, source_id) = source("EVAL 7\nPRINT\nCR", "program.tbx");
        let mut writer = RecordingWriter::failing_after(1);

        let failure = failure(execute_registered_source(&sources, source_id, &mut writer));

        assert_eq!(failure.class(), UserFacingFailureClass::Environment);
        assert_eq!(writer.text(), "7");
    }
}
