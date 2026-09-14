use crate::binding::{BindingInsertError, Bindings};
use crate::bootstrap::{register_primitive, PrimitiveBootstrapError};
use crate::name::NormalizedName;
use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::value::Value;
use crate::word::{PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RandomPrimitiveWords {
    rnd: WordId,
}

pub(crate) fn register_random_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<RandomPrimitiveWords, PrimitiveBootstrapError> {
    let name = NormalizedName::new("RND").expect("built-in random primitive name is valid");
    bindings
        .validate_new_name(&name)
        .map_err(|error| match error {
            BindingInsertError::NameConflict => PrimitiveBootstrapError::NameConflict,
            BindingInsertError::ReservedName => PrimitiveBootstrapError::ReservedName,
        })?;
    let primitive = primitives.register(rnd);
    let rnd = register_primitive(words, bindings, name, primitive)?;
    Ok(RandomPrimitiveWords { rnd })
}

impl RandomPrimitiveWords {
    pub(crate) const fn rnd(self) -> WordId {
        self.rnd
    }
}

fn rnd(context: &mut PrimitiveContext<'_, '_>) -> Result<(), PrimitiveError> {
    let upper_bound = context.peek()?.as_integer();
    let result = context.random_inclusive(upper_bound)?;
    context
        .pop()
        .expect("RND upper bound was checked before consuming it");
    context.push(Value::integer(result));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Bindings;
    use crate::instruction::{Instruction, InstructionSequence};
    use crate::primitive::PrimitiveRegistry;
    use crate::random::RandomState;
    use crate::vm::{ExecutionView, RunOutcome, Vm, VmErrorKind};
    use crate::word_lookup::PublishedWordLookup;

    fn run(
        seed: u64,
        upper_bound: i16,
    ) -> (RandomState, Vm, Result<RunOutcome, crate::vm::VmError>) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let random_words = register_random_primitives(&mut primitives, &mut words, &mut bindings)
            .expect("RND should bootstrap");
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(Value::integer(upper_bound)));
        code.append(Instruction::Call(random_words.rnd()));
        code.append(Instruction::Halt);
        let mut random = RandomState::seeded(seed);
        let mut vm = Vm::new(code.view(), entry).expect("entry should be valid");
        let result = {
            let mut execution = ExecutionView::new(
                code.view(),
                PublishedWordLookup::new(&words),
                primitives.lookup(),
            )
            .with_random(&mut random);
            vm.run(&mut execution)
        };
        (random, vm, result)
    }

    #[test]
    fn rnd_one_always_returns_one() {
        let (_, mut vm, result) = run(42, 1);
        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.pop_data(), Ok(Value::integer(1)));
    }

    #[test]
    fn rnd_stays_inside_inclusive_positive_bound() {
        for seed in 1..32 {
            let (_, mut vm, result) = run(seed, 97);
            assert_eq!(result, Ok(RunOutcome::Halted));
            let value = vm
                .pop_data()
                .expect("RND should return a value")
                .as_integer();
            assert!((1..=97).contains(&value));
        }
    }

    #[test]
    fn invalid_bound_does_not_advance_random_state_or_stack() {
        let (after, vm, result) = run(42, 0);
        assert!(
            matches!(result, Err(error) if matches!(error.kind(), VmErrorKind::PrimitiveFailed { source: PrimitiveError::InvalidRandomUpperBound { upper_bound: 0 }, .. }))
        );
        assert_eq!(vm.data_stack_depth(), 1);
        let expected = RandomState::seeded(42);
        assert_eq!(after, expected);
    }

    #[test]
    fn equal_seeds_reproduce_the_same_first_value() {
        let (_, mut first, first_result) = run(99, 100);
        let (_, mut second, second_result) = run(99, 100);
        assert_eq!(first_result, Ok(RunOutcome::Halted));
        assert_eq!(second_result, Ok(RunOutcome::Halted));
        assert_eq!(first.pop_data(), second.pop_data());
    }
}
