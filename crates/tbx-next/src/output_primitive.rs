use crate::binding::{BindingInsertError, Bindings};
use crate::bootstrap::{register_primitive, PrimitiveBootstrapError};
use crate::name::NormalizedName;
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::value::Value;
use crate::word::{PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputPrimitiveWords {
    putdec: WordId,
    putchr: WordId,
    cr: WordId,
}

pub(crate) fn register_output_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<OutputPrimitiveWords, PrimitiveBootstrapError> {
    let putdec_name = builtin_name("PUTDEC");
    let putchr_name = builtin_name("PUTCHR");
    let cr_name = builtin_name("CR");

    for name in [&putdec_name, &putchr_name, &cr_name] {
        bindings
            .validate_new_name(name)
            .map_err(primitive_bootstrap_precheck_error)?;
    }

    let putdec_primitive = primitives.register(putdec);
    let putchr_primitive = primitives.register(putchr);
    let cr_primitive = primitives.register(cr);
    let putdec = register_primitive(words, bindings, putdec_name, putdec_primitive)?;
    let putchr = register_primitive(words, bindings, putchr_name, putchr_primitive)?;
    let cr = register_primitive(words, bindings, cr_name, cr_primitive)?;

    Ok(OutputPrimitiveWords { putdec, putchr, cr })
}

impl OutputPrimitiveWords {
    pub(crate) const fn putdec(self) -> WordId {
        self.putdec
    }

    pub(crate) const fn putchr(self) -> WordId {
        self.putchr
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

fn putdec(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    if context.data_stack_is_empty() {
        return Ok(());
    }

    let value = context.peek()?;
    let text = format_putdec_value(value);
    context.write_output(&text)?;
    context
        .pop()
        .expect("PUTDEC value was checked before consuming it");
    Ok(())
}

fn putchr(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    if context.data_stack_is_empty() {
        return Ok(());
    }

    let value = context.peek()?.as_integer();
    let byte = u8::try_from(value).map_err(|_| PrimitiveError::AsciiOutOfRange { value })?;
    if byte > 0x7f {
        return Err(PrimitiveError::AsciiOutOfRange { value });
    }

    let text = char::from(byte).to_string();
    context.write_output(&text)?;
    context
        .pop()
        .expect("PUTCHR value was checked before consuming it");
    Ok(())
}

fn cr(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    context.write_output("\n")
}

fn format_putdec_value(value: Value) -> String {
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
    use std::rc::Rc;

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
            code.append(Instruction::Call(words.putdec()));
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
                code.append(Instruction::Call(words.putdec()));
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
            code.append(Instruction::Call(words.putdec()));
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
            code.append(Instruction::Call(words.putdec()));
            code.append(Instruction::Call(words.cr()));
            code.append(Instruction::Push(value(-7)));
            code.append(Instruction::Call(words.putdec()));
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
        let call = code.append(Instruction::Call(output_words.putdec()));
        code.append(Instruction::Halt);
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        let result = vm.step(execution(&code, &words, &primitives).with_output(&mut output));

        let error = result.expect_err("failed output should fail PUTDEC");
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

    #[derive(Debug, Default)]
    struct PartialThenFailOutput {
        chunks: Vec<String>,
    }

    impl RuntimeOutput for PartialThenFailOutput {
        fn write(&mut self, text: &str) -> Result<(), RuntimeOutputError> {
            self.chunks.push(text[..3].to_owned());
            Err(RuntimeOutputError::Failed)
        }
    }

    #[test]
    fn print_output_failure_does_not_require_external_effect_rollback() {
        let (primitives, words, _, output_words) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(21)));
        code.append(Instruction::Call(output_words.putdec()));
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
    fn putchr_outputs_ascii_representatives_and_consumes_only_the_top_value() {
        for (input, expected) in [
            (0, "\0"),
            (10, "\n"),
            (32, " "),
            (39, "'"),
            (48, "0"),
            (65, "A"),
            (97, "a"),
            (127, "\u{7f}"),
        ] {
            let (mut vm, output, result) = run_calls(|code, words| {
                code.append(Instruction::Push(value(7)));
                code.append(Instruction::Push(value(input)));
                code.append(Instruction::Call(words.putchr()));
            });

            assert_eq!(result, Ok(RunOutcome::Halted), "input {input}");
            assert_eq!(output.chunks(), [expected], "input {input}");
            assert_eq!(vm.data_stack_depth(), 1, "input {input}");
            assert_eq!(vm.pop_data(), Ok(value(7)), "input {input}");
        }
    }

    #[test]
    fn putchr_rejects_values_outside_ascii_without_output_or_stack_consumption() {
        for input in [-1, 128, 241] {
            let (mut vm, output, result) = run_calls(|code, words| {
                code.append(Instruction::Push(value(input)));
                code.append(Instruction::Call(words.putchr()));
            });

            let error = result.expect_err("out-of-range PUTCHR should fail");
            assert!(
                matches!(
                    error.kind(),
                    VmErrorKind::PrimitiveFailed {
                        source: PrimitiveError::AsciiOutOfRange { value },
                        ..
                    } if value == input
                ),
                "input {input}"
            );
            assert!(output.chunks().is_empty(), "input {input}");
            assert_eq!(vm.data_stack_depth(), 1, "input {input}");
            assert_eq!(vm.pop_data(), Ok(value(input)), "input {input}");
        }
    }

    #[test]
    fn putchr_output_failure_leaves_target_value_on_stack() {
        let (primitives, words, _, output_words) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(65)));
        let call = code.append(Instruction::Call(output_words.putchr()));
        code.append(Instruction::Halt);
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        let result = vm.step(execution(&code, &words, &primitives).with_output(&mut output));

        let error = result.expect_err("failed output should fail PUTCHR");
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
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.pop_data(), Ok(value(65)));
        assert_eq!(vm.instruction_pointer(), code.view().location(call));
    }

    #[test]
    fn output_primitive_bootstrap_publishes_putdec_putchr_and_cr_as_runtime_words() {
        let (primitives, words, bindings, output_words) = bootstrapped_output_words();

        assert_eq!(primitives.len(), 3);
        assert_eq!(words.len(), 3);
        assert_eq!(
            resolve_word_name(&bindings, "putdec"),
            Ok(output_words.putdec())
        );
        assert_eq!(
            resolve_word_name(&bindings, "putchr"),
            Ok(output_words.putchr())
        );
        assert_eq!(resolve_word_name(&bindings, "cr"), Ok(output_words.cr()));
        assert_eq!(
            bindings.get(&name("PUTDEC")),
            Some(&Binding::Word(output_words.putdec()))
        );
        assert_eq!(
            bindings.get(&name("PUTCHR")),
            Some(&Binding::Word(output_words.putchr()))
        );
        assert!(bindings.get(&name("PRINT")).is_none());
        assert_eq!(
            bindings.get(&name("CR")),
            Some(&Binding::Word(output_words.cr()))
        );
        assert!(matches!(
            words.get(output_words.putdec()),
            Ok(WordDefinition::Primitive { .. })
        ));
        assert!(matches!(
            words.get(output_words.putchr()),
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
        assert!(bindings.get(&name("PUTDEC")).is_none());
    }

    #[test]
    fn fixed_text_instruction_emits_exact_compile_time_text() {
        let (vm, output, result) = run_calls(|code, _| {
            code.append(Instruction::WriteFixedText(Rc::from(" a  b ")));
        });

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(output.chunks(), [" a  b "]);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn fixed_text_instruction_supports_empty_text() {
        let (_vm, output, result) = run_calls(|code, _| {
            code.append(Instruction::WriteFixedText(Rc::from("")));
        });

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(output.chunks(), [""]);
    }

    #[test]
    fn fixed_text_output_failure_stops_before_following_instruction() {
        let (primitives, words, _, _) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::WriteFixedText(Rc::from("text")));
        code.append(Instruction::WriteFixedText(Rc::from("after")));
        code.append(Instruction::Halt);
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");

        let result = vm.run(execution(&code, &words, &primitives).with_output(&mut output));

        let error = result.expect_err("fixed text output should fail");
        assert!(matches!(
            error.kind(),
            VmErrorKind::FixedTextOutputFailed {
                source: RuntimeOutputError::Failed
            }
        ));
        assert!(output.chunks().is_empty());
        assert_eq!(vm.instruction_pointer(), code.view().location(entry));
    }

    #[test]
    fn fixed_text_partial_output_is_not_rolled_back_and_does_not_advance() {
        let (primitives, words, _, _) = bootstrapped_output_words();
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::WriteFixedText(Rc::from("prefix")));
        code.append(Instruction::WriteFixedText(Rc::from("after")));
        code.append(Instruction::Halt);
        let mut output = PartialThenFailOutput::default();
        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");

        let result = vm.run(execution(&code, &words, &primitives).with_output(&mut output));

        assert!(result.is_err());
        assert_eq!(output.chunks, ["pre"]);
        assert_eq!(vm.instruction_pointer(), code.view().location(entry));
    }
}
