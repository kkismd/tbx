use crate::tbx16::address::Address;
use crate::tbx16::cell::Cell;
use crate::tbx16::error::Tbx16Error;
use crate::tbx16::memory::Memory;

/// A byte-addressed half-open stack region `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackRegion {
    start: Address,
    end: Address,
}

impl StackRegion {
    /// Creates a stack region with aligned endpoints and an even byte length.
    pub fn new(start: Address, end: Address) -> Result<Self, Tbx16Error> {
        if start >= end {
            return Err(Tbx16Error::InvalidStackRegion {
                start,
                end,
                reason: "start must be lower than end",
            });
        }
        if !start.is_even() || !end.is_even() {
            return Err(Tbx16Error::InvalidStackRegion {
                start,
                end,
                reason: "stack region endpoints must be 2-byte aligned",
            });
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> Address {
        self.start
    }

    pub const fn end(self) -> Address {
        self.end
    }

    pub fn len_bytes(self) -> u16 {
        self.end.get() - self.start.get()
    }

    pub fn contains(self, addr: Address) -> bool {
        self.start <= addr && addr < self.end
    }

    pub fn contains_pointer(self, addr: Address) -> bool {
        self.start <= addr && addr <= self.end
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// A return-stack frame stored as two cells in unified memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnFrame {
    pub return_ip: Address,
    pub caller_bp: Address,
}

pub(crate) fn ensure_pointer_in_region(
    pointer: Address,
    region: StackRegion,
    stack_name: &'static str,
) -> Result<(), Tbx16Error> {
    if !region.contains_pointer(pointer) {
        return Err(Tbx16Error::InvalidStackRegion {
            start: region.start(),
            end: region.end(),
            reason: match stack_name {
                "data" => "data stack pointer escaped its fixed region",
                _ => "stack pointer escaped its configured region",
            },
        });
    }
    if ((pointer.get() - region.start().get()) % 2) != 0 {
        return Err(Tbx16Error::MisalignedStackPointer {
            stack: stack_name,
            addr: pointer,
        });
    }
    Ok(())
}

pub(crate) fn push_cell(
    memory: &mut Memory,
    pointer: &mut Address,
    region: StackRegion,
    stack_name: &'static str,
    value: Cell,
    overflow: Tbx16Error,
) -> Result<(), Tbx16Error> {
    ensure_pointer_in_region(*pointer, region, stack_name)?;
    let next = pointer.checked_add(2).ok_or(overflow.clone())?;
    if !region.contains_pointer(next) {
        return Err(overflow);
    }
    memory.write_cell(*pointer, value)?;
    *pointer = next;
    Ok(())
}

pub(crate) fn pop_cell(
    memory: &Memory,
    pointer: &mut Address,
    region: StackRegion,
    stack_name: &'static str,
    underflow: Tbx16Error,
) -> Result<Cell, Tbx16Error> {
    ensure_pointer_in_region(*pointer, region, stack_name)?;
    if *pointer == region.start() {
        return Err(underflow);
    }
    let next = pointer.checked_sub(2).ok_or(underflow.clone())?;
    if !region.contains_pointer(next) {
        return Err(underflow);
    }
    *pointer = next;
    memory.read_cell(*pointer)
}

pub(crate) fn peek_cell(
    memory: &Memory,
    pointer: Address,
    region: StackRegion,
    stack_name: &'static str,
    depth: usize,
    underflow: Tbx16Error,
) -> Result<Cell, Tbx16Error> {
    ensure_pointer_in_region(pointer, region, stack_name)?;
    let byte_depth = depth
        .checked_add(1)
        .and_then(|n| n.checked_mul(2))
        .ok_or(underflow.clone())?;
    let byte_depth = u16::try_from(byte_depth).map_err(|_| underflow.clone())?;
    let addr = pointer.checked_sub(byte_depth).ok_or(underflow.clone())?;
    if addr < region.start() {
        return Err(underflow);
    }
    memory.read_cell(addr)
}
