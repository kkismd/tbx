use crate::value::Value;

/// Crate-internal identifier for one global scalar variable slot.
///
/// ADR #1370 keeps global storage outside fresh `Vm` control state. IDs are
/// issued monotonically in this milestone and are not runtime `Value`s,
/// serialized handles, or reusable public ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlobalVarId {
    slot: usize,
}

impl GlobalVarId {
    #[cfg(test)]
    pub(crate) const fn test_invalid(slot: usize) -> Self {
        Self { slot }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalVariableError {
    InvalidGlobalVarId { id: GlobalVarId },
}

/// Session-owned storage for TBX Next global scalar variables.
///
/// The VM receives only narrow access views later; this owner is deliberately
/// independent from executable code, bindings, and transient VM execution state.
#[derive(Debug, Default)]
pub(crate) struct GlobalVariables {
    slots: Vec<Value>,
}

impl GlobalVariables {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn allocate(&mut self) -> GlobalVarId {
        let id = GlobalVarId {
            slot: self.slots.len(),
        };
        self.slots.push(Value::integer(0));
        id
    }

    pub(crate) fn view(&self) -> GlobalVariableView<'_> {
        GlobalVariableView { slots: &self.slots }
    }

    pub(crate) fn view_mut(&mut self) -> GlobalVariableViewMut<'_> {
        GlobalVariableViewMut {
            slots: &mut self.slots,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GlobalVariableView<'a> {
    slots: &'a [Value],
}

impl GlobalVariableView<'_> {
    pub(crate) fn read(self, id: GlobalVarId) -> Result<Value, GlobalVariableError> {
        self.slots
            .get(id.slot)
            .copied()
            .ok_or(GlobalVariableError::InvalidGlobalVarId { id })
    }
}

#[derive(Debug)]
pub(crate) struct GlobalVariableViewMut<'a> {
    slots: &'a mut [Value],
}

impl GlobalVariableViewMut<'_> {
    pub(crate) fn read(&self, id: GlobalVarId) -> Result<Value, GlobalVariableError> {
        self.slots
            .get(id.slot)
            .copied()
            .ok_or(GlobalVariableError::InvalidGlobalVarId { id })
    }

    pub(crate) fn write(
        &mut self,
        id: GlobalVarId,
        value: Value,
    ) -> Result<(), GlobalVariableError> {
        let slot = self
            .slots
            .get_mut(id.slot)
            .ok_or(GlobalVariableError::InvalidGlobalVarId { id })?;
        *slot = value;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_initializes_slot_to_integer_zero() {
        let mut globals = GlobalVariables::new();

        let id = globals.allocate();

        assert_eq!(globals.len(), 1);
        assert!(!globals.is_empty());
        assert_eq!(globals.view().read(id), Ok(Value::integer(0)));
    }

    #[test]
    fn multiple_slots_keep_independent_identity() {
        let mut globals = GlobalVariables::new();
        let first = globals.allocate();
        let second = globals.allocate();
        let third = globals.allocate();

        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);

        let mut view = globals.view_mut();
        view.write(first, Value::integer(10))
            .expect("first slot should be valid");
        view.write(second, Value::integer(20))
            .expect("second slot should be valid");
        view.write(third, Value::integer(30))
            .expect("third slot should be valid");

        assert_eq!(view.read(first), Ok(Value::integer(10)));
        assert_eq!(view.read(second), Ok(Value::integer(20)));
        assert_eq!(view.read(third), Ok(Value::integer(30)));
    }

    #[test]
    fn read_and_write_valid_id() {
        let mut globals = GlobalVariables::new();
        let id = globals.allocate();

        {
            let mut view = globals.view_mut();
            assert_eq!(view.write(id, Value::integer(-7)), Ok(()));
            assert_eq!(view.read(id), Ok(Value::integer(-7)));
        }

        assert_eq!(globals.view().read(id), Ok(Value::integer(-7)));
    }

    #[test]
    fn invalid_id_is_structured_error_without_mutation() {
        let mut globals = GlobalVariables::new();
        let valid = globals.allocate();
        let invalid = GlobalVarId::test_invalid(99);

        {
            let mut view = globals.view_mut();
            assert_eq!(
                view.write(invalid, Value::integer(42)),
                Err(GlobalVariableError::InvalidGlobalVarId { id: invalid })
            );
            assert_eq!(
                view.read(invalid),
                Err(GlobalVariableError::InvalidGlobalVarId { id: invalid })
            );
        }

        assert_eq!(globals.view().read(valid), Ok(Value::integer(0)));
    }

    #[test]
    fn later_allocations_do_not_change_previous_ids() {
        let mut globals = GlobalVariables::new();
        let first = globals.allocate();
        let second = globals.allocate();

        {
            let mut view = globals.view_mut();
            view.write(first, Value::integer(1))
                .expect("first slot should be valid");
            view.write(second, Value::integer(2))
                .expect("second slot should be valid");
        }

        let third = globals.allocate();

        assert_eq!(globals.view().read(first), Ok(Value::integer(1)));
        assert_eq!(globals.view().read(second), Ok(Value::integer(2)));
        assert_eq!(globals.view().read(third), Ok(Value::integer(0)));
    }
}
