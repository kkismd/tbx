use crate::binding::{BindingInsertError, Bindings};
use crate::bootstrap::{register_primitive, PrimitiveBootstrapError};
use crate::name::NormalizedName;
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::value::Value;
use crate::word::{PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputPrimitiveWords {
    input_question: WordId,
}

pub(crate) fn register_input_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<InputPrimitiveWords, PrimitiveBootstrapError> {
    let name = NormalizedName::new("INPUT?").expect("built-in input primitive name is valid");
    bindings
        .validate_new_name(&name)
        .map_err(|error| match error {
            BindingInsertError::NameConflict => PrimitiveBootstrapError::NameConflict,
            BindingInsertError::ReservedName => PrimitiveBootstrapError::ReservedName,
        })?;
    let primitive = primitives.register(input_question);
    let input_question = register_primitive(words, bindings, name, primitive)?;
    Ok(InputPrimitiveWords { input_question })
}

impl InputPrimitiveWords {
    pub(crate) const fn input_question(self) -> WordId {
        self.input_question
    }
}

fn input_question(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    let Some(line) = context.read_input()? else {
        context.push(Value::integer(0));
        context.push(Value::integer(0));
        return Ok(());
    };
    let Some(value) = parse_decimal_i16(&line) else {
        context.push(Value::integer(0));
        context.push(Value::integer(0));
        return Ok(());
    };
    context.push(Value::integer(value));
    context.push(Value::integer(1));
    Ok(())
}

fn parse_decimal_i16(input: &str) -> Option<i16> {
    let input = input.trim_matches([' ', '\t']);
    let (sign, digits) = match input.as_bytes().first().copied() {
        Some(b'+') => (1i32, &input[1..]),
        Some(b'-') => (-1i32, &input[1..]),
        _ => (1i32, input),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.bytes().try_fold(0i32, |value, byte| {
        value.checked_mul(10)?.checked_add(i32::from(byte - b'0'))
    })?;
    let value = magnitude.checked_mul(sign)?;
    i16::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Bindings;
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::primitive::PrimitiveRegistry;
    use crate::runtime_input::{RuntimeInput, RuntimeInputError, TestInput};
    use crate::runtime_output::TestOutput;
    use crate::value::Value;
    use crate::vm::{ExecutionView, RunOutcome, Vm, VmErrorKind};
    use crate::word::PublishedWords;
    use crate::word_lookup::PublishedWordLookup;
    use crate::word_resolution::resolve_word_name;

    #[test]
    fn decimal_parser_accepts_strict_i16_values() {
        for (text, expected) in [
            ("0", 0),
            ("+42", 42),
            (" -42\t", -42),
            ("-32768", i16::MIN),
            ("32767", i16::MAX),
        ] {
            assert_eq!(parse_decimal_i16(text), Some(expected));
        }
    }

    #[test]
    fn decimal_parser_rejects_partial_invalid_and_out_of_range_values() {
        for text in ["", "   ", "+", "12x", "1 2", "32768", "-32769"] {
            assert_eq!(parse_decimal_i16(text), None, "{text:?}");
        }
    }

    #[test]
    fn test_input_can_model_io_failure_without_stdin() {
        let mut input = TestInput::new([Err(RuntimeInputError::Failed)]);
        assert_eq!(input.read_line(), Err(RuntimeInputError::Failed));
    }

    fn run_input(
        lines: Vec<Result<Option<String>, RuntimeInputError>>,
    ) -> (Vm, Result<RunOutcome, crate::vm::VmError>) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let input_words = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("INPUT? should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Halt);
        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let mut input = TestInput::new(lines);
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_input(&mut input);
        let result = vm.run(execution);
        (vm, result)
    }

    #[test]
    fn input_question_returns_value_then_success_flag() {
        let (mut vm, result) = run_input(vec![Ok(Some("  -42\t".into()))]);
        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.pop_data(), Ok(Value::integer(1)));
        assert_eq!(vm.pop_data(), Ok(Value::integer(-42)));
    }

    #[test]
    fn input_question_consumes_invalid_line_and_reads_the_next_line() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let input_words = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("INPUT? should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Halt);
        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let mut input = TestInput::new([Ok(Some("abc".into())), Ok(Some("42".into()))]);
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_input(&mut input);
        assert_eq!(vm.run(execution), Ok(RunOutcome::Halted));
        assert_eq!(vm.data_stack_depth(), 4);
        assert_eq!(vm.pop_data(), Ok(Value::integer(1)));
        assert_eq!(vm.pop_data(), Ok(Value::integer(42)));
        assert_eq!(vm.pop_data(), Ok(Value::integer(0)));
        assert_eq!(vm.pop_data(), Ok(Value::integer(0)));
    }

    #[test]
    fn input_question_maps_eof_to_zero_zero() {
        let (mut vm, result) = run_input(vec![Ok(None)]);
        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.pop_data(), Ok(Value::integer(0)));
        assert_eq!(vm.pop_data(), Ok(Value::integer(0)));
    }

    #[test]
    fn strict_input_exhaustion_fails_a_followup_input_question() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let input_words = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("INPUT? should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Halt);
        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let mut input = TestInput::strict([Ok(Some("42".into()))]);
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_input(&mut input);

        let error = vm
            .run(execution)
            .expect_err("an unexpected followup INPUT? should fail");
        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::InputFailed {
                    source: RuntimeInputError::Failed
                },
                ..
            }
        ));
        assert_eq!(vm.pop_data(), Ok(Value::integer(1)));
        assert_eq!(vm.pop_data(), Ok(Value::integer(42)));
    }

    #[test]
    fn input_failure_and_missing_capability_do_not_change_the_stack() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let input_words = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("INPUT? should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(Value::integer(7)));
        code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Halt);
        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let mut input = TestInput::new([Err(RuntimeInputError::Failed)]);
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_input(&mut input);
        let error = vm
            .run(execution)
            .expect_err("I/O failure should be runtime error");
        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::InputFailed {
                    source: RuntimeInputError::Failed
                },
                ..
            }
        ));
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.pop_data(), Ok(Value::integer(7)));

        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        );
        let error = vm
            .run(execution)
            .expect_err("missing input should be runtime error");
        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::InputFailed {
                    source: RuntimeInputError::Unavailable
                },
                ..
            }
        ));
        assert_eq!(vm.data_stack_depth(), 1);
    }

    #[test]
    fn input_word_is_published_in_the_shared_binding_namespace() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let input_words = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("INPUT? should bootstrap");
        assert_eq!(
            resolve_word_name(&bindings, "input?"),
            Ok(input_words.input_question())
        );
    }

    #[test]
    fn input_and_output_capabilities_are_available_to_the_same_execution() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let output_words = crate::output_primitive::register_output_primitives(
            &mut primitives,
            &mut words,
            &mut bindings,
        )
        .expect("output primitives should bootstrap");
        let input_words = register_input_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("INPUT? should bootstrap");

        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(Value::integer(7)));
        code.append(Instruction::Call(output_words.putdec()));
        code.append(Instruction::Call(input_words.input_question()));
        code.append(Instruction::Call(output_words.putdec()));
        code.append(Instruction::Call(output_words.putdec()));
        code.append(Instruction::Halt);

        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let mut input = TestInput::new([Ok(Some("42".into()))]);
        let mut output = TestOutput::new();
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        )
        .with_input(&mut input)
        .with_output(&mut output);

        assert_eq!(vm.run(execution), Ok(RunOutcome::Halted));
        assert_eq!(output.chunks(), ["7", "1", "42"]);
        assert_eq!(vm.data_stack_depth(), 0);
    }
}
