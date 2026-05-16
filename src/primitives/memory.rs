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
    use crate::cell::Cell;

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
}
