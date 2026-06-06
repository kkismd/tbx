use std::fmt;

/// A 16-bit byte address in the tbx16 address space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address(u16);

impl Address {
    /// Creates an address from its raw 16-bit value.
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the raw 16-bit value.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Returns `true` when the address is aligned to a 2-byte cell boundary.
    pub const fn is_even(self) -> bool {
        self.0 % 2 == 0
    }

    /// Adds a byte offset without wrapping.
    pub fn checked_add(self, offset: u16) -> Option<Self> {
        self.0.checked_add(offset).map(Self)
    }

    /// Adds a byte offset without wrapping.
    pub fn checked_add_usize(self, offset: usize) -> Option<Self> {
        let offset = u16::try_from(offset).ok()?;
        self.checked_add(offset)
    }

    /// Subtracts a byte offset without wrapping.
    pub fn checked_sub(self, offset: u16) -> Option<Self> {
        self.0.checked_sub(offset).map(Self)
    }
}

impl From<u16> for Address {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:04x}", self.0)
    }
}
