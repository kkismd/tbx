use crate::binding::{BindingInsertError, Bindings};
use crate::bootstrap::{register_primitive, PrimitiveBootstrapError};
use crate::name::NormalizedName;
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::value::ValueError;
use crate::word::{PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArithmeticPrimitiveWords {
    abs: WordId,
}

pub(crate) fn register_arithmetic_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<ArithmeticPrimitiveWords, PrimitiveBootstrapError> {
    let name = NormalizedName::new("ABS").expect("built-in arithmetic primitive name is valid");
    bindings
        .validate_new_name(&name)
        .map_err(|error| match error {
            BindingInsertError::NameConflict => PrimitiveBootstrapError::NameConflict,
            BindingInsertError::ReservedName => PrimitiveBootstrapError::ReservedName,
        })?;
    let primitive = primitives.register(abs);
    let abs = register_primitive(words, bindings, name, primitive)?;
    Ok(ArithmeticPrimitiveWords { abs })
}

impl ArithmeticPrimitiveWords {
    pub(crate) const fn abs(self) -> WordId {
        self.abs
    }
}

fn abs(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    let result = context
        .peek()?
        .checked_abs()
        .map_err(primitive_value_error)?;
    context
        .pop()
        .expect("ABS operand was checked before consuming it");
    context.push(result);
    Ok(())
}

fn primitive_value_error(_error: ValueError) -> PrimitiveError {
    PrimitiveError::Failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Bindings};
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::primitive::PrimitiveRegistry;
    use crate::value::Value;
    use crate::vm::{ExecutionView, RunOutcome, Vm, VmErrorKind};
    use crate::word::{PublishedWords, WordDefinition};
    use crate::word_lookup::PublishedWordLookup;
    use crate::word_resolution::resolve_word_name;

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid name")
    }

    fn run_abs(inputs: &[Value]) -> (Vm, Result<RunOutcome, crate::vm::VmError>) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let arithmetic_words =
            register_arithmetic_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("arithmetic primitives should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = if let Some((first, rest)) = inputs.split_first() {
            let entry = code.append(Instruction::Push(*first));
            for input in rest {
                code.append(Instruction::Push(*input));
            }
            code.append(Instruction::Call(arithmetic_words.abs()));
            entry
        } else {
            code.append(Instruction::Call(arithmetic_words.abs()))
        };
        code.append(Instruction::Halt);

        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let execution = ExecutionView::new(
            code.view(),
            PublishedWordLookup::new(&words),
            primitives.lookup(),
        );
        let result = vm.run(execution);
        (vm, result)
    }

    #[test]
    fn abs_replaces_the_top_value_with_its_checked_absolute_value() {
        for (input, expected) in [(0, 0), (42, 42), (-42, 42)] {
            let (mut vm, result) = run_abs(&[value(7), value(input)]);

            assert_eq!(result, Ok(RunOutcome::Halted), "input {input}");
            assert_eq!(vm.data_stack_depth(), 2, "input {input}");
            assert_eq!(vm.pop_data(), Ok(value(expected)), "input {input}");
            assert_eq!(vm.pop_data(), Ok(value(7)), "input {input}");
        }
    }

    #[test]
    fn abs_minimum_integer_fails_without_changing_the_stack() {
        let (mut vm, result) = run_abs(&[value(7), value(i16::MIN)]);
        let error = result.expect_err("minimum integer ABS should fail");

        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::Failed,
                ..
            }
        ));
        assert_eq!(vm.data_stack_depth(), 2);
        assert_eq!(vm.pop_data(), Ok(value(i16::MIN)));
        assert_eq!(vm.pop_data(), Ok(value(7)));
    }

    #[test]
    fn abs_underflow_fails_without_changing_the_stack() {
        let (vm, result) = run_abs(&[]);
        let error = result.expect_err("empty stack should make ABS fail");

        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::DataStackUnderflow { .. },
                ..
            }
        ));
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn arithmetic_primitive_bootstrap_publishes_abs_as_runtime_word() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let arithmetic_words =
            register_arithmetic_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("arithmetic primitives should bootstrap");

        assert_eq!(primitives.len(), 1);
        assert_eq!(words.len(), 1);
        assert_eq!(
            resolve_word_name(&bindings, "abs"),
            Ok(arithmetic_words.abs())
        );
        assert_eq!(
            bindings.get(&name("ABS")),
            Some(&Binding::Word(arithmetic_words.abs()))
        );
        assert!(matches!(
            words.get(arithmetic_words.abs()),
            Ok(WordDefinition::Primitive { .. })
        ));
    }

    #[test]
    fn arithmetic_primitive_bootstrap_prechecks_abs_name_conflict() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let existing = WordId::test_invalid(0);
        bindings
            .insert_new(name("ABS"), Binding::Word(existing))
            .expect("test setup should bind ABS");

        let result = register_arithmetic_primitives(&mut primitives, &mut words, &mut bindings);

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(primitives.len(), 0);
        assert_eq!(words.len(), 0);
        assert_eq!(bindings.get(&name("ABS")), Some(&Binding::Word(existing)));
    }
}
