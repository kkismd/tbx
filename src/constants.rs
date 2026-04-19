/// Maximum number of cells in the dictionary (data layer).
/// Approximately 8 MB when each cell is 8 bytes.
pub const MAX_DICTIONARY_CELLS: usize = 1_048_576;

/// Maximum depth of the return stack.
/// Exceeding this limit raises `TbxError::ReturnStackOverflow`.
pub const MAX_RETURN_STACK_DEPTH: usize = 4_096;
