/// Maximum number of cells in the dictionary (data layer).
/// Approximately 8 MB when each cell is 8 bytes.
pub const MAX_DICTIONARY_CELLS: usize = 1_048_576;

/// Maximum depth of the data stack.
/// Exceeding this limit raises `TbxError::DataStackOverflow`.
pub const MAX_DATA_STACK_DEPTH: usize = 65_536;

/// Maximum depth of the return stack.
/// Exceeding this limit raises `TbxError::ReturnStackOverflow`.
pub const MAX_RETURN_STACK_DEPTH: usize = 4_096;

/// Maximum number of elements a single array may hold.
///
/// Applies to both 1D and 2D allocations.  For 2D arrays, `width * height`
/// must not exceed this value.  The limit guards against accidental allocation
/// of multi-gigabyte arrays due to programmer error.
pub const MAX_ARRAY_ELEMENTS: usize = 16_777_216;

/// Base index offset for VAR-declared local variables in variadic words.
///
/// In variadic words, the number of actual arguments is not known at compile time,
/// so `StackAddr` indices for VAR locals cannot be placed directly after formal
/// parameters (as they are in non-variadic words).  Instead, their indices are
/// encoded as `VARIADIC_LOCAL_BASE + local_slot_index`, which places them in a
/// distinct range well above any realistic argument count.
///
/// `VM::resolve_local_idx` detects indices in `[VARIADIC_LOCAL_BASE, ..)` and maps
/// them to `bp + actual_arity + (index - VARIADIC_LOCAL_BASE)` at runtime.
///
/// `ARG_ADDR` indices are always in `[0, actual_arity)` and are never in this range,
/// so the two namespaces are completely disjoint.
pub const VARIADIC_LOCAL_BASE: usize = 0x4000_0000;
