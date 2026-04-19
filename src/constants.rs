/// Maximum number of cells in the dictionary (data layer).
/// Approximately 8 MB when each cell is 8 bytes.
pub const MAX_DICTIONARY_CELLS: usize = 1_048_576;

/// Token kind code for identifiers.
pub const TOK_ID: i64 = 0;
/// Token kind code for numeric literals.
pub const TOK_NUM: i64 = 1;
/// Token kind code for operators.
pub const TOK_OP: i64 = 2;
/// Token kind code for delimiters (e.g. `;`).
pub const TOK_DELIM: i64 = 3;
/// Token kind code for string literals.
pub const TOK_STR: i64 = 4;
