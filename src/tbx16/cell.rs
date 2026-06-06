/// A raw 16-bit value in the tbx16 VM.
///
/// The cell does not carry a runtime tag. Primitive semantics decide whether a
/// given bit-pattern is interpreted as an integer, boolean, address, XT, or
/// other VM-level value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell(u16);

impl Cell {
    pub const FALSE: Self = Self(0x0000);
    pub const TRUE: Self = Self(0xffff);

    /// Creates a cell from its raw bits.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the raw 16-bit bits.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Returns the little-endian byte encoding.
    pub const fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// Creates a cell from little-endian bytes.
    pub const fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
    }

    /// Interprets the raw bits as a signed 16-bit integer.
    pub const fn as_i16(self) -> i16 {
        self.0 as i16
    }

    /// Reinterprets a signed 16-bit integer as raw cell bits.
    pub const fn from_i16(value: i16) -> Self {
        Self(value as u16)
    }

    /// Returns `true` for any non-zero value.
    pub const fn is_truthy(self) -> bool {
        self.0 != 0
    }

    /// Normalizes the current value to the canonical tbx16 boolean encoding.
    pub const fn to_canonical_bool(self) -> Self {
        Self::from_bool(self.is_truthy())
    }

    /// Creates a canonical boolean cell.
    pub const fn from_bool(value: bool) -> Self {
        if value {
            Self::TRUE
        } else {
            Self::FALSE
        }
    }
}

impl From<u16> for Cell {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}
