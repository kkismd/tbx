use super::*;
use crate::arithmetic_primitive::register_arithmetic_primitives;
use crate::bootstrap::{register_builtin_global_variables, register_builtin_source_words};
use crate::global_array::GlobalArrays;
use crate::global_variable::GlobalVariables;
use crate::input_primitive::register_input_primitives;
use crate::operator::register_named_operator_primitives;
use crate::output_primitive::register_output_primitives;
use crate::random::RandomState;
use crate::random_primitive::register_random_primitives;
use crate::source_processor::{run_unit, SourceCompileContext, SourceExecutionContext};
use crate::stack_primitive::register_stack_primitives;
use crate::static_image::{test_lower_and_run, TestImageStatistics};
use crate::word::PublishedWords;
use crate::word_lookup::PublishedWordLookup;
use std::io::Write;

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
        let mut globals = GlobalVariables::new();
        register_builtin_global_variables(&mut globals, &mut bindings)
            .expect("global bootstrap succeeds");
        Self {
            bindings,
            primitives,
            words,
            operators,
            source_words,
            globals,
            arrays: GlobalArrays::new(),
            published_code: PublishedCode::new(),
            random: RandomState::seeded(0x5442_582D_4E45_5854),
            primitive_words: (
                operators,
                abs,
                [stack.dup(), stack.drop(), stack.swap()],
                [output.putdec(), output.putchr(), output.cr()],
                input.input_question(),
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
        .expect("source compiles")
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

fn evaluate(source: &str, display_name: &str, compare_execution: bool) -> TestImageStatistics {
    let mut sources = SourceTexts::new();
    let stdlib_id = register_embedded_standard_library(&mut sources);
    let program_id = sources.register(source, display_name);
    let mut fixture = Fixture::new();
    let _stdlib = fixture.compile(&sources, stdlib_id);
    let unit = fixture.compile(&sources, program_id);

    let host = if compare_execution {
        let code_spaces = [fixture.published_code.instruction_view()];
        let source_mappings = [fixture.published_code.source_mapping()];
        let mut output = Output::default();
        let result = {
            let mut runtime_output = WriteRuntimeOutput::new(&mut output);
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
            .with_output(&mut runtime_output);
            run_unit(&unit, context).expect("host executes source")
        };
        Some((result, output.0))
    } else {
        None
    };

    let temporary = unit.instructions();
    let published = fixture.published_code.instruction_view();
    // The temporary owner is first, so its entry is exactly CodePosition(0).
    assert_eq!(
        unit.entry(),
        crate::instruction::InstructionAddress::from_index(0)
    );
    let mut poc_output = Output::default();
    let poc = test_lower_and_run(
        &[temporary, published],
        &fixture.words,
        fixture.primitive_words,
        &fixture.globals,
        &fixture.arrays,
        &mut poc_output,
    )
    .expect("source lowers and executes in the reference VM");
    if let Some((host, host_output)) = host {
        assert!(poc.halted);
        assert_eq!(host.outcome(), crate::vm::RunOutcome::Halted);
        assert_eq!(host_output, poc_output.0);
        assert_eq!(
            host.data_stack()
                .iter()
                .map(|v| v.as_integer())
                .collect::<Vec<_>>(),
            poc.data_stack
        );
    }
    poc.statistics
}

#[test]
fn prime_source_matches_host_execution_and_reports_static_image() {
    let statistics = evaluate(
        include_str!("../../../../../docs/next/examples/prime.tbx"),
        "prime.tbx",
        true,
    );
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
    );
    assert!(mandelbrot.instruction_count > 0);
    let grades = evaluate(
        include_str!("../../../../../docs/next/examples/grades.tbx"),
        "grades.tbx",
        false,
    );
    assert!(grades.instruction_count > 0);
    assert!(!grades.array_lengths.is_empty());
}
