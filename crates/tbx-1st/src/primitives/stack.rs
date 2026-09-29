//! Low-dependency stack primitives.
//!
//! These primitives manipulate the data stack without touching strings,
//! arrays, dictionary writes, or the compiler. They only depend on
//! [`Cell`], [`VM`], and [`TbxError`].

use crate::cell::Cell;
use crate::error::TbxError;
use crate::vm::VM;

/// DROP — discard the top element of the data stack.
pub fn drop_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.pop()?;
    Ok(())
}

/// LIT_MARKER — push a Cell::Marker sentinel onto the data stack.
pub fn lit_marker_prim(vm: &mut VM) -> Result<(), TbxError> {
    vm.push(Cell::Marker)?;
    Ok(())
}

/// DUP — duplicate the top element of the data stack.
pub fn dup_prim(vm: &mut VM) -> Result<(), TbxError> {
    let top = vm.pop()?;
    vm.push(top.clone())?;
    vm.push(top)?;
    Ok(())
}

/// SWAP — exchange the top two elements of the data stack.
pub fn swap_prim(vm: &mut VM) -> Result<(), TbxError> {
    let a = vm.pop()?;
    let b = vm.pop()?;
    vm.push(a)?;
    vm.push(b)?;
    Ok(())
}
