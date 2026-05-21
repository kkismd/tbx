use crate::array_ref::ArrayRef;
use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// Return the `ArrayRef` if `cell` is `Cell::Array`, otherwise `None`.
///
/// This helper is used by STORE / SET to detect when an array handle is being
/// written to a dictionary slot, so the frame-escape check can be applied.
fn array_ref_from_cell(cell: &Cell) -> Option<&ArrayRef> {
    match cell {
        Cell::Array(ar) => Some(ar),
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
    let Some(ar) = array_ref_from_cell(value) else {
        return Ok(());
    };

    // Find the pool_idx for this ArrayRef.  If it is not registered in the
    // pool at all, treat it as non-global (conservative: will return Err below
    // if inside a call frame).
    let pool_idx = vm.arrays.iter().position(|entry| entry.ptr_eq(ar));

    if let Some(idx) = pool_idx {
        if is_global_array_pool_idx(vm, idx) {
            return Ok(());
        }
    }

    if vm.is_executing_top_level() {
        // Promote to global if possible.
        if let Some(idx) = pool_idx {
            promote_array_pool_idx_to_global(vm, idx);
        }
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
    // Validate before borrowing to avoid borrow conflicts.
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
    ar.set(elem_idx, value)?;
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
            let pool_size = vm.arrays.len();
            let ar = vm
                .arrays
                .get(pool_idx)
                .ok_or(TbxError::IndexOutOfBounds {
                    index: pool_idx,
                    size: pool_size,
                })?
                .clone();
            let size = ar.len();
            if elem_idx >= size {
                return Err(TbxError::ArrayIndexOutOfBounds {
                    index: elem_idx as i64,
                    size,
                });
            }
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
pub fn store_prim(vm: &mut VM) -> Result<(), TbxError> {
    let addr = vm.pop()?;
    let value = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            check_dict_reference_write(vm, &value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
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
pub fn set_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    let addr = vm.pop()?;
    match addr {
        Cell::DictAddr(a) => {
            check_dict_reference_write(vm, &value)?;
            vm.dict_write_at(a, value)?;
            Ok(())
        }
        Cell::StackAddr(a) => {
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
    use crate::array_ref::ArrayRef;
    use crate::cell::{Cell, ReturnFrame, Xt};

    /// Helper to create a one-element pool entry and push Cell::Array for tests
    /// that still exercise the pool-index path (ARRAY_ADDR / STORE / FETCH).
    fn push_pooled_array(vm: &mut VM, elems: Vec<Cell>) -> usize {
        let pool_idx = vm.arrays.len();
        let ar = ArrayRef::new(elems);
        vm.arrays.push(ar.clone());
        vm.push(Cell::Array(ar)).unwrap();
        pool_idx
    }

    #[test]
    fn test_array_ref_from_cell_array_only() {
        // array_ref_from_cell returns Some only for Cell::Array.
        let ar = ArrayRef::new(vec![Cell::Int(3)]);
        assert!(array_ref_from_cell(&Cell::Array(ar)).is_some());
        assert!(array_ref_from_cell(&Cell::string("hello")).is_none());
        assert!(array_ref_from_cell(&Cell::DictAddr(0)).is_none());
        assert!(array_ref_from_cell(&Cell::StackAddr(0)).is_none());
        assert!(array_ref_from_cell(&Cell::ArrayAddr {
            pool_idx: 1,
            elem_idx: 2
        })
        .is_none());
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
        let pool_idx = push_pooled_array(&mut vm, vec![Cell::None]);

        // Push 1-based index, then call ARRAY_ADDR.
        // Note: push_pooled_array already pushed Cell::Array on the stack.
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

        // Verify the array element was updated via the pool entry.
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
        let outer_ar = ArrayRef::new(vec![Cell::None]);
        vm.arrays.push(outer_ar);

        // Inner array — the value to be (illegally) stored.
        let inner_ar = ArrayRef::new(vec![Cell::Int(1)]);
        let inner_cell = Cell::Array(inner_ar);

        // STORE convention: push value then addr.
        vm.push(inner_cell).unwrap();
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
        let outer_ar = ArrayRef::new(vec![Cell::None]);
        vm.arrays.push(outer_ar);

        // Value array (nested).
        let inner_ar = ArrayRef::new(vec![Cell::Int(1)]);
        let inner_cell = Cell::Array(inner_ar);

        // SET convention: push addr then value.
        vm.push(Cell::ArrayAddr {
            pool_idx: outer_idx,
            elem_idx: 0,
        })
        .unwrap();
        vm.push(inner_cell).unwrap();

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
        let mut vm = VM::new();

        // Allocate a dictionary slot for the target variable.
        vm.dictionary.push(Cell::None);
        vm.dp = 1;

        // Create the frame-local array; global_array_pool_len stays at 0.
        let ar = ArrayRef::new(vec![Cell::Int(7)]);
        vm.arrays.push(ar.clone());
        // global_array_pool_len = 0 means pool_idx 0 is not global.

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
        vm.push(Cell::Array(ar)).unwrap();
        vm.push(Cell::DictAddr(0)).unwrap();

        let result = store_prim(&mut vm);
        assert!(
            matches!(result, Err(TbxError::ArrayFrameEscape)),
            "expected ArrayFrameEscape, got {:?}",
            result
        );
    }

    /// Verify that storing a top-level Cell::Array into a dictionary slot promotes
    /// global_array_pool_len to cover the stored array.
    ///
    /// When execution is at the top level (only a TopLevel sentinel on the return
    /// stack), STORE must promote the array to the global region instead of
    /// returning an error.
    #[test]
    fn test_store_top_level_array_to_dict_promotes_global_boundary() {
        let mut vm = VM::new();

        // Allocate a dictionary slot for the target variable.
        vm.dictionary.push(Cell::None);
        vm.dp = 1;

        // Create the array at top level; global_array_pool_len starts at 0.
        let top_level_ar = ArrayRef::new(vec![Cell::Int(99)]);
        let top_level_idx = vm.arrays.len();
        vm.arrays.push(top_level_ar.clone());

        // Simulate top-level execution: only a TopLevel sentinel on the return stack.
        vm.return_stack.push(ReturnFrame::TopLevel);

        // STORE convention: push value then addr.
        let arr_cell = Cell::Array(top_level_ar);
        vm.push(arr_cell.clone()).unwrap();
        vm.push(Cell::DictAddr(0)).unwrap();

        store_prim(&mut vm).unwrap();

        // The global boundary must have been promoted to cover top_level_idx.
        assert!(
            vm.global_array_pool_len > top_level_idx,
            "expected global_array_pool_len > {}, got {}",
            top_level_idx,
            vm.global_array_pool_len
        );
        // The dictionary slot must now hold the array handle.
        // Use ptr_eq to compare since ArrayRef equality is pointer-based.
        let stored = vm.dict_read(0).unwrap();
        if let (Cell::Array(stored_ar), Cell::Array(orig_ar)) = (&stored, &arr_cell) {
            assert!(
                stored_ar.ptr_eq(orig_ar),
                "stored ArrayRef must be the same Rc"
            );
        } else {
            panic!("expected Cell::Array in dictionary slot, got {:?}", stored);
        }
    }
}
