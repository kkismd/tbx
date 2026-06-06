use crate::tbx16::cell::{
    canonical_bool, cell_from_i16, cell_to_i16, Cell, FALSE, TRUE, WORD_BYTES,
};
use crate::tbx16::dict::{Instr, Primitive, Program, ReturnMode, StringId, UserWord, Word, WordId};

pub const DEFAULT_DATA_STACK_LIMIT: usize = 64;
pub const DEFAULT_RETURN_STACK_LIMIT: usize = 64;
const DEFAULT_MEMORY_SIZE: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trap {
    InvalidEntryWord(WordId),
    InvalidWord(WordId),
    InvalidString(StringId),
    InvalidBranchTarget { word: WordId, target: usize },
    InvalidLocalSlot { word: WordId, slot: u8 },
    StackUnderflow,
    StackOverflow,
    ReturnStackOverflow,
    DivisionByZero,
    InvalidMemoryAddress(u16),
    MissingExit(WordId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallFrame {
    pub word: WordId,
    pub pc: usize,
    pub locals: Vec<Cell>,
    pub return_mode: ReturnMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RunningWord {
    id: WordId,
    locals: Vec<Cell>,
    return_mode: ReturnMode,
    pc: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vm {
    memory: Vec<u8>,
    data_stack: Vec<Cell>,
    return_stack: Vec<CallFrame>,
    output: Vec<u8>,
    data_stack_limit: usize,
    return_stack_limit: usize,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    #[must_use]
    pub fn new() -> Self {
        Self::with_memory_size(DEFAULT_MEMORY_SIZE)
    }

    #[must_use]
    pub fn with_memory_size(memory_size: usize) -> Self {
        Self {
            memory: vec![0; memory_size],
            data_stack: Vec::with_capacity(DEFAULT_DATA_STACK_LIMIT),
            return_stack: Vec::with_capacity(DEFAULT_RETURN_STACK_LIMIT),
            output: Vec::new(),
            data_stack_limit: DEFAULT_DATA_STACK_LIMIT,
            return_stack_limit: DEFAULT_RETURN_STACK_LIMIT,
        }
    }

    #[must_use]
    pub fn with_limits(
        memory_size: usize,
        data_stack_limit: usize,
        return_stack_limit: usize,
    ) -> Self {
        Self {
            memory: vec![0; memory_size],
            data_stack: Vec::with_capacity(data_stack_limit),
            return_stack: Vec::with_capacity(return_stack_limit),
            output: Vec::new(),
            data_stack_limit,
            return_stack_limit,
        }
    }

    pub fn run(&mut self, program: &Program, entry: WordId) -> Result<(), Trap> {
        let entry_word = program.word(entry).ok_or(Trap::InvalidEntryWord(entry))?;
        let entry_user = match entry_word {
            Word::User(word) => word,
            Word::Primitive(_) => return Err(Trap::InvalidEntryWord(entry)),
        };

        let mut current = RunningWord {
            id: entry,
            locals: vec![0; usize::from(entry_user.frame_slots)],
            return_mode: ReturnMode::Void,
            pc: 0,
        };

        loop {
            let word = program
                .word(current.id)
                .ok_or(Trap::InvalidWord(current.id))?;
            let user_word = match word {
                Word::User(word) => word,
                Word::Primitive(_) => return Err(Trap::InvalidWord(current.id)),
            };

            let instr = user_word
                .code
                .get(current.pc)
                .ok_or(Trap::MissingExit(current.id))?
                .clone();
            current.pc += 1;

            match instr {
                Instr::Call(word_id) => {
                    self.invoke(program, word_id, &mut current)?;
                }
                Instr::Lit(value) => self.push(value)?,
                Instr::Branch(target) => {
                    self.ensure_branch_target(user_word, current.id, target)?;
                    current.pc = target;
                }
                Instr::BranchIfZero(target) => {
                    let value = self.pop()?;
                    if value == FALSE {
                        self.ensure_branch_target(user_word, current.id, target)?;
                        current.pc = target;
                    }
                }
                Instr::BranchIfNonZero(target) => {
                    let value = self.pop()?;
                    if value != FALSE {
                        self.ensure_branch_target(user_word, current.id, target)?;
                        current.pc = target;
                    }
                }
                Instr::LoadLocal(slot) => {
                    let value =
                        *current
                            .locals
                            .get(usize::from(slot))
                            .ok_or(Trap::InvalidLocalSlot {
                                word: current.id,
                                slot,
                            })?;
                    self.push(value)?;
                }
                Instr::StoreLocal(slot) => {
                    let value = self.pop()?;
                    let local = current.locals.get_mut(usize::from(slot)).ok_or(
                        Trap::InvalidLocalSlot {
                            word: current.id,
                            slot,
                        },
                    )?;
                    *local = value;
                }
                Instr::Exit => {
                    if let Some(frame) = self.return_stack.pop() {
                        let return_value = match current.return_mode {
                            ReturnMode::Void => None,
                            ReturnMode::Value => Some(self.pop()?),
                        };
                        current = RunningWord {
                            id: frame.word,
                            locals: frame.locals,
                            return_mode: frame.return_mode,
                            pc: frame.pc,
                        };
                        if let Some(value) = return_value {
                            self.push(value)?;
                        }
                    } else {
                        return Ok(());
                    }
                }
                Instr::Halt => return Ok(()),
            }
        }
    }

    #[must_use]
    pub fn data_stack(&self) -> &[Cell] {
        &self.data_stack
    }

    #[must_use]
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    fn invoke(
        &mut self,
        program: &Program,
        word_id: WordId,
        current: &mut RunningWord,
    ) -> Result<(), Trap> {
        let word = program.word(word_id).ok_or(Trap::InvalidWord(word_id))?;
        match word {
            Word::Primitive(primitive) => self.exec_primitive(program, *primitive),
            Word::User(user_word) => self.enter_word(word_id, user_word, current),
        }
    }

    fn enter_word(
        &mut self,
        word_id: WordId,
        user_word: &UserWord,
        current: &mut RunningWord,
    ) -> Result<(), Trap> {
        if self.return_stack.len() >= self.return_stack_limit {
            return Err(Trap::ReturnStackOverflow);
        }

        let arity = usize::from(user_word.arity);
        if self.data_stack.len() < arity {
            return Err(Trap::StackUnderflow);
        }

        let frame_len = usize::from(user_word.frame_slots);
        let base = self.data_stack.len() - arity;
        let args = self.data_stack.split_off(base);
        let mut locals = vec![0; frame_len];
        locals[..arity].copy_from_slice(&args);

        self.return_stack.push(CallFrame {
            word: current.id,
            pc: current.pc,
            locals: std::mem::take(&mut current.locals),
            return_mode: current.return_mode,
        });

        *current = RunningWord {
            id: word_id,
            locals,
            return_mode: user_word.return_mode,
            pc: 0,
        };
        Ok(())
    }

    fn ensure_branch_target(
        &self,
        word: &UserWord,
        word_id: WordId,
        target: usize,
    ) -> Result<(), Trap> {
        if target >= word.code.len() {
            return Err(Trap::InvalidBranchTarget {
                word: word_id,
                target,
            });
        }
        Ok(())
    }

    fn exec_primitive(&mut self, program: &Program, primitive: Primitive) -> Result<(), Trap> {
        match primitive {
            Primitive::Dup => {
                let value = *self.data_stack.last().ok_or(Trap::StackUnderflow)?;
                self.push(value)
            }
            Primitive::Drop => {
                self.pop()?;
                Ok(())
            }
            Primitive::Swap => {
                let len = self.data_stack.len();
                if len < 2 {
                    return Err(Trap::StackUnderflow);
                }
                self.data_stack.swap(len - 1, len - 2);
                Ok(())
            }
            Primitive::Over => {
                let len = self.data_stack.len();
                if len < 2 {
                    return Err(Trap::StackUnderflow);
                }
                let value = self.data_stack[len - 2];
                self.push(value)
            }
            Primitive::Add => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(lhs.wrapping_add(rhs))
            }
            Primitive::Sub => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(lhs.wrapping_sub(rhs))
            }
            Primitive::Mul => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(lhs.wrapping_mul(rhs))
            }
            Primitive::Div => {
                let rhs = cell_to_i16(self.pop()?);
                let lhs = cell_to_i16(self.pop()?);
                let value = signed_div(lhs, rhs)?;
                self.push(cell_from_i16(value))
            }
            Primitive::Mod => {
                let rhs = cell_to_i16(self.pop()?);
                let lhs = cell_to_i16(self.pop()?);
                let value = signed_mod(lhs, rhs)?;
                self.push(cell_from_i16(value))
            }
            Primitive::Negate => {
                let value = self.pop()?;
                let signed = cell_to_i16(value);
                self.push(cell_from_i16(signed.wrapping_neg()))
            }
            Primitive::Eq => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(bool_cell(lhs == rhs))
            }
            Primitive::Ne => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(bool_cell(lhs != rhs))
            }
            Primitive::Lt => {
                let rhs = cell_to_i16(self.pop()?);
                let lhs = cell_to_i16(self.pop()?);
                self.push(bool_cell(lhs < rhs))
            }
            Primitive::Le => {
                let rhs = cell_to_i16(self.pop()?);
                let lhs = cell_to_i16(self.pop()?);
                self.push(bool_cell(lhs <= rhs))
            }
            Primitive::Gt => {
                let rhs = cell_to_i16(self.pop()?);
                let lhs = cell_to_i16(self.pop()?);
                self.push(bool_cell(lhs > rhs))
            }
            Primitive::Ge => {
                let rhs = cell_to_i16(self.pop()?);
                let lhs = cell_to_i16(self.pop()?);
                self.push(bool_cell(lhs >= rhs))
            }
            Primitive::Not => {
                let value = self.pop()?;
                self.push(bool_cell(value == FALSE))
            }
            Primitive::And => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(canonical_bool(lhs) & canonical_bool(rhs))
            }
            Primitive::Or => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(canonical_bool(lhs) | canonical_bool(rhs))
            }
            Primitive::BitAnd => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(lhs & rhs)
            }
            Primitive::BitOr => {
                let rhs = self.pop()?;
                let lhs = self.pop()?;
                self.push(lhs | rhs)
            }
            Primitive::Fetch => {
                let addr = self.pop()?;
                let value = self.read_cell(addr)?;
                self.push(value)
            }
            Primitive::Store => {
                let value = self.pop()?;
                let addr = self.pop()?;
                self.write_cell(addr, value)
            }
            Primitive::PutDec => {
                let value = cell_to_i16(self.pop()?);
                self.output.extend_from_slice(value.to_string().as_bytes());
                Ok(())
            }
            Primitive::PutChr => {
                let value = self.pop()?;
                self.output.push(value.to_le_bytes()[0]);
                Ok(())
            }
            Primitive::PutStr => {
                let string_id = StringId(self.pop()?);
                let bytes = program
                    .string(string_id)
                    .ok_or(Trap::InvalidString(string_id))?;
                self.output.extend_from_slice(bytes);
                Ok(())
            }
        }
    }

    fn push(&mut self, value: Cell) -> Result<(), Trap> {
        if self.data_stack.len() >= self.data_stack_limit {
            return Err(Trap::StackOverflow);
        }
        self.data_stack.push(value);
        Ok(())
    }

    fn pop(&mut self) -> Result<Cell, Trap> {
        self.data_stack.pop().ok_or(Trap::StackUnderflow)
    }

    fn read_cell(&self, addr: u16) -> Result<Cell, Trap> {
        let index = usize::from(addr);
        let Some(end) = index.checked_add(WORD_BYTES - 1) else {
            return Err(Trap::InvalidMemoryAddress(addr));
        };
        if end >= self.memory.len() {
            return Err(Trap::InvalidMemoryAddress(addr));
        }
        Ok(u16::from_le_bytes([
            self.memory[index],
            self.memory[index + 1],
        ]))
    }

    fn write_cell(&mut self, addr: u16, value: Cell) -> Result<(), Trap> {
        let index = usize::from(addr);
        let Some(end) = index.checked_add(WORD_BYTES - 1) else {
            return Err(Trap::InvalidMemoryAddress(addr));
        };
        if end >= self.memory.len() {
            return Err(Trap::InvalidMemoryAddress(addr));
        }
        let [lo, hi] = value.to_le_bytes();
        self.memory[index] = lo;
        self.memory[index + 1] = hi;
        Ok(())
    }
}

fn bool_cell(value: bool) -> Cell {
    if value {
        TRUE
    } else {
        FALSE
    }
}

fn signed_div(lhs: i16, rhs: i16) -> Result<i16, Trap> {
    if rhs == 0 {
        return Err(Trap::DivisionByZero);
    }
    if lhs == i16::MIN && rhs == -1 {
        return Ok(i16::MIN);
    }
    Ok(lhs / rhs)
}

fn signed_mod(lhs: i16, rhs: i16) -> Result<i16, Trap> {
    if rhs == 0 {
        return Err(Trap::DivisionByZero);
    }
    if lhs == i16::MIN && rhs == -1 {
        return Ok(0);
    }
    Ok(lhs % rhs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tbx16::dict::{CoreWords, ProgramError, ReturnMode};

    fn install_program() -> Result<(Program, CoreWords), ProgramError> {
        let mut program = Program::new();
        let core = program.install_core_words()?;
        Ok((program, core))
    }

    #[test]
    fn arithmetic_wraps_and_signed_division_follow_profile() {
        let (mut program, core) = install_program().expect("core words");
        let entry = program
            .add_word(Word::User(UserWord::new(
                0,
                0,
                ReturnMode::Void,
                vec![
                    Instr::Lit(0x7fff),
                    Instr::Lit(1),
                    Instr::Call(core.add),
                    Instr::Lit(cell_from_i16(-7)),
                    Instr::Lit(3),
                    Instr::Call(core.div),
                    Instr::Lit(cell_from_i16(-7)),
                    Instr::Lit(3),
                    Instr::Call(core.modulo),
                    Instr::Lit(cell_from_i16(i16::MIN)),
                    Instr::Lit(cell_from_i16(-1)),
                    Instr::Call(core.div),
                    Instr::Halt,
                ],
            )))
            .expect("entry word");

        let mut vm = Vm::new();
        vm.run(&program, entry).expect("program should run");

        assert_eq!(
            vm.data_stack(),
            &[
                cell_from_i16(i16::MIN),
                cell_from_i16(-2),
                cell_from_i16(-1),
                cell_from_i16(i16::MIN),
            ]
        );
    }

    #[test]
    fn logical_ops_normalize_to_canonical_booleans_and_branch_on_zero() {
        let (mut program, core) = install_program().expect("core words");
        let entry = program
            .add_word(Word::User(UserWord::new(
                0,
                0,
                ReturnMode::Void,
                vec![
                    Instr::Lit(5),
                    Instr::BranchIfZero(4),
                    Instr::Lit(1),
                    Instr::Branch(5),
                    Instr::Lit(2),
                    Instr::Lit(5),
                    Instr::Call(core.not),
                    Instr::Lit(1),
                    Instr::Lit(2),
                    Instr::Call(core.and),
                    Instr::Lit(0),
                    Instr::Lit(9),
                    Instr::Call(core.or),
                    Instr::Halt,
                ],
            )))
            .expect("entry word");

        let mut vm = Vm::new();
        vm.run(&program, entry).expect("program should run");

        assert_eq!(vm.data_stack(), &[1, FALSE, TRUE, TRUE]);
    }

    #[test]
    fn fetch_and_store_use_little_endian_cells_on_byte_addresses() {
        let (mut program, core) = install_program().expect("core words");
        let entry = program
            .add_word(Word::User(UserWord::new(
                0,
                0,
                ReturnMode::Void,
                vec![
                    Instr::Lit(1),
                    Instr::Lit(0x1234),
                    Instr::Call(core.store),
                    Instr::Lit(1),
                    Instr::Call(core.fetch),
                    Instr::Halt,
                ],
            )))
            .expect("entry word");

        let mut vm = Vm::with_memory_size(8);
        vm.run(&program, entry).expect("program should run");

        assert_eq!(vm.memory()[1], 0x34);
        assert_eq!(vm.memory()[2], 0x12);
        assert_eq!(vm.data_stack(), &[0x1234]);
    }

    #[test]
    fn user_words_can_read_arguments_and_locals_and_return_one_value() {
        let (mut program, core) = install_program().expect("core words");
        let doubler = program
            .add_word(Word::User(UserWord::new(
                2,
                3,
                ReturnMode::Value,
                vec![
                    Instr::LoadLocal(0),
                    Instr::LoadLocal(1),
                    Instr::Call(core.add),
                    Instr::StoreLocal(2),
                    Instr::LoadLocal(2),
                    Instr::LoadLocal(2),
                    Instr::Call(core.add),
                    Instr::Exit,
                ],
            )))
            .expect("helper word");
        let entry = program
            .add_word(Word::User(UserWord::new(
                0,
                0,
                ReturnMode::Void,
                vec![
                    Instr::Lit(10),
                    Instr::Lit(7),
                    Instr::Call(doubler),
                    Instr::Halt,
                ],
            )))
            .expect("entry word");

        let mut vm = Vm::new();
        vm.run(&program, entry).expect("program should run");

        assert_eq!(vm.data_stack(), &[34]);
    }

    #[test]
    fn output_words_emit_decimal_char_and_static_string_bytes() {
        let (mut program, core) = install_program().expect("core words");
        let hello = program.add_string(b"HI".to_vec()).expect("string");
        let entry = program
            .add_word(Word::User(UserWord::new(
                0,
                0,
                ReturnMode::Void,
                vec![
                    Instr::Lit(hello.0),
                    Instr::Call(core.putstr),
                    Instr::Lit(u16::from(b' ')),
                    Instr::Call(core.putchr),
                    Instr::Lit(cell_from_i16(-2)),
                    Instr::Call(core.putdec),
                    Instr::Halt,
                ],
            )))
            .expect("entry word");

        let mut vm = Vm::new();
        vm.run(&program, entry).expect("program should run");

        assert_eq!(vm.output(), b"HI -2");
    }

    #[test]
    fn division_by_zero_traps_deterministically() {
        let (mut program, core) = install_program().expect("core words");
        let entry = program
            .add_word(Word::User(UserWord::new(
                0,
                0,
                ReturnMode::Void,
                vec![
                    Instr::Lit(1),
                    Instr::Lit(0),
                    Instr::Call(core.div),
                    Instr::Halt,
                ],
            )))
            .expect("entry word");

        let mut vm = Vm::new();
        let err = vm.run(&program, entry).expect_err("division should trap");

        assert_eq!(err, Trap::DivisionByZero);
    }
}
