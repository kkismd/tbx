use crate::binding::{BindingInsertError, Bindings};
use crate::bootstrap::{register_primitive, PrimitiveBootstrapError};
use crate::name::NormalizedName;
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::word::{PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StackPrimitiveWords {
    dup: WordId,
    drop: WordId,
}

pub(crate) fn register_stack_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<StackPrimitiveWords, PrimitiveBootstrapError> {
    let dup_name = builtin_name("DUP");
    let drop_name = builtin_name("DROP");
    bindings
        .validate_new_name(&dup_name)
        .map_err(primitive_bootstrap_precheck_error)?;
    bindings
        .validate_new_name(&drop_name)
        .map_err(primitive_bootstrap_precheck_error)?;

    let dup_primitive = primitives.register(dup);
    let dup = register_primitive(words, bindings, dup_name, dup_primitive)?;
    let drop_primitive = primitives.register(drop);
    let drop = register_primitive(words, bindings, drop_name, drop_primitive)?;

    Ok(StackPrimitiveWords { dup, drop })
}

impl StackPrimitiveWords {
    pub(crate) const fn dup(self) -> WordId {
        self.dup
    }

    pub(crate) const fn drop(self) -> WordId {
        self.drop
    }
}

fn builtin_name(input: &str) -> NormalizedName {
    NormalizedName::new(input).expect("built-in stack primitive name should be valid")
}

fn primitive_bootstrap_precheck_error(error: BindingInsertError) -> PrimitiveBootstrapError {
    match error {
        BindingInsertError::NameConflict => PrimitiveBootstrapError::NameConflict,
        BindingInsertError::ReservedName => PrimitiveBootstrapError::ReservedName,
    }
}

fn dup(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    let value = context.peek()?;
    context.push(value);
    Ok(())
}

fn drop(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    context.pop()?;
    Ok(())
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

    fn run_dup(inputs: &[Value]) -> (Vm, Result<RunOutcome, crate::vm::VmError>) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let stack_words = register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("stack primitives should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = if let Some((first, rest)) = inputs.split_first() {
            let entry = code.append(Instruction::Push(*first));
            for value in rest {
                code.append(Instruction::Push(*value));
            }
            code.append(Instruction::Call(stack_words.dup()));
            entry
        } else {
            code.append(Instruction::Call(stack_words.dup()))
        };
        code.append(Instruction::Halt);

        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");
        let result = vm.run(execution(&code, &words, &primitives));
        (vm, result)
    }

    fn run_drop(inputs: &[Value]) -> (Vm, Result<RunOutcome, crate::vm::VmError>) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let stack_words = register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("stack primitives should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = if let Some((first, rest)) = inputs.split_first() {
            let entry = code.append(Instruction::Push(*first));
            for value in rest {
                code.append(Instruction::Push(*value));
            }
            code.append(Instruction::Call(stack_words.drop()));
            entry
        } else {
            code.append(Instruction::Call(stack_words.drop()))
        };
        code.append(Instruction::Halt);

        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");
        let result = vm.run(execution(&code, &words, &primitives));
        (vm, result)
    }

    #[test]
    fn dup_copies_the_only_stack_value() {
        let (mut vm, result) = run_dup(&[value(7)]);

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.data_stack_depth(), 2);
        assert_eq!(vm.pop_data(), Ok(value(7)));
        assert_eq!(vm.pop_data(), Ok(value(7)));
    }

    #[test]
    fn dup_copies_only_top_value_without_reordering_lower_values() {
        let (mut vm, result) = run_dup(&[value(3), value(5), value(8)]);

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.data_stack_depth(), 4);
        assert_eq!(vm.pop_data(), Ok(value(8)));
        assert_eq!(vm.pop_data(), Ok(value(8)));
        assert_eq!(vm.pop_data(), Ok(value(5)));
        assert_eq!(vm.pop_data(), Ok(value(3)));
    }

    #[test]
    fn dup_underflow_fails_without_changing_the_stack() {
        let (vm, result) = run_dup(&[]);
        let error = result.expect_err("empty stack should make DUP fail");

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
    fn drop_consumes_only_the_top_value() {
        let (mut vm, result) = run_drop(&[value(3), value(5), value(8)]);

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.data_stack_depth(), 2);
        assert_eq!(vm.pop_data(), Ok(value(5)));
        assert_eq!(vm.pop_data(), Ok(value(3)));
    }

    #[test]
    fn drop_underflow_fails_without_changing_the_stack() {
        let (vm, result) = run_drop(&[]);
        let error = result.expect_err("empty stack should make DROP fail");

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
    fn stack_primitive_bootstrap_publishes_dup_as_runtime_word() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let stack_words = register_stack_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("stack primitives should bootstrap");

        assert_eq!(primitives.len(), 2);
        assert_eq!(words.len(), 2);
        assert_eq!(resolve_word_name(&bindings, "dup"), Ok(stack_words.dup()));
        assert_eq!(resolve_word_name(&bindings, "drop"), Ok(stack_words.drop()));
        assert_eq!(
            bindings.get(&name("DUP")),
            Some(&Binding::Word(stack_words.dup()))
        );
        assert!(matches!(
            words.get(stack_words.dup()),
            Ok(WordDefinition::Primitive { .. })
        ));
        assert!(matches!(
            words.get(stack_words.drop()),
            Ok(WordDefinition::Primitive { .. })
        ));
    }

    #[test]
    fn stack_primitive_bootstrap_prechecks_dup_name_conflict() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let existing = WordId::test_invalid(0);
        bindings
            .insert_new(name("DUP"), Binding::Word(existing))
            .expect("test setup should bind DUP");

        let result = register_stack_primitives(&mut primitives, &mut words, &mut bindings);

        assert_eq!(result, Err(PrimitiveBootstrapError::NameConflict));
        assert_eq!(primitives.len(), 0);
        assert_eq!(words.len(), 0);
        assert_eq!(bindings.get(&name("DUP")), Some(&Binding::Word(existing)));
    }
}
