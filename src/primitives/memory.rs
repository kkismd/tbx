use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// Extract the array pool index from a `Cell::Array`.
///
/// Returns `None` for any cell that is not `Cell::Array`.  `Cell::Str` is
/// `Rc<str>`-backed and has no VM-managed pool index.
fn array_pool_idx_from_cell(cell: &Cell) -> Option<usize> {
    match cell {
        Cell::Array(idx) => Some(*idx),
        _ => None,
    }
}

fn is_global_array_pool_idx(vm: &VM, pool_idx: usize) -> bool {
    pool_idx < vm.global_array_pool_len
}

fn promote_array_pool_idx_to_global(vm: &mut VM, pool_idx: usize) {
    vm.global_array_pool_len = vm.global_array_pool_len.max(pool_idx + 1);
}

fn check_dict_reference_write(vm: &mut VM, value: &Cell) -> Result<(), TbxError> {
    let Some(pool_idx) = array_pool_idx_from_cell(value) else {
        return Ok(());
    };

    if is_global_array_pool_idx(vm, pool_idx) {
        return Ok(());
    }

    if vm.is_executing_top_level() {
        promote_array_pool_idx_to_global(vm, pool_idx);
        return Ok(());
    }

    Err(TbxError::ArrayFrameEscape)
}

/// Write `value` to element `elem_idx` of the array at `pool_idx`.
fn write_array_element(
    vm: &mut VM,
    pool_idx: usize,
    elem_idx: usize,
    value: Cell,
) -> Result<(), TbxError> {
    // Validate before get_mut() to avoid borrow conflicts.
    super::arrays::check_array_element_value(&value)?;
    let pool_size = vm.arrays.len();
    let arr = vm
        .arrays
        .get_mut(pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: pool_idx,
            size: pool_size,
        })?;
    let size = arr.len();
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx as i64,
            size,
        });
    }
    arr[elem_idx] = value;
    Ok(())
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
            let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
                index: pool_idx,
                size: vm.arrays.len(),
            })?;
            let size = arr.len();
            if elem_idx >= size {
                return Err(TbxError::ArrayIndexOutOfBounds {
                    index: elem_idx as i64,
                    size,
                });
            }
            let value = arr[elem_idx].clone();
            vm.push(value)?;
            Ok(())
        }
        _ => Err(TbxError::TypeError {
            expected: "address",
            got: "non-address",
        }),
    }
}

/// Check that `value` is not a whole-array handle being written to a scalar variable.
///
/// `Cell::Array` is forbidden as a value in scalar variable writes (`DictAddr` or
/// `StackAddr` destinations).  Arrays are not first-class values on the surface
/// language; only element-level access via `Cell::ArrayAddr` is permitted.
fn check_scalar_write_value(value: &Cell) -> Result<(), TbxError> {
    if matches!(value, Cell::Array(_)) {
        return Err(TbxError::TypeError {
            expected: "non-array value for scalar variable write",
            got: "Array",
        });
    }
    Ok(())
}

/// STORE — pop addr (top) then value (below), and store value at addr.
///
/// Stack convention: `[..., value, addr]` → STORE → `[...]`
///
/// Storing a whole `Cell::Array` into a scalar variable (`DictAddr` or `StackAddr`)
/// is rejected with a TypeError; element-level writes via `Cell::ArrayAddr` are allowed.
pub fn store_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    let value = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            check_scalar_write_value(&value)?;
            check_dict_reference_write(vm, &value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            check_scalar_write_value(&value)?;
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
/// Storing a whole `Cell::Array` into a scalar variable (`DictAddr` or `StackAddr`)
/// is rejected with a TypeError; element-level writes via `Cell::ArrayAddr` are allowed.
pub fn set_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            check_scalar_write_value(&value)?;
            check_dict_reference_write(vm, &value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
            check_scalar_write_value(&value)?;
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
    use crate::cell::{Cell, ReturnFrame, Xt};

    #[test]
    fn test_array_pool_idx_from_cell_array_only() {
        // array_pool_idx_from_cell returns Some only for Cell::Array;
        // Cell::Str is Rc<str>-backed and has no array pool index.
        assert_eq!(array_pool_idx_from_cell(&Cell::Array(3)), Some(3));
        assert_eq!(array_pool_idx_from_cell(&Cell::string("hello")), None);
        assert_eq!(
            array_pool_idx_from_cell(&Cell::ArrayAddr {
                pool_idx: 1,
                elem_idx: 2,
            }),
            None
        );
        assert_eq!(array_pool_idx_from_cell(&Cell::DictAddr(0)), None);
        assert_eq!(array_pool_idx_from_cell(&Cell::StackAddr(0)), None);
    }

    #[test]
    fn test_promote_array_pool_idx_to_global_never_moves_boundary_backward() {
        let mut vm = VM::new();

        vm.global_array_pool_len = 5;
        promote_array_pool_idx_to_global(&mut vm, 1);
        assert_eq!(vm.global_array_pool_len, 5);
        promote_array_pool_idx_to_global(&mut vm, 7);
        assert_eq!(vm.global_array_pool_len, 8);
    }

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
        vm.arrays.push(vec![Cell::None]);

        // Push array handle and 1-based index, then call ARRAY_ADDR.
        vm.push(Cell::Array(pool_idx)).unwrap();
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
        assert_eq!(vm.arrays[pool_idx][0], new_value);

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
        vm.arrays.push(vec![Cell::None]);

        // Inner array — the value to be (illegally) stored.
        let inner_idx = vm.arrays.len();
        vm.arrays.push(vec![Cell::Int(1)]);

        // STORE convention: push value then addr.
        vm.push(Cell::Array(inner_idx)).unwrap();
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
        vm.arrays.push(vec![Cell::None]);

        // Value array (nested).
        let inner_idx = vm.arrays.len();
        vm.arrays.push(vec![Cell::Int(1)]);

        // SET convention: push addr then value.
        vm.push(Cell::ArrayAddr {
            pool_idx: outer_idx,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(Cell::Array(inner_idx)).unwrap();

        let result = set_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::InvalidArrayElement { .. })),
            "expected InvalidArrayElement, got {:?}",
            result
        );
    }

    /// Verify that storing a frame-local Cell::Array into a dictionary slot raises
    /// ArrayFrameEscape.
    ///
    /// A frame-local array is one whose pool index is >= global_array_pool_len and
    /// the VM is currently executing inside a Call frame (not top-level).
    #[test]
    fn test_store_frame_local_array_to_dict_errors() {
        // Storing a whole Cell::Array into a DictAddr slot is now always rejected
        // with TypeError, regardless of frame context.  (Previously it was
        // ArrayFrameEscape for frame-local arrays; the earlier check was
        // superseded by the surface-level prohibition on whole-array writes.)
        let mut vm = VM::new();

        // Allocate a dictionary slot for the target variable.
        vm.dictionary.push(Cell::None);
        vm.dp = 1;

        let frame_local_idx = vm.arrays.len();
        vm.arrays.push(vec![Cell::Int(7)]);

        // Simulate a Call frame so is_executing_top_level() returns false.
        vm.return_stack.push(ReturnFrame::TopLevel);
        vm.return_stack.push(ReturnFrame::Call {
            callee_xt: Xt(0),
            return_pc: 0,
            saved_bp: 0,
            saved_array_pool_len: 0,
            actual_arity: 0,
        });

        // STORE convention: push value then addr.
        vm.push(Cell::Array(frame_local_idx)).unwrap();
        vm.push(Cell::DictAddr(0)).unwrap();

        let result = store_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { .. })),
            "expected TypeError for whole-array write, got {:?}",
            result
        );
    }

    /// Verify that storing a top-level Cell::Array into a dictionary slot is now
    /// rejected with TypeError.
    ///
    /// Arrays are not first-class surface values; storing a whole array handle
    /// into a scalar variable is forbidden regardless of the execution context.
    #[test]
    fn test_store_top_level_array_to_dict_promotes_global_boundary() {
        let mut vm = VM::new();

        // Allocate a dictionary slot for the target variable.
        vm.dictionary.push(Cell::None);
        vm.dp = 1;

        let top_level_idx = vm.arrays.len();
        vm.arrays.push(vec![Cell::Int(99)]);

        // Simulate top-level execution: only a TopLevel sentinel on the return stack.
        vm.return_stack.push(ReturnFrame::TopLevel);

        // STORE convention: push value then addr.
        vm.push(Cell::Array(top_level_idx)).unwrap();
        vm.push(Cell::DictAddr(0)).unwrap();

        let result = store_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::TypeError { .. })),
            "expected TypeError for whole-array write at top level, got {:?}",
            result
        );
    }
}
