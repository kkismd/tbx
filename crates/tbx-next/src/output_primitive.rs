use crate::binding::{BindingInsertError, Bindings};
use crate::bootstrap::{register_primitive, PrimitiveBootstrapError};
use crate::name::NormalizedName;
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::value::Value;
use crate::word::{PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputPrimitiveWords {
    print: WordId,
    cr: WordId,
}

pub(crate) fn register_output_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<OutputPrimitiveWords, PrimitiveBootstrapError> {
    let print_name = builtin_name("PRINT");
    let cr_name = builtin_name("CR");

    for name in [&print_name, &cr_name] {
        bindings
            .validate_new_name(name)
            .map_err(primitive_bootstrap_precheck_error)?;
    }

    let print_primitive = primitives.register(print);
    let cr_primitive = primitives.register(cr);
    let print = register_primitive(words, bindings, print_name, print_primitive)?;
    let cr = register_primitive(words, bindings, cr_name, cr_primitive)?;

    Ok(OutputPrimitiveWords { print, cr })
}

impl OutputPrimitiveWords {
    pub(crate) const fn print(self) -> WordId {
        self.print
    }

    pub(crate) const fn cr(self) -> WordId {
        self.cr
    }
}

fn builtin_name(input: &str) -> NormalizedName {
    NormalizedName::new(input).expect("built-in output primitive name should be valid")
}

fn primitive_bootstrap_precheck_error(error: BindingInsertError) -> PrimitiveBootstrapError {
    match error {
        BindingInsertError::NameConflict => PrimitiveBootstrapError::NameConflict,
        BindingInsertError::ReservedName => PrimitiveBootstrapError::ReservedName,
    }
}

fn print(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    if context.data_stack_is_empty() {
        return Ok(());
    }

    let value = context.peek()?;
    let text = format_print_value(value);
    context.write_output(&text)?;
    context
        .pop()
        .expect("PRINT value was checked before consuming it");
    Ok(())
}

fn cr(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    context.write_output("\n")
}

fn format_print_value(value: Value) -> String {
    value.as_integer().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::primitive::PrimitiveRegistry;
    use crate::runtime_output::{RuntimeOutput, RuntimeOutputError, TestOutput};
    use crate::value::Value;
    use crate::vm::{ExecutionView, RunOutcome, StepOutcome, Vm, VmError, VmErrorKind};
    use crate::word::{PublishedWords, WordDefinition};
    use crate::word_lookup::PublishedWordLookup;
    use crate::word_resolution::resolve_word_name;

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid name")
    }

    fn execution<'a>(
        code: &'a InstructionSequence,
        words: &'a PublishedWords,
        primitives: &'a PrimitiveRegistry,
    ) -> ExecutionView<'a> {
        ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(words),
            primitives.lookup(),
        )
    }

    fn bootstrapped_output_words() -> (
        PrimitiveRegistry,
        PublishedWords,
        Bindings,
        OutputPrimitiveWords,
    ) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let output_words = register_output_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("output primitives should bootstrap");

        (primitives, words, bindings, output_words)
    }

    fn run_calls(
        setup: impl FnOnce(&mut InstructionSequence, OutputPrimitiveWords),
    ) -> (Vm, TestOutput, Result<RunOutcome, VmError>) {
        let (primitives, words, _, output_words) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        setup(&mut code, output_words);
        let entry = crate::instruction::InstructionAddress::from_index(0);
        code.append(Instruction::Halt);
        let mut output = TestOutput::new();
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");
        let result = vm.run(execution(&code, &words, &primitives).with_output(&mut output));

        (vm, output, result)
    }

    #[test]
    fn print_succeeds_without_output_on_empty_stack() {
        let (vm, output, result) = run_calls(|code, words| {
            code.append(Instruction::Call(words.print()));
        });

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert!(output.chunks().is_empty());
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn print_formats_signed_decimal_integer_representatives() {
        for (input, expected) in [
            (0, "0"),
            (1, "1"),
            (42, "42"),
            (-1, "-1"),
            (-32768, "-32768"),
            (32767, "32767"),
        ] {
            let (vm, output, result) = run_calls(|code, words| {
                code.append(Instruction::Push(value(input)));
                code.append(Instruction::Call(words.print()));
            });

            assert_eq!(result, Ok(RunOutcome::Halted), "input {input}");
            assert_eq!(output.chunks(), [expected], "input {input}");
            assert_eq!(vm.data_stack_depth(), 0, "input {input}");
        }
    }

    #[test]
    fn print_consumes_only_top_value_and_does_not_emit_newline() {
        let (mut vm, output, result) = run_calls(|code, words| {
            code.append(Instruction::Push(value(3)));
            code.append(Instruction::Push(value(5)));
            code.append(Instruction::Call(words.print()));
        });

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(output.chunks(), ["5"]);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.pop_data(), Ok(value(3)));
    }

    #[test]
    fn cr_writes_only_newline_without_changing_data_stack() {
        let (mut vm, output, result) = run_calls(|code, words| {
            code.append(Instruction::Push(value(12)));
            code.append(Instruction::Call(words.cr()));
        });

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(output.chunks(), ["\n"]);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.pop_data(), Ok(value(12)));
    }

    #[test]
    fn print_and_cr_keep_output_order_separate() {
        let (vm, output, result) = run_calls(|code, words| {
            code.append(Instruction::Push(value(42)));
            code.append(Instruction::Call(words.print()));
            code.append(Instruction::Call(words.cr()));
            code.append(Instruction::Push(value(-7)));
            code.append(Instruction::Call(words.print()));
        });

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(output.chunks(), ["42", "\n", "-7"]);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn print_output_failure_leaves_target_value_on_stack() {
        let (primitives, words, _, output_words) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(99)));
        code.append(Instruction::Push(value(13)));
        let call = code.append(Instruction::Call(output_words.print()));
        code.append(Instruction::Halt);
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        let result = vm.step(execution(&code, &words, &primitives).with_output(&mut output));

        let error = result.expect_err("failed output should fail PRINT");
        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::OutputFailed {
                    source: RuntimeOutputError::Failed,
                },
                ..
            }
        ));
        assert!(output.chunks().is_empty());
        assert_eq!(vm.data_stack_depth(), 2);
        assert_eq!(vm.pop_data(), Ok(value(13)));
        assert_eq!(vm.pop_data(), Ok(value(99)));
        assert_eq!(vm.instruction_pointer(), code.view().location(call));
    }

    #[derive(Debug, Default)]
    struct PartialFailOutput {
        chunks: Vec<String>,
    }

    impl RuntimeOutput for PartialFailOutput {
        fn write(&mut self, text: &str) -> Result<(), RuntimeOutputError> {
            self.chunks.push(text.to_owned());
            Err(RuntimeOutputError::Failed)
        }
    }

    #[test]
    fn print_output_failure_does_not_require_external_effect_rollback() {
        let (primitives, words, _, output_words) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(21)));
        code.append(Instruction::Call(output_words.print()));
        code.append(Instruction::Halt);
        let mut output = PartialFailOutput::default();
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        let result = vm.step(execution(&code, &words, &primitives).with_output(&mut output));

        assert!(result.is_err());
        assert_eq!(output.chunks, ["21"]);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.pop_data(), Ok(value(21)));
    }

    #[test]
    fn output_primitive_bootstrap_publishes_print_and_cr_as_runtime_words() {
        let (primitives, words, bindings, output_words) = bootstrapped_output_words();

        assert_eq!(primitives.len(), 2);
        assert_eq!(words.len(), 2);
        assert_eq!(
            resolve_word_name(&bindings, "print"),
            Ok(output_words.print())
        );
        assert_eq!(resolve_word_name(&bindings, "cr"), Ok(output_words.cr()));
        assert_eq!(
            bindings.get(&name("PRINT")),
            Some(&Binding::Word(output_words.print()))
        );
        assert_eq!(
            bindings.get(&name("CR")),
            Some(&Binding::Word(output_words.cr()))
        );
        assert!(matches!(
            words.get(output_words.print()),
            Ok(WordDefinition::Primitive { .. })
        ));
        assert!(matches!(
            words.get(output_words.cr()),
            Ok(WordDefinition::Primitive { .. })
        ));
    }

    #[test]
    fn output_primitive_bootstrap_prechecks_all_name_conflicts() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let existing = WordId::test_invalid(0);
        bindings
            .insert_new(name("CR"), Binding::Word(existing))
            .expect("test setup should bind CR");

        let result = register_output_primitives(&mut primitives, &mut words, &mut bindings);

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(primitives.len(), 0);
        assert_eq!(words.len(), 0);
        assert_eq!(bindings.get(&name("CR")), Some(&Binding::Word(existing)));
        assert!(bindings.get(&name("PRINT")).is_none());
    }
}
