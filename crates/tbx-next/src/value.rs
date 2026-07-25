/// Runtime value stored on the TBX Next data stack.
///
/// ADR #1365 keeps the initial language value domain to signed 16-bit integers,
/// but that does not constrain VM instruction addresses, word identifiers, host
/// indexes, or return frames to 16 bits. Control information must remain in
/// dedicated VM types instead of being folded into this runtime value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Value {
    Integer(i16),
}

/// Error produced by deterministic value operations.
///
/// Checked arithmetic is part of the language contract from ADR #1365, so these
/// operations must not depend on Rust debug/release overflow behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueError {
    ArithmeticOverflow,
    DivisionByZero,
}

impl Value {
    pub(crate) const fn integer(value: i16) -> Self {
        Self::Integer(value)
    }

    pub(crate) const fn as_integer(self) -> i16 {
        match self {
            Self::Integer(value) => value,
        }
    }

    pub(crate) const fn is_zero(self) -> bool {
        self.as_integer() == 0
    }

    pub(crate) fn checked_add(self, rhs: Self) -> Result<Self, ValueError> {
        checked_integer_binary(self, rhs, i16::checked_add)
    }

    pub(crate) fn checked_sub(self, rhs: Self) -> Result<Self, ValueError> {
        checked_integer_binary(self, rhs, i16::checked_sub)
    }

    pub(crate) fn checked_mul(self, rhs: Self) -> Result<Self, ValueError> {
        checked_integer_binary(self, rhs, i16::checked_mul)
    }

    pub(crate) fn checked_div(self, rhs: Self) -> Result<Self, ValueError> {
        if rhs.is_zero() {
            return Err(ValueError::DivisionByZero);
        }

        checked_integer_binary(self, rhs, i16::checked_div)
    }

    pub(crate) fn checked_rem(self, rhs: Self) -> Result<Self, ValueError> {
        if rhs.is_zero() {
            return Err(ValueError::DivisionByZero);
        }

        checked_integer_binary(self, rhs, i16::checked_rem)
    }

    pub(crate) fn checked_neg(self) -> Result<Self, ValueError> {
        self.as_integer()
            .checked_neg()
            .map(Self::integer)
            .ok_or(ValueError::ArithmeticOverflow)
    }

    pub(crate) fn checked_abs(self) -> Result<Self, ValueError> {
        self.as_integer()
            .checked_abs()
            .map(Self::integer)
            .ok_or(ValueError::ArithmeticOverflow)
    }
}

fn checked_integer_binary(
    lhs: Value,
    rhs: Value,
    operation: impl FnOnce(i16, i16) -> Option<i16>,
) -> Result<Value, ValueError> {
    operation(lhs.as_integer(), rhs.as_integer())
        .map(Value::integer)
        .ok_or(ValueError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Value = Value::integer(i16::MIN);
    const MAX: Value = Value::integer(i16::MAX);
    const ZERO: Value = Value::integer(0);
    const ONE: Value = Value::integer(1);
    const NEG_ONE: Value = Value::integer(-1);

    #[test]
    fn integer_values_round_trip_full_i16_range_edges() {
        assert_eq!(MIN.as_integer(), i16::MIN);
        assert_eq!(MAX.as_integer(), i16::MAX);
        assert_eq!(ZERO.as_integer(), 0);
        assert_eq!(Value::integer(123).as_integer(), 123);
        assert_eq!(Value::integer(-123).as_integer(), -123);
    }

    #[test]
    fn only_zero_is_false() {
        assert!(ZERO.is_zero());
        assert!(!ONE.is_zero());
        assert!(!NEG_ONE.is_zero());
        assert!(!MIN.is_zero());
        assert!(!MAX.is_zero());
    }

    #[test]
    fn checked_add_returns_value_or_overflow() {
        assert_eq!(
            Value::integer(12).checked_add(Value::integer(30)),
            Ok(Value::integer(42))
        );
        assert_eq!(
            Value::integer(-12).checked_add(Value::integer(30)),
            Ok(Value::integer(18))
        );
        assert_eq!(MAX.checked_add(ONE), Err(ValueError::ArithmeticOverflow));
    }

    #[test]
    fn checked_sub_returns_value_or_overflow() {
        assert_eq!(
            Value::integer(30).checked_sub(Value::integer(12)),
            Ok(Value::integer(18))
        );
        assert_eq!(
            Value::integer(-12).checked_sub(Value::integer(30)),
            Ok(Value::integer(-42))
        );
        assert_eq!(MIN.checked_sub(ONE), Err(ValueError::ArithmeticOverflow));
    }

    #[test]
    fn checked_mul_returns_value_or_overflow() {
        assert_eq!(
            Value::integer(6).checked_mul(Value::integer(7)),
            Ok(Value::integer(42))
        );
        assert_eq!(
            Value::integer(-6).checked_mul(Value::integer(7)),
            Ok(Value::integer(-42))
        );
        assert_eq!(Value::integer(0).checked_mul(Value::integer(-7)), Ok(ZERO));
        assert_eq!(
            MAX.checked_mul(Value::integer(2)),
            Err(ValueError::ArithmeticOverflow)
        );
    }

    #[test]
    fn checked_div_uses_i16_truncation_and_reports_errors() {
        assert_eq!(
            Value::integer(7).checked_div(Value::integer(2)),
            Ok(Value::integer(3))
        );
        assert_eq!(
            Value::integer(-7).checked_div(Value::integer(2)),
            Ok(Value::integer(-3))
        );
        assert_eq!(
            Value::integer(7).checked_div(Value::integer(-2)),
            Ok(Value::integer(-3))
        );
        assert_eq!(
            MIN.checked_div(NEG_ONE),
            Err(ValueError::ArithmeticOverflow)
        );
        assert_eq!(ONE.checked_div(ZERO), Err(ValueError::DivisionByZero));
        assert_eq!(NEG_ONE.checked_div(ZERO), Err(ValueError::DivisionByZero));
    }

    #[test]
    fn checked_rem_uses_i16_remainder_and_reports_errors() {
        assert_eq!(Value::integer(7).checked_rem(Value::integer(2)), Ok(ONE));
        assert_eq!(
            Value::integer(-7).checked_rem(Value::integer(2)),
            Ok(NEG_ONE)
        );
        assert_eq!(Value::integer(7).checked_rem(Value::integer(-2)), Ok(ONE));
        assert_eq!(
            MIN.checked_rem(NEG_ONE),
            Err(ValueError::ArithmeticOverflow)
        );
        assert_eq!(ONE.checked_rem(ZERO), Err(ValueError::DivisionByZero));
        assert_eq!(NEG_ONE.checked_rem(ZERO), Err(ValueError::DivisionByZero));
    }

    #[test]
    fn checked_neg_returns_value_or_overflow() {
        assert_eq!(Value::integer(42).checked_neg(), Ok(Value::integer(-42)));
        assert_eq!(Value::integer(-42).checked_neg(), Ok(Value::integer(42)));
        assert_eq!(ZERO.checked_neg(), Ok(ZERO));
        assert_eq!(MIN.checked_neg(), Err(ValueError::ArithmeticOverflow));
    }

    #[test]
    fn checked_abs_returns_value_or_overflow() {
        assert_eq!(Value::integer(42).checked_abs(), Ok(Value::integer(42)));
        assert_eq!(Value::integer(-42).checked_abs(), Ok(Value::integer(42)));
        assert_eq!(ZERO.checked_abs(), Ok(ZERO));
        assert_eq!(MIN.checked_abs(), Err(ValueError::ArithmeticOverflow));
    }

    #[test]
    fn failed_operations_do_not_mutate_operands() {
        let lhs = MIN;
        let rhs = NEG_ONE;

        assert_eq!(lhs.checked_div(rhs), Err(ValueError::ArithmeticOverflow));
        assert_eq!(lhs, MIN);
        assert_eq!(rhs, NEG_ONE);
    }
}
