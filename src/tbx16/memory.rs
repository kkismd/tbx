use crate::tbx16::address::Address;
use crate::tbx16::cell::Cell;
use crate::tbx16::error::Tbx16Error;

pub const MEMORY_SIZE: usize = 65_536;

/// Unified 64 KiB memory owned by the tbx16 VM.
#[derive(Debug, Clone)]
pub struct Memory {
    bytes: [u8; MEMORY_SIZE],
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            bytes: [0; MEMORY_SIZE],
        }
    }
}

impl Memory {
    /// Returns the byte at `addr`.
    pub fn read_byte(&self, addr: Address) -> Result<u8, Tbx16Error> {
        Ok(self.bytes[usize::from(addr.get())])
    }

    /// Writes one byte at `addr`.
    pub fn write_byte(&mut self, addr: Address, value: u8) -> Result<(), Tbx16Error> {
        self.bytes[usize::from(addr.get())] = value;
        Ok(())
    }

    /// Reads one little-endian cell from `addr`.
    pub fn read_cell(&self, addr: Address) -> Result<Cell, Tbx16Error> {
        let next = addr.checked_add(1).ok_or(Tbx16Error::InvalidMemoryAccess {
            addr,
            operation: "cell read",
        })?;
        let lo = self.read_byte(addr)?;
        let hi = self.read_byte(next)?;
        Ok(Cell::from_le_bytes([lo, hi]))
    }

    /// Writes one little-endian cell at `addr`.
    pub fn write_cell(&mut self, addr: Address, value: Cell) -> Result<(), Tbx16Error> {
        let next = addr.checked_add(1).ok_or(Tbx16Error::InvalidMemoryAccess {
            addr,
            operation: "cell write",
        })?;
        let [lo, hi] = value.to_le_bytes();
        self.write_byte(addr, lo)?;
        self.write_byte(next, hi)?;
        Ok(())
    }

    /// Loads raw bytes into memory without wrapping.
    pub fn load_bytes(&mut self, start: Address, bytes: &[u8]) -> Result<(), Tbx16Error> {
        let end = checked_range_end(start, bytes.len(), "byte load")?;
        self.bytes[usize::from(start.get())..usize::from(end.get())].copy_from_slice(bytes);
        Ok(())
    }

    /// Zeroes the half-open range `[start, start + len)`.
    pub fn zero_range(&mut self, start: Address, len: usize) -> Result<(), Tbx16Error> {
        let end = checked_range_end(start, len, "zero fill")?;
        self.bytes[usize::from(start.get())..usize::from(end.get())].fill(0);
        Ok(())
    }

    /// Returns the full memory image.
    pub fn as_bytes(&self) -> &[u8; MEMORY_SIZE] {
        &self.bytes
    }

    /// Returns mutable access to the full memory image.
    pub fn as_bytes_mut(&mut self) -> &mut [u8; MEMORY_SIZE] {
        &mut self.bytes
    }
}

fn checked_range_end(
    start: Address,
    len: usize,
    operation: &'static str,
) -> Result<Address, Tbx16Error> {
    start
        .checked_add_usize(len)
        .ok_or(Tbx16Error::InvalidMemoryAccess {
            addr: start,
            operation,
        })
}
