//! Low-dependency numeric primitives.
//!
//! These primitives operate on `Cell::Int` / `Cell::Float` values popped
//! from the data stack. They only depend on [`Cell`], [`VM`], and
//! [`TbxError`], and preserve the existing overflow / division-by-zero /
//! float handling semantics.

use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

pub fn add_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_add(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x + y))?,
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 + y))?,
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x + y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

pub fn sub_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_sub(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x - y))?,
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 - y))?,
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x - y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

pub fn mul_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_mul(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x * y))?,
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 * y))?,
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x * y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

#[allow(clippy::redundant_guards)] // Float(0.0) pattern also matches -0.0; use guard for clarity
pub fn div_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_number()?;
    let a = vm.pop_number()?;
    match (a, b) {
        (Cell::Int(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Int(y)) => {
            let result = x.checked_div(y).ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        (Cell::Float(_), Cell::Float(y)) if y == 0.0 => return Err(TbxError::DivisionByZero),
        (Cell::Float(x), Cell::Float(y)) => vm.push(Cell::Float(x / y))?,
        (Cell::Int(_), Cell::Float(y)) if y == 0.0 => return Err(TbxError::DivisionByZero),
        (Cell::Int(x), Cell::Float(y)) => vm.push(Cell::Float(x as f64 / y))?,
        (Cell::Float(_), Cell::Int(0)) => return Err(TbxError::DivisionByZero),
        (Cell::Float(x), Cell::Int(y)) => vm.push(Cell::Float(x / y as f64))?,
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

pub fn mod_prim(vm: &mut VM) -> Result<(), TbxError> {
    let b = vm.pop_int()?;
    let a = vm.pop_int()?;
    if b == 0 {
        return Err(TbxError::DivisionByZero);
    }
    let result = a.checked_rem(b).ok_or(TbxError::IntegerOverflow)?;
    vm.push(Cell::Int(result))?;
    Ok(())
}

/// SQRT — compute the square root of a number. Pushes a Float result.
/// Accepts Int or Float. Negative values and NaN/Infinity produce an error.
/// Negative zero (-0.0) is normalized to positive zero (0.0).
pub fn sqrt_prim(vm: &mut VM) -> Result<(), TbxError> {
    let num = vm.pop_number()?;
    match num {
        Cell::Int(i) if i < 0 => {
            return Err(TbxError::InvalidArgument {
                message: format!("sqrt of negative number: {i}"),
            });
        }
        Cell::Float(f) if f.is_nan() || !f.is_finite() => {
            return Err(TbxError::InvalidArgument {
                message: format!("sqrt of invalid number: {f}"),
            });
        }
        Cell::Float(f) if f < 0.0 => {
            return Err(TbxError::InvalidArgument {
                message: format!("sqrt of negative number: {f}"),
            });
        }
        Cell::Int(i) => {
            let result = (i as f64).sqrt();
            vm.push(Cell::Float(result))?;
        }
        Cell::Float(f) => {
            // Normalize -0.0 to 0.0 for consistency with zero handling elsewhere.
            let f = if f == 0.0 { 0.0 } else { f };
            let result = f.sqrt();
            vm.push(Cell::Float(result))?;
        }
        _ => unreachable!("pop_number guarantees Int or Float"),
    }
    Ok(())
}

/// NEGATE — negate the numeric value on top of the data stack.
///
/// - `Cell::Int(n)` → `Cell::Int(-n)` (returns `IntegerOverflow` for `i64::MIN`)
/// - `Cell::Float(v)` → `Cell::Float(-v)`
/// - any other type → `TbxError::TypeError`
pub fn negate_prim(vm: &mut VM) -> Result<(), TbxError> {
    let val = vm.pop()?;
    match val {
        Cell::Int(n) => {
            let result = n.checked_neg().ok_or(TbxError::IntegerOverflow)?;
            vm.push(Cell::Int(result))?;
        }
        Cell::Float(v) => {
            vm.push(Cell::Float(-v))?;
        }
        other => {
            return Err(TbxError::TypeError {
                expected: "Int or Float",
                got: other.type_name(),
            });
        }
    }
    Ok(())
}
