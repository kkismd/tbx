use crate::value::Value;

/// Crate-internal identity for one session-owned global array.
///
/// IDs are monotonically assigned and are never runtime values or public handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ArrayId {
    slot: usize,
}

impl ArrayId {
    #[cfg(test)]
    pub(crate) const fn test_invalid(slot: usize) -> Self {
        Self { slot }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalArrayError {
    InvalidArrayId {
        id: ArrayId,
    },
    IndexOutOfBounds {
        id: ArrayId,
        index: usize,
        len: usize,
    },
}

/// Owns global array elements for the lifetime of a processing session.
#[derive(Debug, Default)]
pub(crate) struct GlobalArrays {
    arrays: Vec<Vec<Value>>,
}

impl GlobalArrays {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn allocate(&mut self, len: usize) -> ArrayId {
        let id = ArrayId {
            slot: self.arrays.len(),
        };
        self.arrays.push(vec![Value::integer(0); len]);
        id
    }

    pub(crate) fn view(&self) -> GlobalArrayView<'_> {
        GlobalArrayView {
            arrays: &self.arrays,
        }
    }

    pub(crate) fn view_mut(&mut self) -> GlobalArrayViewMut<'_> {
        GlobalArrayViewMut {
            arrays: &mut self.arrays,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GlobalArrayView<'a> {
    arrays: &'a [Vec<Value>],
}

impl GlobalArrayView<'_> {
    pub(crate) fn read(self, id: ArrayId, index: usize) -> Result<Value, GlobalArrayError> {
        read_element(self.arrays, id, index)
    }
}

#[derive(Debug)]
pub(crate) struct GlobalArrayViewMut<'a> {
    arrays: &'a mut [Vec<Value>],
}

impl GlobalArrayViewMut<'_> {
    pub(crate) fn read(&self, id: ArrayId, index: usize) -> Result<Value, GlobalArrayError> {
        read_element(self.arrays, id, index)
    }

    pub(crate) fn write(
        &mut self,
        id: ArrayId,
        index: usize,
        value: Value,
    ) -> Result<(), GlobalArrayError> {
        let array = self
            .arrays
            .get_mut(id.slot)
            .ok_or(GlobalArrayError::InvalidArrayId { id })?;
        let len = array.len();
        let element = array
            .get_mut(index)
            .ok_or(GlobalArrayError::IndexOutOfBounds { id, index, len })?;
        *element = value;
        Ok(())
    }
}

fn read_element(
    arrays: &[Vec<Value>],
    id: ArrayId,
    index: usize,
) -> Result<Value, GlobalArrayError> {
    let array = arrays
        .get(id.slot)
        .ok_or(GlobalArrayError::InvalidArrayId { id })?;
    array
        .get(index)
        .copied()
        .ok_or(GlobalArrayError::IndexOutOfBounds {
            id,
            index,
            len: array.len(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_zero_initializes_independent_arrays_and_keeps_ids_stable() {
        let mut arrays = GlobalArrays::new();
        let first = arrays.allocate(2);
        let second = arrays.allocate(1);
        assert_ne!(first, second);
        assert_eq!(arrays.view().read(first, 0), Ok(Value::integer(0)));
        assert_eq!(arrays.view().read(first, 1), Ok(Value::integer(0)));
        assert_eq!(arrays.view().read(second, 0), Ok(Value::integer(0)));

        {
            let mut view = arrays.view_mut();
            view.write(first, 1, Value::integer(11)).unwrap();
            view.write(second, 0, Value::integer(22)).unwrap();
        }
        let third = arrays.allocate(1);
        assert_eq!(arrays.view().read(first, 1), Ok(Value::integer(11)));
        assert_eq!(arrays.view().read(second, 0), Ok(Value::integer(22)));
        assert_eq!(arrays.view().read(third, 0), Ok(Value::integer(0)));
    }

    #[test]
    fn invalid_id_and_indices_return_errors_without_mutation() {
        let mut arrays = GlobalArrays::new();
        let id = arrays.allocate(2);
        let invalid = ArrayId::test_invalid(5);
        let mut view = arrays.view_mut();

        assert_eq!(
            view.read(invalid, 0),
            Err(GlobalArrayError::InvalidArrayId { id: invalid })
        );
        for index in [2, usize::MAX] {
            let error = GlobalArrayError::IndexOutOfBounds { id, index, len: 2 };
            assert_eq!(view.read(id, index), Err(error));
            assert_eq!(view.write(id, index, Value::integer(9)), Err(error));
        }
        assert_eq!(
            view.write(invalid, 0, Value::integer(9)),
            Err(GlobalArrayError::InvalidArrayId { id: invalid })
        );
        assert_eq!(view.read(id, 0), Ok(Value::integer(0)));
        assert_eq!(view.read(id, 1), Ok(Value::integer(0)));
    }
}
