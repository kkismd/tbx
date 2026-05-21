//! `ArrayRef` — Rc-backed shared mutable container for array elements.
//!
//! # Design discipline
//!
//! * `ArrayRef` is an `Rc<RefCell<Vec<Cell>>>` handle; cloning it shares
//!   the same underlying storage (reference-count bump only).
//! * `Ref` / `RefMut` borrow guards must **never** escape a primitive boundary.
//!   Callers read an element by cloning `Cell` while inside a `borrow()` and
//!   dropping the guard immediately; callers write by calling `borrow_mut()`
//!   only for the duration of a single assignment.
//! * `ArrayAddr` will eventually hold `(ArrayRef, elem_idx)` but must not
//!   carry a borrow guard — the guard lifetime would be too short to bridge
//!   separate fetch / store primitives.

use std::cell::RefCell;
use std::rc::Rc;

use crate::cell::Cell;
use crate::error::TbxError;

/// A reference-counted, interior-mutable handle to an array of `Cell` values.
///
/// Multiple `ArrayRef` clones point to the same storage.  All mutation goes
/// through the shared `RefCell`, which panics at runtime if a borrow violation
/// occurs.  In practice, borrow guards are held only transiently (see the
/// module-level design discipline note), so a panic here always indicates a
/// programming error in a primitive implementation, not a user error.
#[derive(Clone)]
pub struct ArrayRef {
    inner: Rc<RefCell<Vec<Cell>>>,
}

impl ArrayRef {
    /// Create a new `ArrayRef` wrapping the given element vector.
    pub fn new(elems: Vec<Cell>) -> Self {
        ArrayRef {
            inner: Rc::new(RefCell::new(elems)),
        }
    }

    /// Return the number of elements in the array.
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Return `true` if the array contains no elements.
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// Clone and return the element at `idx`, or `None` if out of bounds.
    ///
    /// # Borrow discipline
    ///
    /// The `borrow()` guard is acquired, the element is cloned, and the guard
    /// is dropped before this function returns.  No `Ref` escapes the call.
    pub fn get_cloned(&self, idx: usize) -> Option<Cell> {
        self.inner.borrow().get(idx).cloned()
    }

    /// Write `value` into position `idx`.
    ///
    /// Returns `Err(TbxError::ArrayIndexOutOfBounds)` if `idx >= len`.
    ///
    /// # Borrow discipline
    ///
    /// The `borrow_mut()` guard is held only for the duration of the single
    /// assignment and is dropped before this function returns.  No `RefMut`
    /// escapes the call.
    pub fn set(&self, idx: usize, value: Cell) -> Result<(), TbxError> {
        let mut v = self.inner.borrow_mut();
        let len = v.len();
        if idx >= len {
            return Err(TbxError::ArrayIndexOutOfBounds {
                index: idx as i64,
                size: len,
            });
        }
        v[idx] = value;
        Ok(())
    }
}

// --- Trait impls ---

impl std::fmt::Debug for ArrayRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ArrayRef({:?})", &*self.inner.borrow())
    }
}

/// Provisional content equality: two `ArrayRef` values are equal when their
/// element vectors are equal.
///
/// This is intentionally shallow (element-wise `PartialEq`).  A future issue
/// may change the semantics (e.g. identity equality for mutable containers).
impl PartialEq for ArrayRef {
    fn eq(&self, other: &Self) -> bool {
        *self.inner.borrow() == *other.inner.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    #[test]
    fn new_and_len() {
        let ar = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)]);
        assert_eq!(ar.len(), 3);
    }

    #[test]
    fn is_empty_on_empty_vec() {
        let ar = ArrayRef::new(vec![]);
        assert!(ar.is_empty());
    }

    #[test]
    fn is_empty_on_non_empty_vec() {
        let ar = ArrayRef::new(vec![Cell::Int(0)]);
        assert!(!ar.is_empty());
    }

    #[test]
    fn get_cloned_returns_element() {
        let ar = ArrayRef::new(vec![Cell::Int(42), Cell::Bool(true)]);
        assert_eq!(ar.get_cloned(0), Some(Cell::Int(42)));
        assert_eq!(ar.get_cloned(1), Some(Cell::Bool(true)));
    }

    #[test]
    fn get_cloned_out_of_bounds_returns_none() {
        let ar = ArrayRef::new(vec![Cell::Int(1)]);
        assert_eq!(ar.get_cloned(1), None);
    }

    #[test]
    fn set_writes_value() {
        let ar = ArrayRef::new(vec![Cell::Int(0), Cell::Int(0)]);
        ar.set(0, Cell::Int(99)).unwrap();
        assert_eq!(ar.get_cloned(0), Some(Cell::Int(99)));
    }

    #[test]
    fn set_out_of_bounds_returns_error() {
        let ar = ArrayRef::new(vec![Cell::Int(0)]);
        let err = ar.set(1, Cell::Int(1)).unwrap_err();
        assert!(matches!(
            err,
            TbxError::ArrayIndexOutOfBounds { index: 1, size: 1 }
        ));
    }

    #[test]
    fn clone_shares_storage() {
        let ar1 = ArrayRef::new(vec![Cell::Int(0)]);
        let ar2 = ar1.clone();
        ar1.set(0, Cell::Int(7)).unwrap();
        // ar2 must see the mutation because it shares the Rc
        assert_eq!(ar2.get_cloned(0), Some(Cell::Int(7)));
    }

    #[test]
    fn partial_eq_same_content() {
        let a = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        let b = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        assert_eq!(a, b);
    }

    #[test]
    fn partial_eq_different_content() {
        let a = ArrayRef::new(vec![Cell::Int(1)]);
        let b = ArrayRef::new(vec![Cell::Int(2)]);
        assert_ne!(a, b);
    }

    #[test]
    fn debug_format() {
        let ar = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        let s = format!("{:?}", ar);
        assert!(s.starts_with("ArrayRef("));
    }
}
