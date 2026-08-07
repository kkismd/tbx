use crate::primitive::{PrimitiveContext, PrimitiveError, PrimitiveRegistry};
use crate::value::{Value, ValueError};
use crate::word::{CompletedWordDefinition, PublishedWords, WordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OperatorSemantic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Negate,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperatorWords {
    add: WordId,
    subtract: WordId,
    multiply: WordId,
    divide: WordId,
    remainder: WordId,
    negate: WordId,
    equal: WordId,
    not_equal: WordId,
    less: WordId,
    less_equal: WordId,
    greater: WordId,
    greater_equal: WordId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OperatorLookup {
    words: OperatorWords,
}

/// Registers expression operators without creating surface name bindings.
///
/// Operators are syntax semantics, not normal words: later source processors
/// can lower them to early-bound `Call(WordId)` instructions without consulting
/// or mutating the ordinary `Bindings` table.
pub(crate) fn register_operator_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
) -> OperatorWords {
    let add = register_operator(primitives, words, add);
    let subtract = register_operator(primitives, words, subtract);
    let multiply = register_operator(primitives, words, multiply);
    let divide = register_operator(primitives, words, divide);
    let remainder = register_operator(primitives, words, remainder);
    let negate = register_operator(primitives, words, negate);
    let equal = register_operator(primitives, words, equal);
    let not_equal = register_operator(primitives, words, not_equal);
    let less = register_operator(primitives, words, less);
    let less_equal = register_operator(primitives, words, less_equal);
    let greater = register_operator(primitives, words, greater);
    let greater_equal = register_operator(primitives, words, greater_equal);

    OperatorWords {
        add,
        subtract,
        multiply,
        divide,
        remainder,
        negate,
        equal,
        not_equal,
        less,
        less_equal,
        greater,
        greater_equal,
    }
}

impl OperatorWords {
    pub(crate) const fn lookup(self) -> OperatorLookup {
        OperatorLookup { words: self }
    }
}

impl OperatorLookup {
    pub(crate) const fn resolve(self, semantic: OperatorSemantic) -> WordId {
        match semantic {
            OperatorSemantic::Add => self.words.add,
            OperatorSemantic::Subtract => self.words.subtract,
            OperatorSemantic::Multiply => self.words.multiply,
            OperatorSemantic::Divide => self.words.divide,
            OperatorSemantic::Remainder => self.words.remainder,
            OperatorSemantic::Negate => self.words.negate,
            OperatorSemantic::Equal => self.words.equal,
            OperatorSemantic::NotEqual => self.words.not_equal,
            OperatorSemantic::Less => self.words.less,
            OperatorSemantic::LessEqual => self.words.less_equal,
            OperatorSemantic::Greater => self.words.greater,
            OperatorSemantic::GreaterEqual => self.words.greater_equal,
        }
    }
}

fn register_operator(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    handler: fn(&mut PrimitiveContext<'_>) -> Result<(), PrimitiveError>,
) -> WordId {
    let primitive = primitives.register(handler);
    words.add(CompletedWordDefinition::primitive(primitive))
}

fn add(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    checked_binary(context, Value::checked_add)
}

fn subtract(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    checked_binary(context, Value::checked_sub)
}

fn multiply(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    checked_binary(context, Value::checked_mul)
}

fn divide(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    checked_binary(context, Value::checked_div)
}

fn remainder(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    checked_binary(context, Value::checked_rem)
}

fn negate(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    let value = context.pop()?;
    context.push(value.checked_neg().map_err(primitive_value_error)?);
    Ok(())
}

fn equal(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    comparison(context, i16::eq)
}

fn not_equal(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    comparison(context, i16::ne)
}

fn less(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    comparison(context, i16::lt)
}

fn less_equal(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    comparison(context, i16::le)
}

fn greater(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    comparison(context, i16::gt)
}

fn greater_equal(context: &mut PrimitiveContext<'_>) -> Result<(), PrimitiveError> {
    comparison(context, i16::ge)
}

fn checked_binary(
    context: &mut PrimitiveContext<'_>,
    operation: fn(Value, Value) -> Result<Value, ValueError>,
) -> Result<(), PrimitiveError> {
    let (lhs, rhs) = context.pop2()?;
    context.push(operation(lhs, rhs).map_err(primitive_value_error)?);
    Ok(())
}

fn comparison(
    context: &mut PrimitiveContext<'_>,
    predicate: fn(&i16, &i16) -> bool,
) -> Result<(), PrimitiveError> {
    let (lhs, rhs) = context.pop2()?;
    let result = if predicate(&lhs.as_integer(), &rhs.as_integer()) {
        1
    } else {
        0
    };
    context.push(Value::integer(result));
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
    use crate::name::NormalizedName;
    use crate::primitive::PrimitiveRegistry;
    use crate::redefinition::redefine_word;
    use crate::vm::{ExecutionView, RunOutcome, Vm, VmErrorKind};
    use crate::word::{PrimitiveId, WordDefinition};
    use crate::word_lookup::PublishedWordLookup;
    use crate::word_resolution::{resolve_word_name, WordResolutionError};

    const ALL_SEMANTICS: [OperatorSemantic; 12] = [
        OperatorSemantic::Add,
        OperatorSemantic::Subtract,
        OperatorSemantic::Multiply,
        OperatorSemantic::Divide,
        OperatorSemantic::Remainder,
        OperatorSemantic::Negate,
        OperatorSemantic::Equal,
        OperatorSemantic::NotEqual,
        OperatorSemantic::Less,
        OperatorSemantic::LessEqual,
        OperatorSemantic::Greater,
        OperatorSemantic::GreaterEqual,
    ];

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
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

    fn run_operator(
        semantic: OperatorSemantic,
        inputs: &[Value],
    ) -> (Vm, Result<RunOutcome, crate::vm::VmError>) {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let mut code = InstructionSequence::new();

        let entry = code.append(Instruction::Push(inputs[0]));
        for value in &inputs[1..] {
            code.append(Instruction::Push(*value));
        }
        code.append(Instruction::Call(operators.lookup().resolve(semantic)));
        code.append(Instruction::Halt);

        let mut vm = Vm::new(code.view(), entry).expect("test entry should be valid");
        let result = vm.run(execution(&code, &words, &primitives));
        (vm, result)
    }

    fn assert_operator_result(semantic: OperatorSemantic, inputs: &[Value], expected: Value) {
        let (mut vm, result) = run_operator(semantic, inputs);

        assert_eq!(result, Ok(RunOutcome::Halted));
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.pop_data(), Ok(expected));
    }

    fn assert_operator_failure(semantic: OperatorSemantic, inputs: &[Value]) {
        let (mut vm, result) = run_operator(semantic, inputs);
        let error = result.expect_err("operator should fail");

        assert!(matches!(
            error.kind(),
            VmErrorKind::PrimitiveFailed {
                source: PrimitiveError::Failed,
                ..
            }
        ));
        assert_eq!(vm.data_stack_depth(), inputs.len());
        for expected in inputs.iter().rev() {
            assert_eq!(vm.pop_data(), Ok(*expected));
        }
    }

    #[test]
    fn arithmetic_operators_return_checked_results() {
        assert_operator_result(OperatorSemantic::Add, &[value(19), value(23)], value(42));
        assert_operator_result(
            OperatorSemantic::Subtract,
            &[value(19), value(23)],
            value(-4),
        );
        assert_operator_result(
            OperatorSemantic::Multiply,
            &[value(-6), value(7)],
            value(-42),
        );
        assert_operator_result(OperatorSemantic::Divide, &[value(-7), value(2)], value(-3));
        assert_operator_result(
            OperatorSemantic::Remainder,
            &[value(-7), value(2)],
            value(-1),
        );
    }

    #[test]
    fn unary_negation_returns_checked_result() {
        assert_operator_result(OperatorSemantic::Negate, &[value(42)], value(-42));
        assert_operator_result(OperatorSemantic::Negate, &[value(-42)], value(42));
        assert_operator_result(OperatorSemantic::Negate, &[value(0)], value(0));
    }

    #[test]
    fn arithmetic_errors_fail_deterministically_and_preserve_stack() {
        assert_operator_failure(OperatorSemantic::Add, &[value(i16::MAX), value(1)]);
        assert_operator_failure(OperatorSemantic::Subtract, &[value(i16::MIN), value(1)]);
        assert_operator_failure(OperatorSemantic::Multiply, &[value(i16::MAX), value(2)]);
        assert_operator_failure(OperatorSemantic::Divide, &[value(i16::MIN), value(-1)]);
        assert_operator_failure(OperatorSemantic::Remainder, &[value(i16::MIN), value(-1)]);
        assert_operator_failure(OperatorSemantic::Divide, &[value(1), value(0)]);
        assert_operator_failure(OperatorSemantic::Remainder, &[value(1), value(0)]);
        assert_operator_failure(OperatorSemantic::Negate, &[value(i16::MIN)]);
    }

    #[test]
    fn comparison_operators_return_zero_or_one() {
        assert_operator_result(OperatorSemantic::Equal, &[value(3), value(3)], value(1));
        assert_operator_result(OperatorSemantic::Equal, &[value(3), value(4)], value(0));
        assert_operator_result(OperatorSemantic::NotEqual, &[value(3), value(4)], value(1));
        assert_operator_result(OperatorSemantic::NotEqual, &[value(3), value(3)], value(0));
        assert_operator_result(OperatorSemantic::Less, &[value(3), value(4)], value(1));
        assert_operator_result(OperatorSemantic::Less, &[value(4), value(3)], value(0));
        assert_operator_result(OperatorSemantic::LessEqual, &[value(3), value(3)], value(1));
        assert_operator_result(OperatorSemantic::LessEqual, &[value(4), value(3)], value(0));
        assert_operator_result(OperatorSemantic::Greater, &[value(4), value(3)], value(1));
        assert_operator_result(OperatorSemantic::Greater, &[value(3), value(4)], value(0));
        assert_operator_result(
            OperatorSemantic::GreaterEqual,
            &[value(3), value(3)],
            value(1),
        );
        assert_operator_result(
            OperatorSemantic::GreaterEqual,
            &[value(3), value(4)],
            value(0),
        );
    }

    #[test]
    fn operator_lookup_returns_stable_published_word_ids() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let lookup = operators.lookup();

        assert_eq!(words.len(), ALL_SEMANTICS.len());
        assert_eq!(primitives.len(), ALL_SEMANTICS.len());
        for semantic in ALL_SEMANTICS {
            let first = lookup.resolve(semantic);
            let second = lookup.resolve(semantic);

            assert_eq!(first, second);
            assert!(matches!(
                words.get(first),
                Ok(WordDefinition::Primitive { .. })
            ));
        }
    }

    #[test]
    fn operator_registration_does_not_require_surface_bindings() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let bindings = Bindings::new();

        let operators = register_operator_primitives(&mut primitives, &mut words);

        assert!(bindings.is_empty());
        assert_eq!(words.len(), ALL_SEMANTICS.len());
        assert_eq!(
            resolve_word_name(&bindings, "ADD"),
            Err(WordResolutionError::UndefinedName)
        );
        assert!(matches!(
            words.get(operators.lookup().resolve(OperatorSemantic::Add)),
            Ok(WordDefinition::Primitive { .. })
        ));
    }

    #[test]
    fn user_word_redefinition_does_not_change_operator_lookup() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let operators = register_operator_primitives(&mut primitives, &mut words);
        let operator_add = operators.lookup().resolve(OperatorSemantic::Add);
        let user_old = words.add(CompletedWordDefinition::primitive(PrimitiveId::from_slot(
            100,
        )));
        bindings
            .insert_new(name("ADD"), Binding::Word(user_old))
            .expect("user word should bind");
        let user_new = CompletedWordDefinition::primitive(PrimitiveId::from_slot(101));

        let redefinition = redefine_word(&mut words, &mut bindings, &name("ADD"), user_new)
            .expect("user word should redefine");

        assert_eq!(
            operators.lookup().resolve(OperatorSemantic::Add),
            operator_add
        );
        assert_ne!(operator_add, user_old);
        assert_ne!(operator_add, redefinition.current());
        assert_eq!(
            resolve_word_name(&bindings, "ADD"),
            Ok(redefinition.current())
        );
    }
}
