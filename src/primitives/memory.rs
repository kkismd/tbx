use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// Reject `Cell::Array` writes to user-facing `DictAddr` / `StackAddr` destinations.
///
/// Surface-level `SET` and `STORE` must never write a whole-array handle to a
/// scalar variable slot.  DIM local array initialisation bypasses this guard via
/// the hidden `ARRAY_STORE_LOCAL` primitive.
fn reject_array_value(value: &Cell) -> Result<(), TbxError> {
    if matches!(value, Cell::Array(_)) {
        return Err(TbxError::TypeError {
            expected: "scalar value (Int, Float, Bool, Str, or Tuple)",
            got: "Array",
        });
    }
    Ok(())
}

/// Write `value` to element `elem_idx` of the array at `pool_idx`.
fn write_array_element(
    vm: &mut VM,
    pool_idx: usize,
    elem_idx: usize,
    value: Cell,
) -> Result<(), TbxError> {
    // Validate element type before touching the array.
    super::arrays::check_array_element_value(&value)?;
    let pool_size = vm.arrays.len();
    let ar = vm
        .arrays
        .get(pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: pool_idx,
            size: pool_size,
        })?
        .clone();
    ar.set(elem_idx, value)
}

/// FETCH — fetch a value from an address and push it onto the stack.
pub fn fetch_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            let value = vm.dict_read(a)?;
            vm.push(value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            let value = vm.local_read(a)?;
            vm.push(value)?;
            Ok(())
        }
        Cell::ArrayAddr { pool_idx, elem_idx } => {
            let ar = vm
                .arrays
                .get(pool_idx)
                .ok_or(TbxError::IndexOutOfBounds {
                    index: pool_idx,
                    size: vm.arrays.len(),
                })?
                .clone();
            let size = ar.len();
            let value = ar
                .get_cloned(elem_idx)
                .ok_or(TbxError::ArrayIndexOutOfBounds {
                    index: elem_idx as i64,
                    size,
                })?;
            vm.push(value)?;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// STORE — pop addr (top) then value (below), and store value at addr.
///
/// Stack convention: `[..., value, addr]` → STORE → `[...]`
///
/// `Cell::Array` values are rejected for both `DictAddr` and `StackAddr` destinations.
/// DIM local array initialisation uses the hidden `ARRAY_STORE_LOCAL` primitive instead.
pub fn store_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    let value = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            reject_array_value(&value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            reject_array_value(&value)?;
            vm.local_write(a, value)?;
            Ok(())
        }
        Cell::ArrayAddr { pool_idx, elem_idx } => {
            write_array_element(vm, pool_idx, elem_idx, value)
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// SET — pop value (top) then addr (below), and store value at addr.
///
/// Designed for the `SET &var, value` statement pattern where `&var` is
/// pushed before `value` (left-to-right argument evaluation via comma).
/// Stack convention: `[..., addr, value]` → SET → `[...]`
///
/// `Cell::Array` values are rejected for both `DictAddr` and `StackAddr` destinations.
/// DIM local array initialisation uses the hidden `ARRAY_STORE_LOCAL` primitive instead.
pub fn set_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            reject_array_value(&value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            reject_array_value(&value)?;
            vm.local_write(a, value)?;
            Ok(())
        }
        Cell::ArrayAddr { pool_idx, elem_idx } => {
            write_array_element(vm, pool_idx, elem_idx, value)
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    // --- ARRAY_ADDR / STORE / FETCH boundary tests ---

    /// Verify the full ARRAY_ADDR → STORE → FETCH roundtrip for an array element.
    ///
    /// ARRAY_ADDR computes a Cell::ArrayAddr; STORE writes through it; FETCH reads
    /// the value back.  After the roundtrip the stored value must equal what was
    /// pushed before STORE.
    #[test]
    fn test_array_addr_store_fetch_roundtrip() {
        use crate::primitives::arrays::array_addr_prim;

        let mut vm = VM::new();

        // Create a one-element array with initial value Cell::None.
        let pool_idx = vm.arrays.len();
        vm.arrays
            .push(crate::array_ref::ArrayRef::new(vec![Cell::None]));

        // Push array handle and 1-based index, then call ARRAY_ADDR.
        vm.push(Cell::Array(vm.arrays[pool_idx].clone())).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        array_addr_prim(&mut vm).unwrap();

        // Stack now has the Cell::ArrayAddr on top.
        // Push value to store, then addr — STORE convention: [value, addr].
        let new_value = Cell::Int(42);
        vm.push(new_value.clone()).unwrap();
        // Move the addr below the value (swap).
        let addr = vm.pop().unwrap(); // pop new_value temporarily
        let arr_addr = vm.pop().unwrap(); // pop the ArrayAddr
        vm.push(addr).unwrap(); // push new_value back
        vm.push(arr_addr).unwrap(); // push ArrayAddr on top

        store_prim(&mut vm).unwrap();

        // Verify the array element was updated.
        assert_eq!(vm.arrays[pool_idx].get_cloned(0), Some(new_value.clone()));

        // Now FETCH: push the ArrayAddr again and call fetch_prim.
        vm.push(Cell::ArrayAddr {
            pool_idx,
            elem_idx: 0,
        })
        .unwrap();
        fetch_prim(&mut vm).unwrap();

        assert_eq!(vm.pop().unwrap(), new_value);
    }

    /// Verify that STORE rejects a nested Cell::Array written to an array element
    /// (i.e., when the destination address is a Cell::ArrayAddr).
    #[test]
    fn test_store_array_element_rejects_nested_array() {
        let mut vm = VM::new();

        // Outer array — the target element.
        let outer_idx = vm.arrays.len();
        vm.arrays
            .push(crate::array_ref::ArrayRef::new(vec![Cell::None]));

        // Inner array — the value to be (illegally) stored.
        let inner_ar = crate::array_ref::ArrayRef::new(vec![Cell::Int(1)]);
        vm.arrays.push(inner_ar.clone());

        // STORE convention: push value then addr.
        vm.push(Cell::Array(inner_ar)).unwrap();
        vm.push(Cell::ArrayAddr {
            pool_idx: outer_idx,
            elem_idx: 0,
        })
        .unwrap();

        let result = store_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidArrayElement { .. })),
            "expected InvalidArrayElement, got {:?}",
            result
        );
    }

    /// Verify that SET rejects a nested Cell::Array written to an array element.
    #[test]
    fn test_set_array_element_rejects_nested_array() {
        let mut vm = VM::new();

        // Target array.
        let outer_idx = vm.arrays.len();
        vm.arrays
            .push(crate::array_ref::ArrayRef::new(vec![Cell::None]));

        // Value array (nested).
        let inner_ar = crate::array_ref::ArrayRef::new(vec![Cell::Int(1)]);
        vm.arrays.push(inner_ar.clone());

        // SET convention: push addr then value.
        vm.push(Cell::ArrayAddr {
            pool_idx: outer_idx,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::Array(inner_ar)).unwrap();

        let result = set_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidArrayElement { .. })),
            "expected InvalidArrayElement, got {:?}",
            result
        );
    }

    /// Verify that STORE rejects a Cell::Array written to a DictAddr slot.
    ///
    /// Surface-level STORE must never accept a whole-array handle for a scalar slot.
    #[test]
    fn test_store_array_to_dict_addr_is_type_error() {
        let mut vm = VM::new();

        vm.dictionary.push(Cell::None);
        vm.dp = 1;

        let ar = crate::array_ref::ArrayRef::new(vec![Cell::Int(7)]);
        vm.arrays.push(ar.clone());

        // STORE convention: push value then addr.
        vm.push(Cell::Array(ar)).unwrap();
        vm.push(Cell::DictAddr(0)).unwrap();

        let result = store_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { got: "Array", .. })),
            "expected TypeError(Array), got {:?}",
            result
        );
    }

    /// Verify that STORE rejects a Cell::Array written to a StackAddr slot.
    ///
    /// Surface-level STORE must never accept a whole-array handle for a local slot.
    #[test]
    fn test_store_array_to_stack_addr_is_type_error() {
        let mut vm = VM::new();

        // Allocate a local slot via the data stack.
        vm.data_stack.push(Cell::None);
        vm.bp = 0;

        let ar = crate::array_ref::ArrayRef::new(vec![Cell::Int(3)]);
        vm.arrays.push(ar.clone());

        // STORE convention: push value then addr.
        vm.push(Cell::Array(ar)).unwrap();
        vm.push(Cell::StackAddr(0)).unwrap();

        let result = store_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { got: "Array", .. })),
            "expected TypeError(Array), got {:?}",
            result
        );
    }

    /// Verify that SET rejects a Cell::Array written to a DictAddr slot.
    #[test]
    fn test_set_array_to_dict_addr_is_type_error() {
        let mut vm = VM::new();

        vm.dictionary.push(Cell::None);
        vm.dp = 1;

        let ar = crate::array_ref::ArrayRef::new(vec![Cell::Int(5)]);
        vm.arrays.push(ar.clone());

        // SET convention: push addr then value.
        vm.push(Cell::DictAddr(0)).unwrap();
        vm.push(Cell::Array(ar)).unwrap();

        let result = set_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { got: "Array", .. })),
            "expected TypeError(Array), got {:?}",
            result
        );
    }

    /// Verify that SET rejects a Cell::Array written to a StackAddr slot.
    #[test]
    fn test_set_array_to_stack_addr_is_type_error() {
        let mut vm = VM::new();

        vm.data_stack.push(Cell::None);
        vm.bp = 0;

        let ar = crate::array_ref::ArrayRef::new(vec![Cell::Int(2)]);
        vm.arrays.push(ar.clone());

        // SET convention: push addr then value.
        vm.push(Cell::StackAddr(0)).unwrap();
        vm.push(Cell::Array(ar)).unwrap();

        let result = set_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { got: "Array", .. })),
            "expected TypeError(Array), got {:?}",
            result
        );
    }
}
