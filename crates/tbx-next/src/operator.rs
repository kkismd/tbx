use crate::binding::{Binding, BindingInsertError, Bindings};
use crate::name::NormalizedName;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperatorBootstrapError {
    NameConflict,
    ReservedName,
    BindingRegistrationInvariantViolated,
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

/// Registers expression operators and binds their existing `WordId`s as words.
///
/// The operator lookup and user-facing names intentionally share the same
/// published definitions. Later explicit word redefinition may move a name
/// binding to a new `WordId`, but this returned lookup remains early-bound to
/// the original operator definitions.
pub(crate) fn register_named_operator_primitives(
    primitives: &mut PrimitiveRegistry,
    words: &mut PublishedWords,
    bindings: &mut Bindings,
) -> Result<OperatorWords, OperatorBootstrapError> {
    let names = named_operator_word_names();

    for (_, name) in &names {
        bindings
            .validate_new_name(name)
            .map_err(OperatorBootstrapError::from_precheck_error)?;
    }

    let operators = register_operator_primitives(primitives, words);

    for (semantic, name) in names {
        let id = operators.lookup().resolve(semantic);
        bindings
            .insert_new(name, Binding::Word(id))
            .map_err(OperatorBootstrapError::from_binding_insert_error)?;
    }

    Ok(operators)
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

fn named_operator_word_names() -> [(OperatorSemantic, NormalizedName); 12] {
    [
        (OperatorSemantic::Add, named_operator_word_name("ADD")),
        (
            OperatorSemantic::Subtract,
            named_operator_word_name("SUBTRACT"),
        ),
        (
            OperatorSemantic::Multiply,
            named_operator_word_name("MULTIPLY"),
        ),
        (OperatorSemantic::Divide, named_operator_word_name("DIVIDE")),
        (
            OperatorSemantic::Remainder,
            named_operator_word_name("REMAINDER"),
        ),
        (OperatorSemantic::Negate, named_operator_word_name("NEGATE")),
        (OperatorSemantic::Equal, named_operator_word_name("EQUAL?")),
        (
            OperatorSemantic::NotEqual,
            named_operator_word_name("NOT_EQUAL?"),
        ),
        (OperatorSemantic::Less, named_operator_word_name("LESS?")),
        (
            OperatorSemantic::LessEqual,
            named_operator_word_name("LESS_EQUAL?"),
        ),
        (
            OperatorSemantic::Greater,
            named_operator_word_name("GREATER?"),
        ),
        (
            OperatorSemantic::GreaterEqual,
            named_operator_word_name("GREATER_EQUAL?"),
        ),
    ]
}

fn named_operator_word_name(input: &str) -> NormalizedName {
    NormalizedName::new(input).expect("named operator word should have a valid name")
}

impl OperatorBootstrapError {
    fn from_precheck_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::NameConflict,
            BindingInsertError::ReservedName => Self::ReservedName,
        }
    }

    fn from_binding_insert_error(error: BindingInsertError) -> Self {
        match error {
            BindingInsertError::NameConflict => Self::BindingRegistrationInvariantViolated,
            BindingInsertError::ReservedName => Self::BindingRegistrationInvariantViolated,
        }
    }
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
    use crate::word_resolution::resolve_word_name;

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

    const NAMED_OPERATORS: [(OperatorSemantic, &str); 12] = [
        (OperatorSemantic::Add, "ADD"),
        (OperatorSemantic::Subtract, "SUBTRACT"),
        (OperatorSemantic::Multiply, "MULTIPLY"),
        (OperatorSemantic::Divide, "DIVIDE"),
        (OperatorSemantic::Remainder, "REMAINDER"),
        (OperatorSemantic::Negate, "NEGATE"),
        (OperatorSemantic::Equal, "EQUAL?"),
        (OperatorSemantic::NotEqual, "NOT_EQUAL?"),
        (OperatorSemantic::Less, "LESS?"),
        (OperatorSemantic::LessEqual, "LESS_EQUAL?"),
        (OperatorSemantic::Greater, "GREATER?"),
        (OperatorSemantic::GreaterEqual, "GREATER_EQUAL?"),
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
    fn named_operator_registration_binds_all_names_to_operator_word_ids() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let operators =
            register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("named operators should bootstrap");

        assert_eq!(words.len(), ALL_SEMANTICS.len());
        assert_eq!(primitives.len(), ALL_SEMANTICS.len());
        assert_eq!(bindings.len(), ALL_SEMANTICS.len());
        for (semantic, input) in NAMED_OPERATORS {
            let operator_id = operators.lookup().resolve(semantic);

            assert_eq!(resolve_word_name(&bindings, input), Ok(operator_id));
            assert!(matches!(
                words.get(operator_id),
                Ok(WordDefinition::Primitive { .. })
            ));
        }
    }

    #[test]
    fn named_operator_registration_prechecks_name_conflicts() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let existing = words.add(CompletedWordDefinition::primitive(PrimitiveId::from_slot(
            100,
        )));
        bindings
            .insert_new(name("ADD"), Binding::Word(existing))
            .expect("test setup should bind ADD");

        let result = register_named_operator_primitives(&mut primitives, &mut words, &mut bindings);

        assert_eq!(result, Err(OperatorBootstrapError::NameConflict));
        assert_eq!(primitives.len(), 0);
        assert_eq!(words.len(), 1);
        assert_eq!(resolve_word_name(&bindings, "ADD"), Ok(existing));
    }

    #[test]
    fn named_word_redefinition_does_not_change_operator_lookup() {
        let mut primitives = PrimitiveRegistry::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let operators =
            register_named_operator_primitives(&mut primitives, &mut words, &mut bindings)
                .expect("named operators should bootstrap");
        let operator_add = operators.lookup().resolve(OperatorSemantic::Add);
        let replacement = CompletedWordDefinition::primitive(PrimitiveId::from_slot(100));

        let redefinition = redefine_word(&mut words, &mut bindings, &name("ADD"), replacement)
            .expect("user word should redefine");

        assert_eq!(
            operators.lookup().resolve(OperatorSemantic::Add),
            operator_add
        );
        assert_eq!(redefinition.previous(), operator_add);
        assert_ne!(operator_add, redefinition.current());
        assert_eq!(
            resolve_word_name(&bindings, "ADD"),
            Ok(redefinition.current())
        );
    }
}
