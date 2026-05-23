//! `ArrayRef` — Rc-backed shared mutable container for array elements.
//!
//! # Surface language policy
//!
//! `ArrayRef` (and its wrapper `Cell::Array`) is an **internal VM representation**
//! for array storage.  It is NOT a surface first-class array value.  The only
//! surface operations that involve arrays are `DIM @A[n]`, `@A[i]`, `&@A[i]`,
//! `LET @A[i] = expr`, `SET &@A[i], expr`, and `ARRAY_LEN(@A)`.
//!
//! Whole-array surface operations (`LET B = A`, `RETURN A`, `TUPLE(A)`,
//! `PUTVAL A`, `A = B`, `EQ(A, B)`) are unsupported.  See
//! `blueprint-language.md` §配列の surface policy for details.
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

/// Shape metadata for an array binding.
///
/// Internal storage is always linear; this enum records the declared
/// dimensionality so that future 2D element access can compute the correct
/// flat index.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayShape {
    /// One-dimensional array: `DIM @A[n]`
    OneD,
    /// Two-dimensional array: `DIM @A[w, h]`
    ///
    /// The flat storage length is `width * height`.
    TwoD { width: usize, height: usize },
}

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
    shape: ArrayShape,
}

impl ArrayRef {
    /// Create a new `ArrayRef` wrapping the given element vector.
    ///
    /// The shape is set to `ArrayShape::OneD` for backward compatibility.
    pub fn new(elems: Vec<Cell>) -> Self {
        ArrayRef {
            inner: Rc::new(RefCell::new(elems)),
            shape: ArrayShape::OneD,
        }
    }

    /// Create a new 2D `ArrayRef` with the given element vector and dimensions.
    ///
    /// The caller is responsible for ensuring `elems.len() == width * height`.
    /// The shape is set to `ArrayShape::TwoD { width, height }`.
    pub fn new_2d(elems: Vec<Cell>, width: usize, height: usize) -> Self {
        ArrayRef {
            inner: Rc::new(RefCell::new(elems)),
            shape: ArrayShape::TwoD { width, height },
        }
    }

    /// Return the shape of this array.
    pub fn shape(&self) -> &ArrayShape {
        &self.shape
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

    /// Return `true` if `self` and `other` point to the same Rc allocation.
    ///
    /// This is **pointer identity**, not content equality.  Two `ArrayRef`
    /// values that hold identical element sequences but were created
    /// independently will return `false`.  Only clones that share the same
    /// underlying `Rc` allocation return `true`.
    ///
    /// # Example
    ///
    /// ```
    /// # use tbx::array_ref::ArrayRef;
    /// # use tbx::cell::Cell;
    /// let a = ArrayRef::new(vec![Cell::Int(1)]);
    /// let b = a.clone();                         // same allocation
    /// let c = ArrayRef::new(vec![Cell::Int(1)]); // different allocation
    /// assert!(a.ptr_eq(&b));
    /// assert!(!a.ptr_eq(&c));
    /// ```
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
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

/// Developer diagnostics only.
///
/// This `Debug` impl shows current element contents for use in test output and
/// error messages.  It does NOT define the final display format or `PUTVAL`
/// semantics for arrays, which are out of scope for this issue.
impl std::fmt::Debug for ArrayRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ArrayRef({:?}, {:?})", self.shape, &*self.inner.borrow())
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
    fn debug_format() {
        let ar = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        let s = format!("{:?}", ar);
        assert!(s.starts_with("ArrayRef("));
    }

    // --- ArrayShape / new_2d tests ---

    #[test]
    fn new_has_oned_shape() {
        let ar = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        assert_eq!(ar.shape(), &ArrayShape::OneD);
    }

    #[test]
    fn new_2d_has_twod_shape() {
        let elems = vec![Cell::None; 12];
        let ar = ArrayRef::new_2d(elems, 4, 3);
        assert_eq!(
            ar.shape(),
            &ArrayShape::TwoD {
                width: 4,
                height: 3
            }
        );
    }

    #[test]
    fn new_2d_len_equals_width_times_height() {
        let width = 4;
        let height = 3;
        let elems = vec![Cell::None; width * height];
        let ar = ArrayRef::new_2d(elems, width, height);
        assert_eq!(ar.len(), width * height);
    }

    #[test]
    fn ptr_eq_clone_is_true() {
        let a = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        let b = a.clone();
        assert!(a.ptr_eq(&b));
    }

    #[test]
    fn ptr_eq_independent_same_content_is_false() {
        let a = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        let b = ArrayRef::new(vec![Cell::Int(1), Cell::Int(2)]);
        assert!(!a.ptr_eq(&b));
    }
}
