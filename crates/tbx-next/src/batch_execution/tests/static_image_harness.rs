use super::*;
use crate::arithmetic_primitive::register_arithmetic_primitives;
use crate::bootstrap::register_builtin_source_words;
use crate::global_array::GlobalArrays;
use crate::global_variable::GlobalVariables;
use crate::input_primitive::register_input_primitives;
use crate::operator::register_named_operator_primitives;
use crate::output_primitive::register_output_primitives;
use crate::random::RandomState;
use crate::random_primitive::register_random_primitives;
use crate::runtime_input::TestInput;
use crate::source_processor::{run_unit, SourceCompileContext, SourceExecutionContext};
use crate::stack_primitive::register_stack_primitives;
use crate::static_image::{
    test_lower_and_encode, test_lower_and_encode_with_statistics, test_lower_and_run,
    LogicalInstructionKind, TestImageStatistics,
};
use crate::word::PublishedWords;
use crate::word_lookup::PublishedWordLookup;
use std::io::Write;

mod sim65_e2e;

struct Evaluation {
    statistics: TestImageStatistics,
    host_output: Option<Vec<u8>>,
    artifact: Option<crate::static_image::bytecode_6502::BytecodeArtifact>,
}

struct Fixture {
    bindings: Bindings,
    primitives: PrimitiveRegistry,
    words: PublishedWords,
    operators: crate::operator::OperatorWords,
    source_words: SourceWordRegistry,
    globals: GlobalVariables,
    arrays: GlobalArrays,
    published_code: PublishedCode,
    random: RandomState,
    primitive_words: (
        crate::operator::OperatorWords,
        crate::word::WordId,
        [crate::word::WordId; 3],
        [crate::word::WordId; 3],
        crate::word::WordId,
        crate::word::WordId,
    ),
}

impl Fixture {
    fn new() -> Self {
        Self::with_seed(0x5442_582D_4E45_5854)
    }

    fn with_seed(seed: u64) -> Self {
        let mut bindings = Bindings::new();
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let operators =
            register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("operator bootstrap succeeds");
        let abs = register_arithmetic_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("arithmetic bootstrap succeeds")
            .abs();
        let stack = register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("stack bootstrap succeeds");
        let output = register_output_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("output bootstrap succeeds");
        let input = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("input bootstrap succeeds");
        let rnd = register_random_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("random bootstrap succeeds");
        let mut source_words = SourceWordRegistry::new();
        register_builtin_source_words(&mut source_words, &mut bindings)
            .expect("source word bootstrap succeeds");
        let globals = GlobalVariables::new();
        Self {
            bindings,
            primitives,
            words,
            operators,
            source_words,
            globals,
            arrays: GlobalArrays::new(),
            published_code: PublishedCode::new(),
            random: RandomState::seeded(seed),
            primitive_words: (
                operators,
                abs,
                [stack.dup(), stack.drop(), stack.swap()],
                [output.putdec(), output.putchr(), output.cr()],
                input.try_input(),
                rnd.rnd(),
            ),
        }
    }

    fn compile(
        &mut self,
        sources: &SourceTexts,
        source_id: SourceId,
    ) -> crate::source_processor::TemporaryExecutionUnit {
        compile_source(
            sources.view(),
            source_id,
            SourceCompileContext::with_source_word_and_runtime_publication_and_operators(
                &mut self.bindings,
                &mut self.source_words,
                self.operators.lookup(),
                &mut self.globals,
                &mut self.published_code,
                &mut self.words,
            )
            .with_global_arrays(&mut self.arrays),
        )
        .unwrap_or_else(|error| panic!("compile source: {error:?}"))
    }
}

#[derive(Default)]
struct Output(Vec<u8>);

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn evaluate(
    source: &str,
    display_name: &str,
    compare_execution: bool,
    encode_artifact: bool,
) -> Evaluation {
    evaluate_with_seed(
        source,
        display_name,
        compare_execution,
        encode_artifact,
        0x5442_582D_4E45_5854,
    )
}

fn evaluate_with_seed(
    source: &str,
    display_name: &str,
    compare_execution: bool,
    encode_artifact: bool,
    seed: u64,
) -> Evaluation {
    evaluate_with_seed_and_input(
        source,
        display_name,
        compare_execution,
        encode_artifact,
        seed,
        None,
    )
}

fn evaluate_with_seed_and_input(
    source: &str,
    display_name: &str,
    compare_execution: bool,
    encode_artifact: bool,
    seed: u64,
    input_lines: Option<&[&str]>,
) -> Evaluation {
    let mut sources = SourceTexts::new();
    let stdlib_id = register_embedded_standard_library(&mut sources);
    let program_id = sources.register(source, display_name);
    let mut fixture = Fixture::with_seed(seed);
    let _stdlib = fixture.compile(&sources, stdlib_id);
    let unit = fixture.compile(&sources, program_id);

    let host = if compare_execution {
        let code_spaces = [fixture.published_code.instruction_view()];
        let source_mappings = [fixture.published_code.source_mapping()];
        let mut output = Output::default();
        let result = {
            let mut runtime_output = WriteRuntimeOutput::new(&mut output);
            let mut input = input_lines
                .map(|lines| TestInput::new(lines.iter().map(|line| Ok(Some((*line).to_owned())))));
            let mut context = SourceExecutionContext::with_runtime_environment(
                &fixture.bindings,
                fixture.source_words.lookup(),
                fixture.operators.lookup(),
                &code_spaces,
                &source_mappings,
                PublishedWordLookup::new(&fixture.words),
                fixture.primitives.lookup(),
            )
            .with_mut_globals(fixture.globals.view_mut())
            .with_mut_arrays(fixture.arrays.view_mut())
            .with_random(&mut fixture.random)
            .with_output(&mut runtime_output);
            if let Some(ref mut input) = input {
                context = context.with_input(input);
            }
            run_unit(&unit, context).expect("host executes source")
        };
        Some((result, output.0))
    } else {
        None
    };

    let temporary = unit.instructions();
    let published = fixture.published_code.instruction_view();
    let entry = unit.entry_location();
    assert_eq!(
        unit.entry(),
        crate::instruction::InstructionAddress::from_index(0)
    );
    let encoded_artifact = if encode_artifact {
        let artifact = test_lower_and_encode(
            &[temporary, published],
            entry,
            &fixture.words,
            fixture.primitive_words,
            &fixture.globals,
            &fixture.arrays,
        )
        .unwrap_or_else(|error| panic!("encode source as M34 bytecode: {error:?}"));
        assert!(!artifact.code().is_empty());
        assert_eq!(artifact.entry_offset(), 0);
        Some(artifact)
    } else {
        None
    };
    let mut poc_output = Output::default();
    let mut poc_runtime_output = WriteRuntimeOutput::new(&mut poc_output);
    let mut poc_random = RandomState::seeded(seed);
    let mut reference_input = input_lines
        .map(|lines| TestInput::new(lines.iter().map(|line| Ok(Some((*line).to_owned())))));
    let poc = test_lower_and_run(
        &[temporary, published],
        entry,
        &fixture.words,
        fixture.primitive_words,
        &fixture.globals,
        &fixture.arrays,
        (
            &mut poc_runtime_output,
            &mut poc_random,
            reference_input
                .as_mut()
                .map(|input| input as &mut dyn crate::runtime_input::RuntimeInput),
        ),
    )
    .expect("source lowers and executes in the reference VM");
    if let Some(ref artifact) = encoded_artifact {
        assert_eq!(
            artifact.global_slot_count() as usize,
            poc.statistics.global_count
        );
    }
    if let Some((ref host, ref host_output)) = host {
        assert!(poc.halted);
        assert_eq!(host.outcome(), crate::vm::RunOutcome::Halted);
        assert_eq!(host_output, &poc_output.0);
        assert_eq!(
            host.data_stack()
                .iter()
                .map(|v| v.as_integer())
                .collect::<Vec<_>>(),
            poc.data_stack
        );
    }
    Evaluation {
        statistics: poc.statistics,
        host_output: host.map(|(_, output)| output),
        artifact: encoded_artifact,
    }
}

fn evaluate_strict_input_failure(
    source: &str,
) -> (
    Vec<u8>,
    Vec<u8>,
    crate::static_image::bytecode_6502::BytecodeArtifact,
) {
    let mut sources = SourceTexts::new();
    let stdlib_id = register_embedded_standard_library(&mut sources);
    let program_id = sources.register(source, "strict-input.tbx");
    let mut fixture = Fixture::new();
    let _stdlib = fixture.compile(&sources, stdlib_id);
    let unit = fixture.compile(&sources, program_id);
    let code_spaces = [fixture.published_code.instruction_view()];
    let source_mappings = [fixture.published_code.source_mapping()];

    let mut host_output = Output::default();
    let mut host_runtime_output = WriteRuntimeOutput::new(&mut host_output);
    let mut host_input = TestInput::strict(std::iter::empty());
    let context = SourceExecutionContext::with_runtime_environment(
        &fixture.bindings,
        fixture.source_words.lookup(),
        fixture.operators.lookup(),
        &code_spaces,
        &source_mappings,
        PublishedWordLookup::new(&fixture.words),
        fixture.primitives.lookup(),
    )
    .with_mut_globals(fixture.globals.view_mut())
    .with_mut_arrays(fixture.arrays.view_mut())
    .with_random(&mut fixture.random)
    .with_output(&mut host_runtime_output)
    .with_input(&mut host_input);
    assert!(run_unit(&unit, context).is_err());

    let temporary = unit.instructions();
    let published = fixture.published_code.instruction_view();
    let entry = unit.entry_location();
    let artifact = test_lower_and_encode(
        &[temporary, published],
        entry,
        &fixture.words,
        fixture.primitive_words,
        &fixture.globals,
        &fixture.arrays,
    )
    .expect("strict input source encodes");

    let mut reference_output = Output::default();
    let mut reference_runtime_output = WriteRuntimeOutput::new(&mut reference_output);
    let mut reference_random = RandomState::seeded(0x5442_582D_4E45_5854);
    let mut reference_input = TestInput::strict(std::iter::empty());
    let result = test_lower_and_run(
        &[temporary, published],
        entry,
        &fixture.words,
        fixture.primitive_words,
        &fixture.globals,
        &fixture.arrays,
        (
            &mut reference_runtime_output,
            &mut reference_random,
            Some(&mut reference_input),
        ),
    );
    assert!(
        result.is_err(),
        "ReferenceVm must preserve strict input failure"
    );
    (host_output.0, reference_output.0, artifact)
}

#[test]
fn prime_source_matches_host_execution_and_reports_static_image() {
    let statistics = evaluate(
        include_str!("../../../../../docs/next/examples/prime.tbx"),
        "prime.tbx",
        true,
        false,
    )
    .statistics;
    assert!(statistics.instruction_count > 0);
    assert_eq!(
        statistics.instruction_count,
        statistics
            .variant_counts
            .iter()
            .map(|(_, count)| count)
            .sum()
    );
}

#[test]
fn representative_sources_compile_and_lower() {
    let mandelbrot = evaluate(
        include_str!("../../../../../docs/next/examples/mandelbrot.tbx"),
        "mandelbrot.tbx",
        false,
        false,
    )
    .statistics;
    assert!(mandelbrot.instruction_count > 0);
    let grades = evaluate(
        include_str!("../../../../../docs/next/examples/grades.tbx"),
        "grades.tbx",
        false,
        false,
    )
    .statistics;
    assert!(grades.instruction_count > 0);
    assert!(!grades.array_lengths.is_empty());
}

#[test]
fn grades_source_encodes_control_value_instructions_and_metadata() {
    let grades = evaluate(
        include_str!("../../../../../docs/next/examples/grades.tbx"),
        "grades.tbx",
        false,
        true,
    );
    let artifact = grades.artifact.expect("grades artifact encodes");

    for expected in [
        crate::static_image::LogicalInstructionKind::ControlPush,
        crate::static_image::LogicalInstructionKind::ControlCopy,
        crate::static_image::LogicalInstructionKind::ControlDrop,
    ] {
        assert!(
            grades
                .statistics
                .variant_counts
                .iter()
                .any(|(kind, count)| *kind == expected && *count > 0),
            "grades StaticImage contains {expected:?}"
        );
    }

    assert_eq!(artifact.array_lengths(), &[6]);
    assert_eq!(artifact.text_count(), 7);
    assert_eq!(artifact.text_storage_bytes(), Some(38));
    assert_eq!(artifact.text_descriptor_bytes(), Some(28));
}

#[test]
fn eightqueen_source_encodes_artifact_and_array_metadata() {
    let eightqueen = evaluate(
        include_str!("../../../../../docs/next/examples/eightqueen.tbx"),
        "eightqueen.tbx",
        false,
        true,
    );
    let artifact = eightqueen.artifact.expect("eightqueen artifact encodes");

    assert_eq!(artifact.array_lengths(), &[8, 8]);
    assert_eq!(artifact.text_count(), 0);
    assert_eq!(artifact.text_storage_bytes(), Some(0));
    assert_eq!(artifact.text_descriptor_bytes(), Some(0));
}

#[test]
fn maze_source_encodes_artifact_and_resource_metadata() {
    let maze = evaluate(
        include_str!("../../../../../docs/next/examples/maze.tbx"),
        "maze.tbx",
        false,
        true,
    );
    let artifact = maze.artifact.expect("maze artifact encodes");

    assert_eq!(artifact.array_lengths(), &[40, 40, 40, 40, 40, 40]);
    assert_eq!(artifact.array_storage_bytes(), Some(480));
    assert_eq!(artifact.array_descriptor_bytes(), Some(24));
    assert_eq!(artifact.text_count(), 2);
    assert_eq!(artifact.text_storage_bytes(), Some(18));
    assert_eq!(artifact.text_descriptor_bytes(), Some(8));
}

#[test]
fn rnd_source_lowers_and_encodes_into_the_6502_artifact() {
    let mut sources = SourceTexts::new();
    let stdlib_id = register_embedded_standard_library(&mut sources);
    let program_id = sources.register("PUTDEC RND(10)", "rnd.tbx");
    let mut fixture = Fixture::new();
    let _stdlib = fixture.compile(&sources, stdlib_id);
    let unit = fixture.compile(&sources, program_id);
    let temporary = unit.instructions();
    let published = fixture.published_code.instruction_view();

    let artifact = test_lower_and_encode(
        &[temporary, published],
        unit.entry_location(),
        &fixture.words,
        fixture.primitive_words,
        &fixture.globals,
        &fixture.arrays,
    )
    .expect("RND source lowers and encodes");

    assert!(
        artifact
            .code()
            .windows(2)
            .any(|bytes| bytes == [0x51, 0x60]),
        "RND must lower to private opcode 0x51 before PUTDEC: {:?}",
        artifact.code()
    );
}

#[test]
fn try_input_source_lowers_and_encodes_without_running_the_reference_vm() {
    let mut sources = SourceTexts::new();
    let stdlib_id = register_embedded_standard_library(&mut sources);
    let program_id = sources.register(
        "VAR VALUE\nIF_LET VALUE = TRY_INPUT()\nENDLET",
        "try-input.tbx",
    );
    let mut fixture = Fixture::new();
    let _stdlib = fixture.compile(&sources, stdlib_id);
    let unit = fixture.compile(&sources, program_id);
    let temporary = unit.instructions();
    let published = fixture.published_code.instruction_view();

    let (statistics, _artifact) = test_lower_and_encode_with_statistics(
        &[temporary, published],
        unit.entry_location(),
        &fixture.words,
        fixture.primitive_words,
        &fixture.globals,
        &fixture.arrays,
    )
    .expect("TRY_INPUT source lowers and encodes");

    assert!(
        statistics
            .variant_counts
            .iter()
            .any(|(kind, count)| *kind == LogicalInstructionKind::CallTryInput && *count > 0),
        "TRY_INPUT source must lower to a TRY_INPUT primitive call: {:?}",
        statistics.variant_counts
    );
}

#[test]
fn array_resource_measurements_are_available_on_request() {
    let minimal = evaluate(
        "DIM @VALUES[3]\nLET @VALUES[1] = 7\nPRINT @VALUES[1]\nCR\n",
        "array-minimal.tbx",
        false,
        false,
    )
    .statistics;
    let squares = evaluate(
        include_str!("../../../../../docs/next/examples/squares.tbx"),
        "squares.tbx",
        false,
        false,
    )
    .statistics;
    let grades = evaluate(
        include_str!("../../../../../docs/next/examples/grades.tbx"),
        "grades.tbx",
        false,
        false,
    )
    .statistics;

    assert!(!minimal.array_lengths.is_empty());
    let variants = format!("{:?}", minimal.variant_counts);
    assert!(variants.contains("LoadArray") && variants.contains("StoreArray"));
    for (name, statistics) in [
        ("array-minimal", minimal),
        ("squares", squares),
        ("grades", grades),
    ] {
        assert!(statistics.instruction_count > 0, "{name}");
        if std::env::var_os("TBX_MEASURE_STATIC_IMAGES").is_some() {
            let array_cells: usize = statistics.array_lengths.iter().sum();
            eprintln!(
                "static image: sample={name} instructions={} variants={:?} relocations={} fixed_text_count={} fixed_text_bytes={} globals={} array_lengths={:?} array_cells={array_cells}",
                statistics.instruction_count,
                statistics.variant_counts,
                statistics.relocation_count,
                statistics.fixed_text_count,
                statistics.fixed_text_bytes,
                statistics.global_count,
                statistics.array_lengths,
            );
        }
    }
}
