//! Low-dependency logical and bitwise primitives.
//!
//! These primitives implement boolean (`AND`, `OR`, `NOT`, `TO_BOOL`) and bitwise
//! (`BAND`, `BOR`) operations over the data stack. They only depend on [`Cell`],
//! [`VM`], and [`TbxError`], and preserve the existing
//! [`Cell::is_truthy`] semantics.

use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// AND — logical AND. Evaluates both operands with is_truthy() and pushes the result as Bool.
pub fn and_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a.is_truthy() && b.is_truthy()))?;
    Ok(())
}

/// OR — logical OR. Evaluates both operands with is_truthy() and pushes the result as Bool.
pub fn or_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    vm.push(Cell::Bool(a.is_truthy() || b.is_truthy()))?;
    Ok(())
}

/// NOT — logical negation. Evaluates one operand with is_truthy() and pushes the inverted Bool.
pub fn not_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    vm.push(Cell::Bool(!value.is_truthy()))?;
    Ok(())
}

/// TO_BOOL — normalize one value to Bool using is_truthy().
pub fn to_bool_prim(vm: &mut VM) -> Result<(), TbxError> {
    let value = vm.pop()?;
    vm.push(Cell::Bool(value.is_truthy()))?;
    Ok(())
}

/// BAND — bitwise AND. Both operands must be Int.
pub fn band_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_int()?;
    let a = vm.pop_int()?;
    vm.push(Cell::Int(a & b))?;
    Ok(())
}

/// BOR — bitwise OR. Both operands must be Int.
pub fn bor_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_int()?;
    let a = vm.pop_int()?;
    vm.push(Cell::Int(a | b))?;
    Ok(())
}
