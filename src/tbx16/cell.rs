pub type Cell = u16;

pub const WORD_BYTES: usize = 2;
pub const FALSE: Cell = 0x0000;
pub const TRUE: Cell = 0xffff;

#[must_use]
pub fn cell_to_i16(cell: Cell) -> i16 {
    i16::from_le_bytes(cell.to_le_bytes())
}

#[must_use]
pub fn cell_from_i16(value: i16) -> Cell {
    u16::from_le_bytes(value.to_le_bytes())
}

#[must_use]
pub fn canonical_bool(value: Cell) -> Cell {
    if value == FALSE {
        FALSE
    } else {
        TRUE
    }
}
