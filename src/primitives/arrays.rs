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

/// TO_ARRAY — collect N values from the stack into a new array.
///
/// The returned `Cell::Array` is bound to the current frame and must not escape.
/// TO_ARRAY with zero arguments (`TO_ARRAY()`) produces an empty array.
pub fn to_array_prim(vm: &mut VM) -> Result<(), TbxError> {
    // Pop the arity pushed by the compiler.
    let n = vm.pop_int()?;
    if n < 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("TO_ARRAY arity must be non-negative, got {n}"),
        });
    }
    let count = n as usize;
    // Pop `count` values in reverse order, then reverse to restore original order.
    let mut elems: Vec<Cell> = Vec::with_capacity(count);
    for _ in 0..count {
        let elem = vm.pop()?;
        // Reject nested arrays; Cell::Str(Rc<str>) is now allowed (#591).
        check_array_element_value(&elem)?;
        elems.push(elem);
    }
    elems.reverse();
    let pool_idx = vm.arrays.len();
    vm.arrays.push(elems);
    vm.push(Cell::Array(pool_idx))?;
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

/// FROM_ARRAY — expand an array onto the stack.
///
/// Pops `Cell::Array(pool_idx)` from the stack and pushes every element of the
/// array onto the stack in order (index 0 first).
///
/// Stack before call: `[Cell::Array(pool_idx)]`
/// Stack after call:  `[elem0, elem1, ..., elem(n-1)]`
pub fn from_array_prim(vm: &mut VM) -> Result<(), TbxError> {
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let elems = vm
        .arrays
        .get(pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: pool_idx,
            size: vm.arrays.len(),
        })?
        .clone();
    for elem in elems {
        vm.push(elem)?;
    }
    Ok(())
}

/// ARRAY — create an array of N elements and push its handle onto the stack.
///
/// Pops `Cell::Int(n)` from the stack (n > 0), pushes `n` `Cell::None` elements
/// into `vm.arrays`, and pushes `Cell::Array(pool_idx)` as the handle.
///
/// Arrays created inside a word are bound to that stack frame and freed automatically
/// when the owning word returns (EXIT/RETURN_VAL truncates the pool).
pub fn array_prim(vm: &mut VM) -> Result<(), TbxError> {
    let n = vm.pop_int()?;
    if n <= 0 {
        return Err(TbxError::InvalidArgument {
            message: format!("ARRAY size must be positive, got {n}"),
        });
    }
    let size = n as usize;
    let idx = vm.arrays.len();
    vm.arrays.push(vec![Cell::None; size]);
    vm.push(Cell::Array(idx))?;
    Ok(())
}

/// ARRAY_GET — read an element from an array.
///
/// Stack: `[..., Cell::Array(pool_idx), Cell::Int(elem_idx)]` → `value`
///
/// Array indices are 1-based from the user's perspective: valid range is `1..=N`.
/// The index is translated to 0-based internally before accessing the Vec.
pub fn array_get_prim(vm: &mut VM) -> Result<(), TbxError> {
    let elem_idx_raw = vm.pop_int()?;
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
        index: pool_idx,
        size: vm.arrays.len(),
    })?;
    let size = arr.len();
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
    let value = arr[elem_idx].clone();
    vm.push(value)?;
    Ok(())
}

/// ARRAY_ADDR — compute the address of an array element.
///
/// Stack: `[..., Cell::Array(pool_idx), Cell::Int(elem_idx)]` → `Cell::ArrayAddr { pool_idx, elem_idx }`
///
/// Array indices are 1-based from the user's perspective: valid range is `1..=N`.
/// The index is translated to 0-based internally before storing in `Cell::ArrayAddr`.
pub fn array_addr_prim(vm: &mut VM) -> Result<(), TbxError> {
    let elem_idx_raw = vm.pop_int()?;
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    // Validate bounds at address-computation time.
    let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
        index: pool_idx,
        size: vm.arrays.len(),
    })?;
    let size = arr.len();
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
    vm.push(Cell::ArrayAddr { pool_idx, elem_idx })?;
    Ok(())
}

/// ARRAY_LEN — return the length of an array.
///
/// Pops `Cell::Array(pool_idx)` from the stack and pushes the number of elements
/// as `Cell::Int`.
///
/// Stack: `[..., Cell::Array(pool_idx)]` → `Cell::Int(len)`
pub fn array_len_prim(vm: &mut VM) -> Result<(), TbxError> {
    let pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let arr = vm.arrays.get(pool_idx).ok_or(TbxError::IndexOutOfBounds {
        index: pool_idx,
        size: vm.arrays.len(),
    })?;
    let len = arr.len() as i64;
    vm.push(Cell::Int(len))?;
    Ok(())
}

/// ARRAY_CONCAT — concatenate two arrays and return a new array.
///
/// Pops two `Cell::Array` handles from the stack and pushes a new array whose
/// contents are all elements of `a` followed by all elements of `b`.
///
/// Stack: `[..., a: Array, b: Array]` → `Cell::Array(new_idx)`
pub fn array_concat_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b_pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };
    let a_pool_idx = match vm.pop()? {
        Cell::Array(idx) => idx,
        other => {
            return Err(TbxError::TypeError {
                expected: "Array",
                got: other.type_name(),
            })
        }
    };

    let a = vm
        .arrays
        .get(a_pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: a_pool_idx,
            size: vm.arrays.len(),
        })?;
    let b = vm
        .arrays
        .get(b_pool_idx)
        .ok_or(TbxError::IndexOutOfBounds {
            index: b_pool_idx,
            size: vm.arrays.len(),
        })?;

    let mut result: Vec<Cell> = Vec::with_capacity(a.len() + b.len());
    result.extend_from_slice(a);
    result.extend_from_slice(b);

    let pool_idx = vm.arrays.len();
    vm.arrays.push(result);
    vm.push(Cell::Array(pool_idx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    /// Verify that TO_ARRAY accepts Cell::Str(Rc<str>) as an array element.
    ///
    /// Cell::Str is Rc<str>-backed (#591); storing a string in an array element
    /// is allowed because the Rc handle keeps the string alive independently of
    /// any stack frame.  This test confirms that to_array_prim does not reject
    /// Cell::Str values with InvalidArrayElement.
    #[test]
    fn test_to_array_accepts_str_elements() {
        let mut vm = VM::new();

        // Push two string elements followed by the arity.
        vm.push(Cell::string("hello")).unwrap();
        vm.push(Cell::string("world")).unwrap();
        vm.push(Cell::Int(2)).unwrap();

        to_array_prim(&mut vm).unwrap();

        // The resulting Cell::Array handle should be on the stack.
        let result = vm.pop().unwrap();
        let pool_idx = match result {
            Cell::Array(idx) => idx,
            other => panic!("expected Cell::Array, got {:?}", other),
        };

        // The array pool entry must contain the two string elements in order.
        let arr = &vm.arrays[pool_idx];
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], Cell::string("hello"));
        assert_eq!(arr[1], Cell::string("world"));
    }
}
