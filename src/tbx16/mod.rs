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

/// Result of one `tbx16` execution entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Halted,
    Returned,
    Trapped(Tbx16Error),
}

/// Primitive registry for the M2.2a threaded kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum PrimitiveId {
    Lit = 1,
    Branch = 2,
    ZBranch = 3,
    Halt = 4,
    Exit = 5,
}

impl PrimitiveId {
    pub const fn as_cell(self) -> Cell {
        Cell::new(self as u16)
    }
}

impl TryFrom<Cell> for PrimitiveId {
    type Error = ();

    fn try_from(value: Cell) -> Result<Self, Self::Error> {
        match value.raw() {
            1 => Ok(Self::Lit),
            2 => Ok(Self::Branch),
            3 => Ok(Self::ZBranch),
            4 => Ok(Self::Halt),
            5 => Ok(Self::Exit),
            _ => Err(()),
        }
    }
}

/// Resolved word metadata for the shared XT namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedWord {
    Primitive(PrimitiveId),
    Colon {
        arity: u16,
        local_count: u16,
        parameter_ip: Address,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EntryContext {
    initial_bp: Address,
    frame_base: Address,
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

    pub fn is_dirty_execution_state(&self) -> bool {
        self.entry_context.is_some()
            || self.call_depth != 0
            || self.registers.rsp != self.return_stack_region.start()
    }

    pub fn reset_execution_state(&mut self) {
        self.registers.ip = None;
        self.registers.bp = DATA_STACK_START;
        self.registers.rsp = self.return_stack_region.start();
        self.call_depth = 0;
        self.entry_context = None;
        self.step_counter = 0;
        self.debug_validate_state();
    }

    pub fn resolve_xt(&self, xt: Cell) -> Result<ResolvedWord, Tbx16Error> {
        self.resolve_word(xt)
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
            let parameter_ip = xt_addr
                .checked_add(6)
                .ok_or_else(|| invalid_execution_token(xt))
                .and_then(validate_instruction_pointer_target)
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
            return Ok(ResolvedWord::Colon {
                arity,
                local_count,
                parameter_ip,
            });
        }

        Err(invalid_execution_token(xt))
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
        let frame = self.peek_return_frame()?;
        self.registers.rsp = self
            .registers
            .rsp
            .checked_sub(RETURN_FRAME_BYTES)
            .ok_or(Tbx16Error::ReturnStackUnderflow)?;
        Ok(frame)
    }

    pub fn run(&mut self, entry_xt: Cell) -> ExecutionOutcome {
        if self.is_dirty_execution_state() {
            return ExecutionOutcome::Trapped(Tbx16Error::DirtyExecutionState);
        }

        self.step_counter = 0;

        let outcome = (|| -> Result<ExecutionOutcome, Tbx16Error> {
            match self.start_entry(entry_xt)? {
                ExecutionState::Finished(outcome) => Ok(outcome),
                ExecutionState::Running => loop {
                    if let Some(outcome) = self.dispatch_step()? {
                        return Ok(outcome);
                    }
                },
            }
        })();

        match outcome {
            Ok(outcome) => outcome,
            Err(err) => ExecutionOutcome::Trapped(err),
        }
    }

    pub fn run_threaded(&mut self, start_ip: Address) -> ExecutionOutcome {
        self.step_counter = 0;
        if let Err(err) = self.set_instruction_pointer(start_ip) {
            return ExecutionOutcome::Trapped(err);
        }

        loop {
            let step = self.dispatch_step();
            match step {
                Ok(Some(outcome)) => return outcome,
                Ok(None) => {}
                Err(err) => return ExecutionOutcome::Trapped(err),
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
        let used_return_bytes = self.registers.rsp.get() - self.return_stack_region.start().get();
        let expected_return_bytes = self.call_depth.checked_mul(RETURN_FRAME_BYTES).ok_or(
            Tbx16Error::InvalidStackRegion {
                start: self.return_stack_region.start(),
                end: self.return_stack_region.end(),
                reason: "call depth overflowed return stack accounting",
            },
        )?;
        if used_return_bytes != expected_return_bytes {
            return Err(Tbx16Error::InvalidStackRegion {
                start: self.return_stack_region.start(),
                end: self.return_stack_region.end(),
                reason: "return stack usage does not match call depth",
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
        Ok(())
    }

    fn execute_entry_primitive(
        &mut self,
        primitive: PrimitiveId,
    ) -> Result<ExecutionOutcome, Tbx16Error> {
        match primitive {
            PrimitiveId::Halt => Ok(ExecutionOutcome::Halted),
            PrimitiveId::Lit | PrimitiveId::Branch | PrimitiveId::ZBranch => {
                self.execute_primitive(primitive)?;
                Ok(ExecutionOutcome::Returned)
            }
            PrimitiveId::Exit => Err(Tbx16Error::InvalidExecutionState),
        }
    }

    fn start_entry(&mut self, entry_xt: Cell) -> Result<ExecutionState, Tbx16Error> {
        self.begin_step()?;
        let resolved = self.resolve_word(entry_xt)?;
        self.step_counter += 1;

        match resolved {
            ResolvedWord::Primitive(primitive) => {
                let outcome = self.execute_entry_primitive(primitive)?;
                self.debug_validate_state();
                Ok(ExecutionState::Finished(outcome))
            }
            ResolvedWord::Colon {
                arity,
                local_count,
                parameter_ip,
            } => {
                let initial_bp = self.registers.bp;
                let frame_base = self.compute_frame_base(arity)?;
                let locals_start = self.registers.dsp;
                let new_dsp = self.checked_extend_data_stack(locals_start, local_count)?;

                if local_count != 0 {
                    self.memory
                        .zero_range(locals_start, usize::from(local_count) * 2)?;
                }
                self.entry_context = Some(EntryContext {
                    initial_bp,
                    frame_base,
                });
                self.registers.bp = frame_base;
                self.registers.dsp = new_dsp;
                self.registers.ip = Some(parameter_ip);
                self.debug_validate_state();
                Ok(ExecutionState::Running)
            }
        }
    }

    fn dispatch_step(&mut self) -> Result<Option<ExecutionOutcome>, Tbx16Error> {
        self.begin_step()?;

        let ip = self.current_ip()?;
        let xt = self.read_ip_cell(ip)?;
        let continuation_ip = validated_successor_ip(ip)?;
        self.step_counter += 1;

        match self.resolve_word(xt)? {
            ResolvedWord::Primitive(primitive) => {
                self.registers.ip = Some(ip);
                let outcome = self.execute_primitive_from_dispatch(primitive, continuation_ip)?;
                if outcome.is_none() {
                    self.debug_validate_state();
                }
                Ok(outcome)
            }
            ResolvedWord::Colon {
                arity,
                local_count,
                parameter_ip,
            } => {
                self.execute_colon_call(arity, local_count, parameter_ip, continuation_ip)?;
                self.debug_validate_state();
                Ok(None)
            }
        }
    }

    fn execute_primitive_from_dispatch(
        &mut self,
        primitive: PrimitiveId,
        continuation_ip: Address,
    ) -> Result<Option<ExecutionOutcome>, Tbx16Error> {
        match primitive {
            PrimitiveId::Halt => {
                self.registers.ip = Some(continuation_ip);
                Ok(Some(ExecutionOutcome::Halted))
            }
            PrimitiveId::Lit => {
                self.execute_lit_from_operand(continuation_ip)?;
                Ok(None)
            }
            PrimitiveId::Branch => {
                self.execute_branch_from_operand(continuation_ip)?;
                Ok(None)
            }
            PrimitiveId::ZBranch => {
                self.execute_zbranch_from_operand(continuation_ip)?;
                Ok(None)
            }
            PrimitiveId::Exit => self.execute_exit_from_operand(continuation_ip),
        }
    }

    fn execute_primitive(&mut self, primitive: PrimitiveId) -> Result<(), Tbx16Error> {
        match primitive {
            PrimitiveId::Lit => self.execute_lit_from_operand(self.current_ip()?),
            PrimitiveId::Branch => self.execute_branch_from_operand(self.current_ip()?),
            PrimitiveId::ZBranch => self.execute_zbranch_from_operand(self.current_ip()?),
            PrimitiveId::Halt => Ok(()),
            PrimitiveId::Exit => Err(Tbx16Error::InvalidExecutionState),
        }
    }

    fn execute_lit_from_operand(&mut self, operand_ip: Address) -> Result<(), Tbx16Error> {
        let literal = self.read_ip_cell(operand_ip)?;
        let next_ip = validated_successor_ip(operand_ip)?;
        self.ensure_data_stack_pushable(self.registers.dsp)?;
        self.push_data_cell(literal)?;
        self.registers.ip = Some(next_ip);
        Ok(())
    }

    fn execute_branch_from_operand(&mut self, operand_ip: Address) -> Result<(), Tbx16Error> {
        let target = self.read_ip_cell(operand_ip)?;
        let target_ip = validate_instruction_pointer_target(Address::new(target.raw()))?;
        self.registers.ip = Some(target_ip);
        Ok(())
    }

    fn execute_zbranch_from_operand(&mut self, operand_ip: Address) -> Result<(), Tbx16Error> {
        let target = self.read_ip_cell(operand_ip)?;
        let target_ip = validate_instruction_pointer_target(Address::new(target.raw()))?;
        let condition = self.peek_data_cell(0)?;
        let fallthrough_ip = validated_successor_ip(operand_ip)?;
        self.pop_data_cell()?;
        self.registers.ip = Some(if condition.raw() == 0 {
            target_ip
        } else {
            fallthrough_ip
        });
        Ok(())
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

    fn execute_colon_call(
        &mut self,
        arity: u16,
        local_count: u16,
        parameter_ip: Address,
        return_ip: Address,
    ) -> Result<(), Tbx16Error> {
        let new_bp = self.compute_frame_base(arity)?;
        let locals_start = self.registers.dsp;
        let new_dsp = self.checked_extend_data_stack(locals_start, local_count)?;
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
        if local_count != 0 {
            self.memory
                .zero_range(locals_start, usize::from(local_count) * 2)?;
        }

        self.registers.rsp = new_rsp;
        self.registers.bp = new_bp;
        self.registers.dsp = new_dsp;
        self.registers.ip = Some(parameter_ip);
        self.call_depth = self
            .call_depth
            .checked_add(1)
            .expect("call depth fits in configured return stack space");
        Ok(())
    }

    fn execute_exit_from_operand(
        &mut self,
        operand_ip: Address,
    ) -> Result<Option<ExecutionOutcome>, Tbx16Error> {
        let return_count = self.read_return_count(operand_ip)?;
        if self.call_depth > 0 {
            self.commit_nested_exit(return_count)?;
            return Ok(None);
        }

        let outcome = self.commit_top_level_exit(return_count)?;
        Ok(Some(outcome))
    }

    fn read_return_count(&self, operand_ip: Address) -> Result<u16, Tbx16Error> {
        let count = self.read_ip_cell(operand_ip)?;
        match count.raw() {
            0 | 1 => Ok(count.raw()),
            _ => Err(Tbx16Error::InvalidReturnCount { count }),
        }
    }

    fn read_exit_return_value(&self) -> Result<Cell, Tbx16Error> {
        if self.registers.dsp <= self.registers.bp {
            return Err(Tbx16Error::DataStackUnderflow);
        }
        self.peek_data_cell(0)
    }

    fn commit_nested_exit(&mut self, return_count: u16) -> Result<(), Tbx16Error> {
        let return_value = if return_count == 1 {
            Some(self.read_exit_return_value()?)
        } else {
            None
        };
        let frame = self.peek_return_frame()?;
        let return_ip = validate_instruction_pointer_target(frame.return_ip)?;
        self.validate_base_pointer(frame.caller_bp)?;

        let new_dsp = if return_count == 0 {
            self.registers.bp
        } else {
            self.checked_extend_data_stack(self.registers.bp, 1)?
        };
        if frame.caller_bp > new_dsp {
            return Err(Tbx16Error::InvalidExecutionState);
        }

        if let Some(value) = return_value {
            self.memory.write_cell(self.registers.bp, value)?;
        }
        self.registers.rsp = self
            .registers
            .rsp
            .checked_sub(RETURN_FRAME_BYTES)
            .ok_or(Tbx16Error::ReturnStackUnderflow)?;
        self.registers.bp = frame.caller_bp;
        self.registers.dsp = new_dsp;
        self.registers.ip = Some(return_ip);
        self.call_depth = self
            .call_depth
            .checked_sub(1)
            .expect("call depth is positive for nested exit");
        Ok(())
    }

    fn commit_top_level_exit(&mut self, return_count: u16) -> Result<ExecutionOutcome, Tbx16Error> {
        let return_value = if return_count == 1 {
            Some(self.read_exit_return_value()?)
        } else {
            None
        };
        let context = self
            .entry_context
            .ok_or(Tbx16Error::InvalidExecutionState)?;
        if self.registers.bp != context.frame_base {
            return Err(Tbx16Error::InvalidExecutionState);
        }
        if self.registers.rsp != self.return_stack_region.start() {
            return Err(Tbx16Error::InvalidExecutionState);
        }

        let new_dsp = if return_count == 0 {
            context.frame_base
        } else {
            self.checked_extend_data_stack(context.frame_base, 1)?
        };

        if let Some(value) = return_value {
            self.memory.write_cell(context.frame_base, value)?;
        }
        self.registers.dsp = new_dsp;
        self.registers.bp = context.initial_bp;
        self.registers.ip = None;
        self.call_depth = 0;
        self.entry_context = None;
        Ok(ExecutionOutcome::Returned)
    }

    fn validate_base_pointer(&self, bp: Address) -> Result<(), Tbx16Error> {
        if !self.data_stack_region.contains_pointer(bp)
            || ((bp.get() - self.data_stack_region.start().get()) % 2) != 0
        {
            return Err(Tbx16Error::InvalidExecutionState);
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

    fn debug_validate_state(&self) {
        #[cfg(debug_assertions)]
        self.validate_invariants()
            .expect("tbx16 invariants must hold after successful state transitions");
    }
}

enum ExecutionState {
    Running,
    Finished(ExecutionOutcome),
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

fn validated_successor_ip(ip: Address) -> Result<Address, Tbx16Error> {
    let next_ip = ip
        .checked_add(2)
        .ok_or(Tbx16Error::InstructionPointerOutOfRange { ip })?;
    validate_instruction_pointer_target(next_ip)
}

fn invalid_execution_token(xt: Cell) -> Tbx16Error {
    Tbx16Error::InvalidExecutionToken { xt }
}
