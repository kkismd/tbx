//! Low-level execution substrate for the future 16-bit tbx16 VM.
//!
//! This module models the target exactly as a single 64 KiB memory image plus
//! byte-addressed registers. Data stack cells, return stack cells, and future
//! threaded code all live in the same `Memory`; stack operations are expressed
//! strictly as reads and writes against that memory.

pub mod address;
pub mod cell;
pub mod error;
pub mod memory;
pub mod registers;
pub mod stack;

use address::Address;
use cell::Cell;
use error::Tbx16Error;
use memory::Memory;
use registers::Registers;
use stack::{ensure_pointer_in_region, peek_cell, pop_cell, push_cell, ReturnFrame, StackRegion};

pub const DATA_STACK_START: Address = Address::new(0x0080);
pub const DATA_STACK_END: Address = Address::new(0x0100);
pub const DEFAULT_RETURN_STACK_START: Address = Address::new(0x0200);
pub const DEFAULT_RETURN_STACK_END: usize = 0x0300;
const PAGE_ONE_END_EXCLUSIVE: usize = 0x0200;

/// tbx16 VM substrate with unified memory and byte-addressed registers.
#[derive(Debug)]
pub struct Tbx16Vm {
    memory: Memory,
    registers: Registers,
    data_stack_region: StackRegion,
    return_stack_region: StackRegion,
}

impl Default for Tbx16Vm {
    fn default() -> Self {
        Self::new(
            StackRegion::new(DEFAULT_RETURN_STACK_START, DEFAULT_RETURN_STACK_END)
                .expect("default return stack region is valid"),
        )
        .expect("default tbx16 VM configuration is valid")
    }
}

impl Tbx16Vm {
    /// Creates a VM with zeroed memory and the configured return-stack region.
    ///
    /// `BP` starts at `DATA_STACK_START`, so slot address calculation follows
    /// the target rule `BP + slot_index * 2` from the beginning of execution.
    pub fn new(return_stack_region: StackRegion) -> Result<Self, Tbx16Error> {
        validate_return_stack_region(return_stack_region)?;
        let data_stack_region =
            StackRegion::new(DATA_STACK_START, usize::from(DATA_STACK_END.get()))
                .expect("fixed data stack region is valid");
        if data_stack_region.overlaps(return_stack_region) {
            return Err(Tbx16Error::InvalidStackRegion {
                start: return_stack_region.start(),
                end: return_stack_region.end_exclusive(),
                reason: "return stack region overlaps the fixed data stack region",
            });
        }

        let registers = Registers {
            ip: None,
            dsp: DATA_STACK_START,
            rsp: return_stack_region.start(),
            bp: DATA_STACK_START,
        };
        let vm = Self {
            memory: Memory::default(),
            registers,
            data_stack_region,
            return_stack_region,
        };
        vm.validate_invariants()?;
        Ok(vm)
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    pub fn registers(&self) -> &Registers {
        &self.registers
    }

    pub fn set_instruction_pointer(&mut self, ip: Address) -> Result<(), Tbx16Error> {
        if usize::from(ip.get()) >= memory::MEMORY_SIZE {
            return Err(Tbx16Error::InstructionPointerOutOfRange { ip });
        }
        self.registers.ip = Some(ip);
        Ok(())
    }

    pub fn data_stack_region(&self) -> StackRegion {
        self.data_stack_region
    }

    pub fn return_stack_region(&self) -> StackRegion {
        self.return_stack_region
    }

    pub fn data_slot_address(&self, slot_index: u16) -> Result<Address, Tbx16Error> {
        let offset = slot_index
            .checked_mul(2)
            .ok_or(Tbx16Error::InvalidMemoryAccess {
                addr: self.registers.bp,
                operation: "data slot address calculation",
            })?;
        self.registers
            .bp
            .checked_add(offset)
            .ok_or(Tbx16Error::InvalidMemoryAccess {
                addr: self.registers.bp,
                operation: "data slot address calculation",
            })
    }

    pub fn push_data_cell(&mut self, value: Cell) -> Result<(), Tbx16Error> {
        push_cell(
            &mut self.memory,
            &mut self.registers.dsp,
            self.data_stack_region,
            "data",
            value,
            Tbx16Error::DataStackOverflow,
        )
    }

    pub fn pop_data_cell(&mut self) -> Result<Cell, Tbx16Error> {
        pop_cell(
            &self.memory,
            &mut self.registers.dsp,
            self.data_stack_region,
            "data",
            Tbx16Error::DataStackUnderflow,
        )
    }

    pub fn peek_data_cell(&self, depth: usize) -> Result<Cell, Tbx16Error> {
        peek_cell(
            &self.memory,
            self.registers.dsp,
            self.data_stack_region,
            "data",
            depth,
            Tbx16Error::DataStackUnderflow,
        )
    }

    pub fn push_return_cell(&mut self, value: Cell) -> Result<(), Tbx16Error> {
        push_cell(
            &mut self.memory,
            &mut self.registers.rsp,
            self.return_stack_region,
            "return",
            value,
            Tbx16Error::ReturnStackOverflow,
        )
    }

    pub fn pop_return_cell(&mut self) -> Result<Cell, Tbx16Error> {
        pop_cell(
            &self.memory,
            &mut self.registers.rsp,
            self.return_stack_region,
            "return",
            Tbx16Error::ReturnStackUnderflow,
        )
    }

    pub fn peek_return_cell(&self, depth: usize) -> Result<Cell, Tbx16Error> {
        peek_cell(
            &self.memory,
            self.registers.rsp,
            self.return_stack_region,
            "return",
            depth,
            Tbx16Error::ReturnStackUnderflow,
        )
    }

    pub fn push_return_frame(&mut self, frame: ReturnFrame) -> Result<(), Tbx16Error> {
        let next = self
            .registers
            .rsp
            .checked_add(4)
            .ok_or(Tbx16Error::ReturnStackOverflow)?;
        ensure_pointer_in_region(self.registers.rsp, self.return_stack_region, "return")?;
        if !self.return_stack_region.contains_pointer(next) {
            return Err(Tbx16Error::ReturnStackOverflow);
        }
        self.memory
            .write_cell(self.registers.rsp, Cell::new(frame.return_ip.get()))?;
        self.memory.write_cell(
            self.registers
                .rsp
                .checked_add(2)
                .expect("aligned stack pointer has room for second frame cell"),
            Cell::new(frame.caller_bp.get()),
        )?;
        self.registers.rsp = next;
        Ok(())
    }

    pub fn pop_return_frame(&mut self) -> Result<ReturnFrame, Tbx16Error> {
        ensure_pointer_in_region(self.registers.rsp, self.return_stack_region, "return")?;
        let frame_start = self
            .registers
            .rsp
            .checked_sub(4)
            .ok_or(Tbx16Error::ReturnStackUnderflow)?;
        if frame_start < self.return_stack_region.start() {
            return Err(Tbx16Error::ReturnStackUnderflow);
        }
        let return_ip = self.memory.read_cell(frame_start)?;
        let caller_bp = self.memory.read_cell(
            frame_start
                .checked_add(2)
                .expect("aligned frame start always has space for caller_bp"),
        )?;
        self.registers.rsp = frame_start;
        Ok(ReturnFrame {
            return_ip: Address::new(return_ip.raw()),
            caller_bp: Address::new(caller_bp.raw()),
        })
    }

    pub fn validate_invariants(&self) -> Result<(), Tbx16Error> {
        ensure_pointer_in_region(self.registers.dsp, self.data_stack_region, "data")?;
        ensure_pointer_in_region(self.registers.rsp, self.return_stack_region, "return")?;
        if !self.data_stack_region.contains_pointer(self.registers.bp) {
            return Err(Tbx16Error::InvalidStackRegion {
                start: self.data_stack_region.start(),
                end: self.data_stack_region.end_exclusive(),
                reason: "base pointer must stay within the data stack region",
            });
        }
        if ((self.registers.bp.get() - self.data_stack_region.start().get()) % 2) != 0 {
            return Err(Tbx16Error::MisalignedStackPointer {
                stack: "base",
                addr: self.registers.bp,
            });
        }
        validate_return_stack_region(self.return_stack_region)?;
        if self.data_stack_region.overlaps(self.return_stack_region) {
            return Err(Tbx16Error::InvalidStackRegion {
                start: self.return_stack_region.start(),
                end: self.return_stack_region.end_exclusive(),
                reason: "return stack region overlaps the fixed data stack region",
            });
        }
        Ok(())
    }
}

fn validate_return_stack_region(region: StackRegion) -> Result<(), Tbx16Error> {
    if usize::from(region.start().get()) < PAGE_ONE_END_EXCLUSIVE {
        return Err(Tbx16Error::InvalidStackRegion {
            start: region.start(),
            end: region.end_exclusive(),
            reason: "return stack region must not overlap zero page or page 1",
        });
    }
    if region.len_bytes() % 2 != 0 {
        return Err(Tbx16Error::InvalidStackRegion {
            start: region.start(),
            end: region.end_exclusive(),
            reason: "return stack region length must be an even number of bytes",
        });
    }
    Ok(())
}
