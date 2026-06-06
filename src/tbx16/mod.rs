//! Low-level execution substrate for the future 16-bit tbx16 VM.
//!
//! This module models the target exactly as a single 64 KiB memory image plus
//! byte-addressed registers. Data stack cells, return stack cells, and threaded
//! code all live in the same `Memory`; stack operations and threaded dispatch
//! are expressed strictly as reads and writes against that memory.

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
pub const DEFAULT_RETURN_STACK_END: Address = Address::new(0x0300);
const PAGE_ONE_END: Address = Address::new(0x0200);
const NO_IP: Address = Address::new(0xffff);
const RETURN_FRAME_BYTES: u16 = 4;

pub const CODE_TOKEN_PRIMITIVE: Cell = Cell::new(0x0001);
pub const CODE_TOKEN_DOCOL: Cell = Cell::new(0x0002);

/// Result of one `tbx16` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Halted,
    Returned,
    Trapped(Tbx16Error),
}

/// Minimal primitive registry for the M2.2 threaded kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum PrimitiveId {
    Lit = 1,
    Branch = 2,
    ZBranch = 3,
    Exit = 4,
    Halt = 5,
}

impl PrimitiveId {
    pub const fn as_cell(self) -> Cell {
        Cell::new(self as u16)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Lit => "LIT",
            Self::Branch => "BRANCH",
            Self::ZBranch => "ZBRANCH",
            Self::Exit => "EXIT",
            Self::Halt => "HALT",
        }
    }
}

impl TryFrom<Cell> for PrimitiveId {
    type Error = ();

    fn try_from(value: Cell) -> Result<Self, Self::Error> {
        match value.raw() {
            1 => Ok(Self::Lit),
            2 => Ok(Self::Branch),
            3 => Ok(Self::ZBranch),
            4 => Ok(Self::Exit),
            5 => Ok(Self::Halt),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EntryContext {
    initial_bp: Address,
    frame_base: Address,
}

#[derive(Debug, Clone, Copy)]
struct ColonWord {
    arity: u16,
    local_count: u16,
    parameter_ip: Address,
}

#[derive(Debug, Clone, Copy)]
enum ResolvedWord {
    Primitive(PrimitiveId),
    Colon(ColonWord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepControl {
    Continue,
    Final(ExecutionOutcome),
}

/// tbx16 VM substrate with unified memory and byte-addressed registers.
#[derive(Debug)]
pub struct Tbx16Vm {
    memory: Memory,
    registers: Registers,
    data_stack_region: StackRegion,
    return_stack_region: StackRegion,
    step_limit: Option<usize>,
    step_counter: usize,
    call_depth: u16,
    entry_context: Option<EntryContext>,
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
        let data_stack_region = StackRegion::new(DATA_STACK_START, DATA_STACK_END)
            .expect("fixed data stack region is valid");
        if data_stack_region.overlaps(return_stack_region) {
            return Err(Tbx16Error::InvalidStackRegion {
                start: return_stack_region.start(),
                end: return_stack_region.end(),
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
            step_limit: None,
            step_counter: 0,
            call_depth: 0,
            entry_context: None,
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
        validate_instruction_pointer_target(ip)?;
        self.registers.ip = Some(ip);
        Ok(())
    }

    pub fn data_stack_region(&self) -> StackRegion {
        self.data_stack_region
    }

    pub fn return_stack_region(&self) -> StackRegion {
        self.return_stack_region
    }

    pub fn set_step_limit(&mut self, step_limit: Option<usize>) {
        self.step_limit = step_limit;
    }

    pub fn step_counter(&self) -> usize {
        self.step_counter
    }

    pub fn call_depth(&self) -> u16 {
        self.call_depth
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
            .checked_add(RETURN_FRAME_BYTES)
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
        let frame = self.peek_return_frame()?;
        self.registers.rsp = self
            .registers
            .rsp
            .checked_sub(RETURN_FRAME_BYTES)
            .ok_or(Tbx16Error::ReturnStackUnderflow)?;
        Ok(frame)
    }

    pub fn run(&mut self, entry_xt: Cell) -> ExecutionOutcome {
        self.step_counter = 0;
        self.call_depth = 0;
        self.entry_context = None;

        let start_control = match self.start_entry(entry_xt) {
            Ok(control) => control,
            Err(err) => return self.finish_run(ExecutionOutcome::Trapped(err)),
        };

        if let StepControl::Final(outcome) = start_control {
            return self.finish_run(outcome);
        }

        loop {
            let control = match self.dispatch_step() {
                Ok(control) => control,
                Err(err) => return self.finish_run(ExecutionOutcome::Trapped(err)),
            };
            if let StepControl::Final(outcome) = control {
                return self.finish_run(outcome);
            }
        }
    }

    pub fn validate_invariants(&self) -> Result<(), Tbx16Error> {
        ensure_pointer_in_region(self.registers.dsp, self.data_stack_region, "data")?;
        ensure_pointer_in_region(self.registers.rsp, self.return_stack_region, "return")?;
        if let Some(ip) = self.registers.ip {
            validate_instruction_pointer_target(ip)?;
        }
        if !self.data_stack_region.contains_pointer(self.registers.bp) {
            return Err(Tbx16Error::InvalidStackRegion {
                start: self.data_stack_region.start(),
                end: self.data_stack_region.end(),
                reason: "base pointer must stay within the data stack region",
            });
        }
        if ((self.registers.bp.get() - self.data_stack_region.start().get()) % 2) != 0 {
            return Err(Tbx16Error::MisalignedStackPointer {
                stack: "base",
                addr: self.registers.bp,
            });
        }
        if self.registers.bp > self.registers.dsp {
            return Err(Tbx16Error::InvalidStackRegion {
                start: self.data_stack_region.start(),
                end: self.data_stack_region.end(),
                reason: "base pointer must not exceed the data stack pointer",
            });
        }
        validate_return_stack_region(self.return_stack_region)?;
        if self.data_stack_region.overlaps(self.return_stack_region) {
            return Err(Tbx16Error::InvalidStackRegion {
                start: self.return_stack_region.start(),
                end: self.return_stack_region.end(),
                reason: "return stack region overlaps the fixed data stack region",
            });
        }
        if self.entry_context.is_some() {
            let used = self.registers.rsp.get() - self.return_stack_region.start().get();
            let expected = self.call_depth.checked_mul(RETURN_FRAME_BYTES).ok_or(
                Tbx16Error::InvalidStackRegion {
                    start: self.return_stack_region.start(),
                    end: self.return_stack_region.end(),
                    reason: "call depth overflowed the return stack accounting",
                },
            )?;
            if used != expected {
                return Err(Tbx16Error::InvalidStackRegion {
                    start: self.return_stack_region.start(),
                    end: self.return_stack_region.end(),
                    reason: "return stack usage does not match call depth",
                });
            }
        }
        Ok(())
    }

    fn start_entry(&mut self, entry_xt: Cell) -> Result<StepControl, Tbx16Error> {
        self.begin_step()?;
        match self.resolve_word(entry_xt)? {
            ResolvedWord::Primitive(primitive) => {
                let control = self.execute_entry_primitive(primitive)?;
                self.step_counter += 1;
                self.debug_validate_state();
                Ok(control)
            }
            ResolvedWord::Colon(word) => {
                let initial_bp = self.registers.bp;
                let frame_base = self.compute_frame_base(word.arity)?;
                let locals_start = self.registers.dsp;
                let new_dsp = self.checked_extend_data_stack(locals_start, word.local_count)?;

                if word.local_count != 0 {
                    self.memory
                        .zero_range(locals_start, usize::from(word.local_count) * 2)?;
                }
                self.registers.bp = frame_base;
                self.registers.dsp = new_dsp;
                self.registers.ip = Some(word.parameter_ip);
                self.entry_context = Some(EntryContext {
                    initial_bp,
                    frame_base,
                });
                self.step_counter += 1;
                self.debug_validate_state();
                Ok(StepControl::Continue)
            }
        }
    }

    fn execute_entry_primitive(
        &mut self,
        primitive: PrimitiveId,
    ) -> Result<StepControl, Tbx16Error> {
        match primitive {
            PrimitiveId::Halt => Ok(StepControl::Final(ExecutionOutcome::Halted)),
            PrimitiveId::Lit | PrimitiveId::Branch | PrimitiveId::ZBranch | PrimitiveId::Exit => {
                let _ = self.current_ip()?;
                Ok(StepControl::Final(ExecutionOutcome::Returned))
            }
        }
    }

    fn dispatch_step(&mut self) -> Result<StepControl, Tbx16Error> {
        self.begin_step()?;

        let ip = self.current_ip()?;
        let xt = self.read_ip_cell(ip)?;
        let continuation_ip = validate_instruction_pointer_target(
            ip.checked_add(2)
                .ok_or(Tbx16Error::InstructionPointerOutOfRange { ip })?,
        )?;

        let control = match self.resolve_word(xt)? {
            ResolvedWord::Primitive(primitive) => {
                self.execute_primitive(primitive, continuation_ip)?
            }
            ResolvedWord::Colon(word) => {
                self.execute_colon_call(word, continuation_ip)?;
                StepControl::Continue
            }
        };

        self.step_counter += 1;
        self.debug_validate_state();
        Ok(control)
    }

    fn execute_primitive(
        &mut self,
        primitive: PrimitiveId,
        continuation_ip: Address,
    ) -> Result<StepControl, Tbx16Error> {
        match primitive {
            PrimitiveId::Lit => {
                let literal = self.read_ip_cell(continuation_ip)?;
                let next_ip =
                    validate_instruction_pointer_target(continuation_ip.checked_add(2).ok_or(
                        Tbx16Error::InstructionPointerOutOfRange {
                            ip: continuation_ip,
                        },
                    )?)?;
                self.ensure_data_stack_pushable(self.registers.dsp)?;
                self.push_data_cell(literal)?;
                self.registers.ip = Some(next_ip);
                Ok(StepControl::Continue)
            }
            PrimitiveId::Branch => {
                let target = self.read_ip_cell(continuation_ip)?;
                let target_ip = validate_instruction_pointer_target(Address::new(target.raw()))?;
                self.registers.ip = Some(target_ip);
                Ok(StepControl::Continue)
            }
            PrimitiveId::ZBranch => {
                let target = self.read_ip_cell(continuation_ip)?;
                let target_ip = validate_instruction_pointer_target(Address::new(target.raw()))?;
                let condition = self.peek_data_cell(0)?;
                let fallthrough_ip =
                    validate_instruction_pointer_target(continuation_ip.checked_add(2).ok_or(
                        Tbx16Error::InstructionPointerOutOfRange {
                            ip: continuation_ip,
                        },
                    )?)?;
                self.pop_data_cell()?;
                self.registers.ip = Some(if condition.raw() == 0 {
                    target_ip
                } else {
                    fallthrough_ip
                });
                Ok(StepControl::Continue)
            }
            PrimitiveId::Exit => self.execute_exit(continuation_ip),
            PrimitiveId::Halt => {
                self.registers.ip = Some(continuation_ip);
                Ok(StepControl::Final(ExecutionOutcome::Halted))
            }
        }
    }

    fn execute_exit(&mut self, continuation_ip: Address) -> Result<StepControl, Tbx16Error> {
        let return_count = self.read_ip_cell(continuation_ip)?;
        let return_value = match return_count.raw() {
            0 => None,
            1 => Some(self.peek_data_cell(0)?),
            _ => {
                return Err(Tbx16Error::InvalidReturnCount {
                    count: return_count,
                })
            }
        };

        if self.call_depth == 0 {
            let entry = self.entry_context.ok_or(Tbx16Error::InvalidReturnCount {
                count: return_count,
            })?;
            if return_value.is_some() {
                self.ensure_data_stack_pushable(entry.frame_base)?;
            }
            if let Some(value) = return_value {
                self.memory.write_cell(entry.frame_base, value)?;
                self.registers.dsp = entry
                    .frame_base
                    .checked_add(2)
                    .expect("validated top-level return slot has room");
            } else {
                self.registers.dsp = entry.frame_base;
            }
            self.registers.bp = entry.initial_bp;
            self.registers.ip = Some(continuation_ip);
            return Ok(StepControl::Final(ExecutionOutcome::Returned));
        }

        let frame = self.peek_return_frame()?;
        let return_ip = validate_instruction_pointer_target(frame.return_ip)?;
        if return_value.is_some() {
            self.ensure_data_stack_pushable(self.registers.bp)?;
        }

        let new_rsp = self
            .registers
            .rsp
            .checked_sub(RETURN_FRAME_BYTES)
            .ok_or(Tbx16Error::ReturnStackUnderflow)?;

        if let Some(value) = return_value {
            self.memory.write_cell(self.registers.bp, value)?;
            self.registers.dsp = self
                .registers
                .bp
                .checked_add(2)
                .expect("validated nested return slot has room");
        } else {
            self.registers.dsp = self.registers.bp;
        }
        self.registers.rsp = new_rsp;
        self.registers.bp = frame.caller_bp;
        self.registers.ip = Some(return_ip);
        self.call_depth -= 1;
        Ok(StepControl::Continue)
    }

    fn execute_colon_call(
        &mut self,
        word: ColonWord,
        return_ip: Address,
    ) -> Result<(), Tbx16Error> {
        let new_bp = self.compute_frame_base(word.arity)?;
        let locals_start = self.registers.dsp;
        let new_dsp = self.checked_extend_data_stack(locals_start, word.local_count)?;
        let new_rsp = self
            .registers
            .rsp
            .checked_add(RETURN_FRAME_BYTES)
            .ok_or(Tbx16Error::ReturnStackOverflow)?;
        ensure_pointer_in_region(self.registers.rsp, self.return_stack_region, "return")?;
        if !self.return_stack_region.contains_pointer(new_rsp) {
            return Err(Tbx16Error::ReturnStackOverflow);
        }

        let caller_bp = self.registers.bp;
        self.memory
            .write_cell(self.registers.rsp, Cell::new(return_ip.get()))?;
        self.memory.write_cell(
            self.registers
                .rsp
                .checked_add(2)
                .expect("validated return frame has room for caller bp"),
            Cell::new(caller_bp.get()),
        )?;
        if word.local_count != 0 {
            self.memory
                .zero_range(locals_start, usize::from(word.local_count) * 2)?;
        }

        self.registers.rsp = new_rsp;
        self.registers.bp = new_bp;
        self.registers.dsp = new_dsp;
        self.registers.ip = Some(word.parameter_ip);
        self.call_depth = self
            .call_depth
            .checked_add(1)
            .expect("call depth fits in u16 for the configured return stack");
        Ok(())
    }

    fn resolve_word(&self, xt: Cell) -> Result<ResolvedWord, Tbx16Error> {
        let xt_addr = validate_execution_token_address(xt)?;
        let code_token = self
            .memory
            .read_cell(xt_addr)
            .map_err(|_| invalid_execution_token(xt))?;

        if code_token == CODE_TOKEN_PRIMITIVE {
            let primitive_id_addr = xt_addr
                .checked_add(2)
                .ok_or_else(|| invalid_execution_token(xt))?;
            let primitive_id = self
                .memory
                .read_cell(primitive_id_addr)
                .map_err(|_| invalid_execution_token(xt))?;
            let primitive =
                PrimitiveId::try_from(primitive_id).map_err(|_| invalid_execution_token(xt))?;
            return Ok(ResolvedWord::Primitive(primitive));
        }

        if code_token == CODE_TOKEN_DOCOL {
            let arity_addr = xt_addr
                .checked_add(2)
                .ok_or_else(|| invalid_execution_token(xt))?;
            let local_count_addr = xt_addr
                .checked_add(4)
                .ok_or_else(|| invalid_execution_token(xt))?;
            let parameter_ip = validate_instruction_pointer_target(
                xt_addr
                    .checked_add(6)
                    .ok_or_else(|| invalid_execution_token(xt))?,
            )
            .map_err(|_| invalid_execution_token(xt))?;
            let arity = self
                .memory
                .read_cell(arity_addr)
                .map_err(|_| invalid_execution_token(xt))?
                .raw();
            let local_count = self
                .memory
                .read_cell(local_count_addr)
                .map_err(|_| invalid_execution_token(xt))?
                .raw();
            return Ok(ResolvedWord::Colon(ColonWord {
                arity,
                local_count,
                parameter_ip,
            }));
        }

        Err(invalid_execution_token(xt))
    }

    fn begin_step(&self) -> Result<(), Tbx16Error> {
        if self
            .step_limit
            .is_some_and(|limit| self.step_counter >= limit)
        {
            return Err(Tbx16Error::StepLimitExceeded);
        }
        Ok(())
    }

    fn current_ip(&self) -> Result<Address, Tbx16Error> {
        let ip = self
            .registers
            .ip
            .ok_or(Tbx16Error::InstructionPointerOutOfRange { ip: NO_IP })?;
        validate_instruction_pointer_target(ip)
    }

    fn read_ip_cell(&self, ip: Address) -> Result<Cell, Tbx16Error> {
        validate_instruction_pointer_target(ip)?;
        self.memory.read_cell(ip)
    }

    fn compute_frame_base(&self, arity: u16) -> Result<Address, Tbx16Error> {
        let arg_bytes = arity.checked_mul(2).ok_or(Tbx16Error::DataStackUnderflow)?;
        let frame_base = self
            .registers
            .dsp
            .checked_sub(arg_bytes)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        if frame_base < self.data_stack_region.start() {
            return Err(Tbx16Error::DataStackUnderflow);
        }
        Ok(frame_base)
    }

    fn checked_extend_data_stack(
        &self,
        base: Address,
        cell_count: u16,
    ) -> Result<Address, Tbx16Error> {
        let byte_len = cell_count
            .checked_mul(2)
            .ok_or(Tbx16Error::DataStackOverflow)?;
        let new_dsp = base
            .checked_add(byte_len)
            .ok_or(Tbx16Error::DataStackOverflow)?;
        ensure_pointer_in_region(base, self.data_stack_region, "data")?;
        if !self.data_stack_region.contains_pointer(new_dsp) {
            return Err(Tbx16Error::DataStackOverflow);
        }
        Ok(new_dsp)
    }

    fn ensure_data_stack_pushable(&self, pointer: Address) -> Result<(), Tbx16Error> {
        let next = pointer
            .checked_add(2)
            .ok_or(Tbx16Error::DataStackOverflow)?;
        ensure_pointer_in_region(pointer, self.data_stack_region, "data")?;
        if !self.data_stack_region.contains_pointer(next) {
            return Err(Tbx16Error::DataStackOverflow);
        }
        Ok(())
    }

    fn peek_return_frame(&self) -> Result<ReturnFrame, Tbx16Error> {
        ensure_pointer_in_region(self.registers.rsp, self.return_stack_region, "return")?;
        let frame_start = self
            .registers
            .rsp
            .checked_sub(RETURN_FRAME_BYTES)
            .ok_or(Tbx16Error::ReturnStackUnderflow)?;
        if frame_start < self.return_stack_region.start() {
            return Err(Tbx16Error::ReturnStackUnderflow);
        }
        let return_ip = self.memory.read_cell(frame_start)?;
        let caller_bp = self.memory.read_cell(
            frame_start
                .checked_add(2)
                .expect("validated frame start has room for caller bp"),
        )?;
        Ok(ReturnFrame {
            return_ip: Address::new(return_ip.raw()),
            caller_bp: Address::new(caller_bp.raw()),
        })
    }

    fn finish_run(&mut self, outcome: ExecutionOutcome) -> ExecutionOutcome {
        self.entry_context = None;
        outcome
    }

    fn debug_validate_state(&self) {
        #[cfg(debug_assertions)]
        self.validate_invariants()
            .expect("tbx16 invariants must hold after successful state transitions");
    }
}

fn validate_return_stack_region(region: StackRegion) -> Result<(), Tbx16Error> {
    if region.start() < PAGE_ONE_END {
        return Err(Tbx16Error::InvalidStackRegion {
            start: region.start(),
            end: region.end(),
            reason: "return stack region must not overlap zero page or page 1",
        });
    }
    if region.len_bytes() % 2 != 0 {
        return Err(Tbx16Error::InvalidStackRegion {
            start: region.start(),
            end: region.end(),
            reason: "return stack region length must be an even number of bytes",
        });
    }
    Ok(())
}

fn validate_execution_token_address(xt: Cell) -> Result<Address, Tbx16Error> {
    let xt_addr = Address::new(xt.raw());
    if xt_addr.get() == NO_IP.get() || !xt_addr.is_even() {
        return Err(invalid_execution_token(xt));
    }
    Ok(xt_addr)
}

fn validate_instruction_pointer_target(ip: Address) -> Result<Address, Tbx16Error> {
    if ip.get() == NO_IP.get() || !ip.is_even() {
        return Err(Tbx16Error::InstructionPointerOutOfRange { ip });
    }
    Ok(ip)
}

fn invalid_execution_token(xt: Cell) -> Tbx16Error {
    Tbx16Error::InvalidExecutionToken { xt }
}
