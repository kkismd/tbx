//! Low-dependency comparison primitives.
//!
//! These primitives implement equality and ordering comparisons over the data
//! stack. They only depend on [`Cell`], [`VM`], and [`TbxError`], and preserve
//! the existing Int/Float promotion semantics.
//!
//! `EQ` and `NEQ` rely on `Cell`'s `PartialEq` implementation for the general
//! case (which compares `Cell::Str` values by string content via `Rc<str>`'s
//! `PartialEq`), while still handling `Int`/`Float` mixed pairs explicitly.

use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// EQ — equality comparison. Pushes Bool(true) if the two top values are equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
/// All other pairs, including two Cell::Str values, use Cell's PartialEq.
pub fn eq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) == *y,
        (Cell::Float(x), Cell::Int(y)) => *x == (*y as f64),
        _ => a == b,
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// NEQ — inequality comparison. Pushes Bool(true) if the two top values are not equal.
/// Int/Float mixed pairs are compared by promoting Int to Float.
/// All other pairs, including two Cell::Str values, use Cell's PartialEq.
pub fn neq_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop()?;
    let a = vm.pop()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) != *y,
        (Cell::Float(x), Cell::Int(y)) => *x != (*y as f64),
        _ => a != b,
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// LT — less than. Pushes Bool(true) if a < b (numeric only, with Int/Float promotion).
pub fn lt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x < y,
        (Cell::Float(x), Cell::Float(y)) => x < y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) < *y,
        (Cell::Float(x), Cell::Int(y)) => *x < (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// GT — greater than. Pushes Bool(true) if a > b (numeric only, with Int/Float promotion).
pub fn gt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x > y,
        (Cell::Float(x), Cell::Float(y)) => x > y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) > *y,
        (Cell::Float(x), Cell::Int(y)) => *x > (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// LE — less than or equal. Pushes Bool(true) if a <= b (numeric only, with Int/Float promotion).
pub fn le_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x <= y,
        (Cell::Float(x), Cell::Float(y)) => x <= y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) <= *y,
        (Cell::Float(x), Cell::Int(y)) => *x <= (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}

/// GE — greater than or equal. Pushes Bool(true) if a >= b (numeric only, with Int/Float promotion).
pub fn ge_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    let result = match (&a, &b) {
        (Cell::Int(x), Cell::Int(y)) => x >= y,
        (Cell::Float(x), Cell::Float(y)) => x >= y,
        (Cell::Int(x), Cell::Float(y)) => (*x as f64) >= *y,
        (Cell::Float(x), Cell::Int(y)) => *x >= (*y as f64),
        _ => unreachable!("pop_number guarantees Int or Float"),
    };
    vm.push(Cell::Bool(result))?;
    Ok(())
}
