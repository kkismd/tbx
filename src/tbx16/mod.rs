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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveOperand {
    None,
    Cell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    pub id: PrimitiveId,
    pub name: &'static str,
    pub operand: PrimitiveOperand,
}

/// Primitive registry for the M2.3b threaded kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum PrimitiveId {
    Lit = 0,
    Branch = 1,
    ZBranch = 2,
    Halt = 3,
    Exit = 4,
    Dup = 16,
    Drop = 17,
    Swap = 18,
    Over = 19,
    Add = 32,
    Sub = 33,
    Mul = 34,
    Div = 35,
    Mod = 36,
    Negate = 37,
    Eq = 48,
    Ne = 49,
    Lt = 50,
    Le = 51,
    Gt = 52,
    Ge = 53,
    ToBool = 64,
    Not = 65,
    And = 66,
    Or = 67,
    Band = 68,
    Bor = 69,
    Fetch = 80,
    Store = 81,
    PutChr = 96,
    PutDec = 97,
    PutStr = 98,
}

impl PrimitiveId {
    pub const fn as_cell(self) -> Cell {
        Cell::new(self as u16)
    }

    pub const fn descriptor(self) -> &'static PrimitiveDescriptor {
        primitive_descriptor_by_id(self)
    }

    pub const fn name(self) -> &'static str {
        self.descriptor().name
    }

    pub const fn operand(self) -> PrimitiveOperand {
        self.descriptor().operand
    }

    pub fn from_name(name: &str) -> Option<Self> {
        primitive_descriptor_by_name(name).map(|descriptor| descriptor.id)
    }
}

impl TryFrom<Cell> for PrimitiveId {
    type Error = ();

    fn try_from(value: Cell) -> Result<Self, Self::Error> {
        match value.raw() {
            0 => Ok(Self::Lit),
            1 => Ok(Self::Branch),
            2 => Ok(Self::ZBranch),
            3 => Ok(Self::Halt),
            4 => Ok(Self::Exit),
            16 => Ok(Self::Dup),
            17 => Ok(Self::Drop),
            18 => Ok(Self::Swap),
            19 => Ok(Self::Over),
            32 => Ok(Self::Add),
            33 => Ok(Self::Sub),
            34 => Ok(Self::Mul),
            35 => Ok(Self::Div),
            36 => Ok(Self::Mod),
            37 => Ok(Self::Negate),
            48 => Ok(Self::Eq),
            49 => Ok(Self::Ne),
            50 => Ok(Self::Lt),
            51 => Ok(Self::Le),
            52 => Ok(Self::Gt),
            53 => Ok(Self::Ge),
            64 => Ok(Self::ToBool),
            65 => Ok(Self::Not),
            66 => Ok(Self::And),
            67 => Ok(Self::Or),
            68 => Ok(Self::Band),
            69 => Ok(Self::Bor),
            80 => Ok(Self::Fetch),
            81 => Ok(Self::Store),
            96 => Ok(Self::PutChr),
            97 => Ok(Self::PutDec),
            98 => Ok(Self::PutStr),
            _ => Err(()),
        }
    }
}

pub const PRIMITIVE_REGISTRY: &[PrimitiveDescriptor] = &[
    PrimitiveDescriptor {
        id: PrimitiveId::Lit,
        name: "LIT",
        operand: PrimitiveOperand::Cell,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Branch,
        name: "BRANCH",
        operand: PrimitiveOperand::Cell,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::ZBranch,
        name: "ZBRANCH",
        operand: PrimitiveOperand::Cell,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Halt,
        name: "HALT",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Exit,
        name: "EXIT",
        operand: PrimitiveOperand::Cell,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Dup,
        name: "DUP",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Drop,
        name: "DROP",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Swap,
        name: "SWAP",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Over,
        name: "OVER",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Add,
        name: "ADD",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Sub,
        name: "SUB",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Mul,
        name: "MUL",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Div,
        name: "DIV",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Mod,
        name: "MOD",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Negate,
        name: "NEGATE",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Eq,
        name: "EQ",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Ne,
        name: "NE",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Lt,
        name: "LT",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Le,
        name: "LE",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Gt,
        name: "GT",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Ge,
        name: "GE",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::ToBool,
        name: "TO_BOOL",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Not,
        name: "NOT",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::And,
        name: "AND",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Or,
        name: "OR",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Band,
        name: "BAND",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Bor,
        name: "BOR",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Fetch,
        name: "FETCH",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::Store,
        name: "STORE",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::PutChr,
        name: "PUTCHR",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::PutDec,
        name: "PUTDEC",
        operand: PrimitiveOperand::None,
    },
    PrimitiveDescriptor {
        id: PrimitiveId::PutStr,
        name: "PUTSTR",
        operand: PrimitiveOperand::None,
    },
];

pub const fn primitive_descriptor_by_id(id: PrimitiveId) -> &'static PrimitiveDescriptor {
    let mut index = 0;
    while index < PRIMITIVE_REGISTRY.len() {
        let descriptor = &PRIMITIVE_REGISTRY[index];
        if descriptor.id as u16 == id as u16 {
            return descriptor;
        }
        index += 1;
    }
    panic!("primitive registry missing descriptor")
}

pub fn primitive_descriptor_by_name(name: &str) -> Option<&'static PrimitiveDescriptor> {
    PRIMITIVE_REGISTRY
        .iter()
        .find(|descriptor| descriptor.name == name)
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
    output: Vec<u8>,
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
            output: Vec::new(),
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

    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    pub fn clear_output(&mut self) {
        self.output.clear();
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
            _ => {
                self.execute_primitive_no_operand(primitive)?;
                Ok(ExecutionOutcome::Returned)
            }
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
            _ => {
                self.execute_primitive_no_operand(primitive)?;
                self.registers.ip = Some(continuation_ip);
                Ok(None)
            }
        }
    }

    fn execute_primitive(&mut self, primitive: PrimitiveId) -> Result<(), Tbx16Error> {
        match primitive {
            PrimitiveId::Lit => self.execute_lit_from_operand(self.current_ip()?),
            PrimitiveId::Branch => self.execute_branch_from_operand(self.current_ip()?),
            PrimitiveId::ZBranch => self.execute_zbranch_from_operand(self.current_ip()?),
            PrimitiveId::Halt => Ok(()),
            PrimitiveId::Exit => Err(Tbx16Error::InvalidExecutionState),
            _ => self.execute_primitive_no_operand(primitive),
        }
    }

    fn execute_primitive_no_operand(&mut self, primitive: PrimitiveId) -> Result<(), Tbx16Error> {
        match primitive {
            PrimitiveId::Halt => Ok(()),
            PrimitiveId::Dup => self.execute_dup(),
            PrimitiveId::Drop => self.execute_drop(),
            PrimitiveId::Swap => self.execute_swap(),
            PrimitiveId::Over => self.execute_over(),
            PrimitiveId::Add => self.execute_add(),
            PrimitiveId::Sub => self.execute_sub(),
            PrimitiveId::Mul => self.execute_mul(),
            PrimitiveId::Div => self.execute_div(),
            PrimitiveId::Mod => self.execute_mod(),
            PrimitiveId::Negate => self.execute_negate(),
            PrimitiveId::Eq => self.execute_eq(),
            PrimitiveId::Ne => self.execute_ne(),
            PrimitiveId::Lt => self.execute_lt(),
            PrimitiveId::Le => self.execute_le(),
            PrimitiveId::Gt => self.execute_gt(),
            PrimitiveId::Ge => self.execute_ge(),
            PrimitiveId::ToBool => self.execute_to_bool(),
            PrimitiveId::Not => self.execute_not(),
            PrimitiveId::And => self.execute_and(),
            PrimitiveId::Or => self.execute_or(),
            PrimitiveId::Band => self.execute_band(),
            PrimitiveId::Bor => self.execute_bor(),
            PrimitiveId::Fetch => self.execute_fetch(),
            PrimitiveId::Store => self.execute_store(),
            PrimitiveId::PutChr => self.execute_putchr(),
            PrimitiveId::PutDec => self.execute_putdec(),
            PrimitiveId::PutStr => self.execute_putstr(),
            PrimitiveId::Lit | PrimitiveId::Branch | PrimitiveId::ZBranch | PrimitiveId::Exit => {
                Err(Tbx16Error::InvalidExecutionState)
            }
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

    fn execute_dup(&mut self) -> Result<(), Tbx16Error> {
        self.ensure_data_stack_pushable(self.registers.dsp)?;
        let value = self.peek_data_cell(0)?;
        self.push_data_cell(value)
    }

    fn execute_drop(&mut self) -> Result<(), Tbx16Error> {
        self.pop_data_cell()?;
        Ok(())
    }

    fn execute_swap(&mut self) -> Result<(), Tbx16Error> {
        let top = self.peek_data_cell(0)?;
        let below = self.peek_data_cell(1)?;
        let top_addr = self
            .registers
            .dsp
            .checked_sub(2)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        let below_addr = self
            .registers
            .dsp
            .checked_sub(4)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        self.memory.write_cell(top_addr, below)?;
        self.memory.write_cell(below_addr, top)?;
        Ok(())
    }

    fn execute_over(&mut self) -> Result<(), Tbx16Error> {
        self.ensure_data_stack_pushable(self.registers.dsp)?;
        let value = self.peek_data_cell(1)?;
        self.push_data_cell(value)
    }

    fn execute_add(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::new(lhs.raw().wrapping_add(rhs.raw())))
    }

    fn execute_sub(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::new(lhs.raw().wrapping_sub(rhs.raw())))
    }

    fn execute_mul(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| {
            Cell::from_i16(lhs.as_i16().wrapping_mul(rhs.as_i16()))
        })
    }

    fn execute_div(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_result(Self::checked_div)
    }

    fn execute_mod(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_result(Self::checked_mod)
    }

    fn execute_negate(&mut self) -> Result<(), Tbx16Error> {
        self.execute_unary_transform(|value| Cell::from_i16(value.as_i16().wrapping_neg()))
    }

    fn execute_eq(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::from_bool(lhs.raw() == rhs.raw()))
    }

    fn execute_ne(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::from_bool(lhs.raw() != rhs.raw()))
    }

    fn execute_lt(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::from_bool(lhs.as_i16() < rhs.as_i16()))
    }

    fn execute_le(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::from_bool(lhs.as_i16() <= rhs.as_i16()))
    }

    fn execute_gt(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::from_bool(lhs.as_i16() > rhs.as_i16()))
    }

    fn execute_ge(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::from_bool(lhs.as_i16() >= rhs.as_i16()))
    }

    fn execute_to_bool(&mut self) -> Result<(), Tbx16Error> {
        self.execute_unary_transform(Cell::to_canonical_bool)
    }

    fn execute_not(&mut self) -> Result<(), Tbx16Error> {
        self.execute_unary_transform(|value| Cell::from_bool(!value.is_truthy()))
    }

    fn execute_and(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| {
            Cell::from_bool(lhs.is_truthy() && rhs.is_truthy())
        })
    }

    fn execute_or(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| {
            Cell::from_bool(lhs.is_truthy() || rhs.is_truthy())
        })
    }

    fn execute_band(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::new(lhs.raw() & rhs.raw()))
    }

    fn execute_bor(&mut self) -> Result<(), Tbx16Error> {
        self.execute_binary_transform(|lhs, rhs| Cell::new(lhs.raw() | rhs.raw()))
    }

    fn execute_fetch(&mut self) -> Result<(), Tbx16Error> {
        let addr = Address::new(self.peek_data_cell(0)?.raw());
        let value = self.memory.read_cell(addr)?;
        let stack_addr = self
            .registers
            .dsp
            .checked_sub(2)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        self.memory.write_cell(stack_addr, value)?;
        Ok(())
    }

    fn execute_store(&mut self) -> Result<(), Tbx16Error> {
        let value = self.peek_data_cell(0)?;
        let addr = Address::new(self.peek_data_cell(1)?.raw());
        let new_dsp = self
            .registers
            .dsp
            .checked_sub(4)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        self.memory.write_cell(addr, value)?;
        self.registers.dsp = new_dsp;
        Ok(())
    }

    fn execute_putchr(&mut self) -> Result<(), Tbx16Error> {
        let value = self.peek_data_cell(0)?;
        self.output.push(value.raw() as u8);
        self.pop_data_cell()?;
        Ok(())
    }

    fn execute_putdec(&mut self) -> Result<(), Tbx16Error> {
        let value = self.peek_data_cell(0)?;
        let rendered = value.as_i16().to_string();
        self.output.extend_from_slice(rendered.as_bytes());
        self.pop_data_cell()?;
        Ok(())
    }

    fn execute_putstr(&mut self) -> Result<(), Tbx16Error> {
        let addr = Address::new(self.peek_data_cell(0)?.raw());
        let bytes = self.read_length_prefixed_bytes(addr)?;
        self.output.extend_from_slice(&bytes);
        self.pop_data_cell()?;
        Ok(())
    }

    fn execute_unary_transform(
        &mut self,
        transform: impl FnOnce(Cell) -> Cell,
    ) -> Result<(), Tbx16Error> {
        let value = self.peek_data_cell(0)?;
        let addr = self
            .registers
            .dsp
            .checked_sub(2)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        self.memory.write_cell(addr, transform(value))?;
        Ok(())
    }

    fn execute_binary_transform(
        &mut self,
        transform: impl FnOnce(Cell, Cell) -> Cell,
    ) -> Result<(), Tbx16Error> {
        let rhs = self.peek_data_cell(0)?;
        let lhs = self.peek_data_cell(1)?;
        let result = transform(lhs, rhs);
        let dst_addr = self
            .registers
            .dsp
            .checked_sub(4)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        let new_dsp = self
            .registers
            .dsp
            .checked_sub(2)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        self.memory.write_cell(dst_addr, result)?;
        self.registers.dsp = new_dsp;
        Ok(())
    }

    fn execute_binary_result(
        &mut self,
        transform: impl FnOnce(Cell, Cell) -> Result<Cell, Tbx16Error>,
    ) -> Result<(), Tbx16Error> {
        let rhs = self.peek_data_cell(0)?;
        let lhs = self.peek_data_cell(1)?;
        let result = transform(lhs, rhs)?;
        let dst_addr = self
            .registers
            .dsp
            .checked_sub(4)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        let new_dsp = self
            .registers
            .dsp
            .checked_sub(2)
            .ok_or(Tbx16Error::DataStackUnderflow)?;
        self.memory.write_cell(dst_addr, result)?;
        self.registers.dsp = new_dsp;
        Ok(())
    }

    fn checked_div(lhs: Cell, rhs: Cell) -> Result<Cell, Tbx16Error> {
        let rhs = rhs.as_i16();
        if rhs == 0 {
            return Err(Tbx16Error::DivisionByZero);
        }
        let lhs = lhs.as_i16();
        if lhs == i16::MIN && rhs == -1 {
            return Ok(Cell::from_i16(i16::MIN));
        }
        Ok(Cell::from_i16(lhs / rhs))
    }

    fn checked_mod(lhs: Cell, rhs: Cell) -> Result<Cell, Tbx16Error> {
        let rhs = rhs.as_i16();
        if rhs == 0 {
            return Err(Tbx16Error::DivisionByZero);
        }
        let lhs = lhs.as_i16();
        if lhs == i16::MIN && rhs == -1 {
            return Ok(Cell::new(0));
        }
        Ok(Cell::from_i16(lhs % rhs))
    }

    fn read_length_prefixed_bytes(&self, addr: Address) -> Result<Vec<u8>, Tbx16Error> {
        let len = usize::from(self.memory.read_cell(addr)?.raw());
        let payload_start =
            usize::from(addr.get())
                .checked_add(2)
                .ok_or(Tbx16Error::InvalidMemoryAccess {
                    addr,
                    operation: "string read",
                })?;
        payload_start
            .checked_add(len)
            .filter(|end| *end <= memory::MEMORY_SIZE)
            .ok_or(Tbx16Error::InvalidMemoryAccess {
                addr,
                operation: "string read",
            })?;
        let mut bytes = Vec::with_capacity(len);
        for offset in 0..len {
            let byte_addr = u16::try_from(payload_start + offset)
                .map(Address::new)
                .map_err(|_| Tbx16Error::InvalidMemoryAccess {
                    addr,
                    operation: "string read",
                })?;
            bytes.push(self.memory.read_byte(byte_addr)?);
        }
        Ok(bytes)
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
