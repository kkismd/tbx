use crate::array_ref::ArrayRef;
use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// Check that `value` is a permitted array element value.
///
/// `Cell::Array` is always rejected (nested arrays are not supported).
/// `Cell::Str(Rc<str>)` is allowed: the `Rc` handle keeps the string alive
/// independently of any stack frame, so there is no dangling risk (#591).
/// All other scalar types are accepted unconditionally.
pub(super) fn check_array_element_value(value: &Cell) -> Result<(), TbxError> {
    match value {
        Cell::Array(_) => Err(TbxError::InvalidArrayElement { got: "Array" }),
        _ => Ok(()),
    }
}

/// Internal primitive used by the `DIM @A[n]` compiler to allocate an array.
///
/// Pops `Cell::Int(n)` (n > 0) from the stack, creates an `ArrayRef` with `n`
/// `Cell::None` slots, registers it in `vm.arrays` for `ArrayAddr` compatibility,
/// and pushes `Cell::Array(ArrayRef)` as the handle.
///
/// This function is NOT registered as a user-facing surface primitive.
/// It is registered under a hidden system entry so that `dim_prim` can emit its
/// Xt into the compiled word body at compile time.
pub(super) fn array_prim(vm: &mut VM) -> Result<(), TbxError> {
    let n = vm.pop_int()?;
    if n <= 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("ARRAY size must be positive, got {n}"),
        });
    }
    let size = n as usize;
    let ar = ArrayRef::new(vec![Cell::None; size]);
    // Register in vm.arrays so that Cell::ArrayAddr can locate this array by
    // pool_idx.  This will be removed when ArrayAddr is migrated to
    // (ArrayRef, elem_idx) in a follow-up issue.
    vm.arrays.push(ar.clone());
    vm.push(Cell::Array(ar))?;
    Ok(())
}

/// TUPLE — collect N values from the stack into a new immutable tuple.
///
/// Pops the arity `n` from the stack, then pops `n` values and assembles them
/// into a `Cell::Tuple`.  Elements are validated by `Cell::new_tuple`; nested
/// `Tuple`, `Array`, `ArrayAddr`, `Xt`, `None`, and `Marker` are rejected.
///
/// `TUPLE()` with zero arguments produces an empty tuple `()`.
pub fn to_tuple_prim(vm: &mut VM) -> Result<(), TbxError> {
    // Pop the arity pushed by the compiler.
    let n = vm.pop_int()?;
    if n < 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("TUPLE arity must be non-negative, got {n}"),
        });
    }
    let count = n as usize;
    // Pop `count` values in reverse order, then reverse to restore original order.
    let mut elems: Vec<Cell> = Vec::with_capacity(count);
    for _ in 0..count {
        let elem = vm.pop()?;
        elems.push(elem);
    }
    elems.reverse();
    // Cell::new_tuple validates element types and returns an error for forbidden types.
    let tuple = Cell::new_tuple(elems)?;
    vm.push(tuple)?;
    Ok(())
}

/// ARRAY_GET — read an element from an array.
///
/// Stack: `[..., Cell::Array(ArrayRef), Cell::Int(elem_idx)]` → `value`
///
/// This is the VM-level primitive that `@A[i]` compiles to.  The expression
/// compiler lowers `@A[i]` to: `<array handle read>  <index expr>  ARRAY_GET`.
///
/// Array indices are 1-based from the user's perspective: valid range is `1..=N`.
/// The index is translated to 0-based internally before accessing the storage.
pub fn array_get_prim(vm: &mut VM) -> Result<(), TbxError> {
    let elem_idx_raw = vm.pop_int()?;
    let ar = match vm.pop()? {
        Cell::Array(ar) => ar,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let size = ar.len();
    // Translate 1-based user index to 0-based internal index.
    // Index 0 or negative is out of bounds.
    if elem_idx_raw < 1 {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let elem_idx = (elem_idx_raw - 1) as usize;
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let value = ar
        .get_cloned(elem_idx)
        .ok_or(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        })?;
    vm.push(value)?;
    Ok(())
}

/// ARRAY_ADDR — compute the address of an array element.
///
/// Stack: `[..., Cell::Array(ArrayRef), Cell::Int(elem_idx)]` → `Cell::ArrayAddr { pool_idx, elem_idx }`
///
/// This is the VM-level primitive that `&@A[i]` compiles to.  The expression
/// compiler lowers `&@A[i]` to: `<array handle read>  <index expr>  ARRAY_ADDR`.
/// The resulting `Cell::ArrayAddr` is used by `SET` (via `STORE`) to write a value
/// to the addressed element.
///
/// Array indices are 1-based from the user's perspective: valid range is `1..=N`.
/// The index is translated to 0-based internally before storing in `Cell::ArrayAddr`.
///
/// NOTE: `Cell::ArrayAddr` still uses `pool_idx` to locate the array in
/// `VM::arrays`.  The `ArrayRef` is used only for bounds checking here.
/// When `ArrayAddr` is migrated to `(ArrayRef, elem_idx)` in a follow-up issue,
/// the `pool_idx` lookup will be removed.
pub fn array_addr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let elem_idx_raw = vm.pop_int()?;
    let ar = match vm.pop()? {
        Cell::Array(ar) => ar,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let size = ar.len();
    // Translate 1-based user index to 0-based internal index.
    // Index 0 or negative is out of bounds.
    if elem_idx_raw < 1 {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let elem_idx = (elem_idx_raw - 1) as usize;
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    // Locate the pool_idx by finding the matching ArrayRef in vm.arrays.
    // This lookup is O(n) but arrays are small and this path is only used for
    // &@A[i] address-of operations.  The pool_idx lookup will be eliminated
    // when ArrayAddr is migrated to (ArrayRef, elem_idx).
    let pool_idx =
        vm.arrays
            .iter()
            .position(|entry| entry.ptr_eq(&ar))
            .ok_or(TbxError::InvalidArgument {
                message: "ARRAY_ADDR: array not found in pool (possible use-after-free)"
                    .to_string(),
            })?;
    vm.push(Cell::ArrayAddr { pool_idx, elem_idx })?;
    Ok(())
}

/// TUPLE_GET — read an element from a tuple (1-based index).
///
/// Stack: `[..., Cell::Tuple(elements), Cell::Int(elem_idx)]` → `value`
///
/// Tuple indices are 1-based from the user's perspective: valid range is `1..=N`.
pub fn tuple_get_prim(vm: &mut VM) -> Result<(), TbxError> {
    // Pop index.
    let idx_raw = vm.pop()?;
    let elem_idx_raw = match idx_raw {
        Cell::Int(n) => n,
        other => {
            return Err(TbxError::TypeError {
                expected: "Int",
                got: other.type_name(),
            })
        }
    };
    // Pop tuple.
    let elements = match vm.pop()? {
        Cell::Tuple(elems) => elems,
        other => {
            return Err(TbxError::TypeError {
                expected: "Tuple",
                got: other.type_name(),
            })
        }
    };
    let size = elements.len();
    if elem_idx_raw < 1 {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let elem_idx = (elem_idx_raw - 1) as usize;
    if elem_idx >= size {
        return Err(TbxError::ArrayIndexOutOfBounds {
            index: elem_idx_raw,
            size,
        });
    }
    let value = elements[elem_idx].clone();
    vm.push(value)?;
    Ok(())
}

/// TUPLE_LEN — return the number of elements in a tuple.
///
/// Pops `Cell::Tuple(elems)` from the stack and pushes the element count as `Cell::Int`.
/// Returns `TbxError::TypeError` if the top of the stack is not a `Cell::Tuple`.
///
/// Stack: `[..., Cell::Tuple(elems)]` → `Cell::Int(len)`
pub fn tuple_len_prim(vm: &mut VM) -> Result<(), TbxError> {
    match vm.pop()? {
        Cell::Tuple(elems) => {
            vm.push(Cell::Int(elems.len() as i64))?;
            Ok(())
        }
        other => Err(TbxError::TypeError {
            expected: "Tuple",
            got: other.type_name(),
        }),
    }
}

/// ARRAY_LEN — return the length of an array.
///
/// Pops `Cell::Array(ArrayRef)` from the stack and pushes the number of elements
/// as `Cell::Int`.
///
/// Stack: `[..., Cell::Array(ArrayRef)]` → `Cell::Int(len)`
pub fn array_len_prim(vm: &mut VM) -> Result<(), TbxError> {
    let ar = match vm.pop()? {
        Cell::Array(ar) => ar,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let len = ar.len() as i64;
    vm.push(Cell::Int(len))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- tuple_len_prim ---

    #[test]
    fn test_tuple_len_prim_empty() {
        // An empty tuple must return 0.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![]).unwrap();
        vm.push(tuple).unwrap();
        tuple_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(0)));
    }

    #[test]
    fn test_tuple_len_prim_one_element() {
        // A one-element tuple must return 1.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![Cell::Int(42)]).unwrap();
        vm.push(tuple).unwrap();
        tuple_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(1)));
    }

    #[test]
    fn test_tuple_len_prim_multi_element() {
        // A three-element tuple must return 3.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]).unwrap();
        vm.push(tuple).unwrap();
        tuple_len_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(3)));
    }

    #[test]
    fn test_tuple_len_prim_non_tuple_returns_type_error() {
        // A non-Tuple value must produce a TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(99)).unwrap();
        assert!(matches!(
            tuple_len_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Tuple",
                ..
            })
        ));
    }

    // --- tuple_get_prim ---

    #[test]
    fn test_tuple_get_prim_basic() {
        // User index 2 maps to internal index 1.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![Cell::Int(10), Cell::Int(20), Cell::Int(30)]).unwrap();
        vm.push(tuple).unwrap();
        vm.push(Cell::Int(2)).unwrap();
        tuple_get_prim(&mut vm).unwrap();
        assert_eq!(vm.pop(), Ok(Cell::Int(20)));
    }

    #[test]
    fn test_tuple_get_prim_index_out_of_bounds_zero() {
        // Index 0 is invalid in 1-based indexing.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![Cell::Int(1), Cell::Int(2)]).unwrap();
        vm.push(tuple).unwrap();
        vm.push(Cell::Int(0)).unwrap();
        assert!(matches!(
            tuple_get_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: 0, .. })
        ));
    }

    #[test]
    fn test_tuple_get_prim_index_out_of_bounds_high() {
        // Index 4 is out of range for a 3-element tuple.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]).unwrap();
        vm.push(tuple).unwrap();
        vm.push(Cell::Int(4)).unwrap();
        assert!(matches!(
            tuple_get_prim(&mut vm),
            Err(TbxError::ArrayIndexOutOfBounds { index: 4, size: 3 })
        ));
    }

    #[test]
    fn test_tuple_get_prim_wrong_index_type() {
        // Float index must produce a TypeError.
        let mut vm = VM::new();
        let tuple = Cell::new_tuple(vec![Cell::Int(1)]).unwrap();
        vm.push(tuple).unwrap();
        vm.push(Cell::Float(1.0)).unwrap();
        assert!(matches!(
            tuple_get_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Int",
                ..
            })
        ));
    }

    #[test]
    fn test_tuple_get_prim_wrong_target_type() {
        // Using a non-tuple value as the target must produce a TypeError.
        let mut vm = VM::new();
        vm.push(Cell::Int(99)).unwrap();
        vm.push(Cell::Int(1)).unwrap();
        assert!(matches!(
            tuple_get_prim(&mut vm),
            Err(TbxError::TypeError {
                expected: "Tuple",
                ..
            })
        ));
    }
}
