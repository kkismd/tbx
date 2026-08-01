use crate::instruction::{
    Instruction, InstructionAddress, InstructionAddressError, InstructionView,
};
#[cfg(test)]
use crate::stack::ReturnFrame;
use crate::stack::{DataStack, ReturnStack, StackError};
use crate::value::Value;

/// Mutable execution state for the initial TBX Next VM core.
///
/// The VM owns only mutable control/data state. It does not own the shared
/// instruction sequence, word registry, bindings, or any builder/publication
/// surface. Callers pass `InstructionView` to execution methods so the VM can
/// fetch and validate instructions without gaining append or mutation access.
#[derive(Debug)]
pub(crate) struct Vm {
    instruction_pointer: InstructionAddress,
    data_stack: DataStack,
    return_stack: ReturnStack,
    halted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepOutcome {
    Continued,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmError {
    address: InstructionAddress,
    kind: VmErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmErrorKind {
    InstructionFetch { source: InstructionAddressError },
    UnexpectedEndOfCode { source: InstructionAddressError },
    DataStackUnderflow { source: StackError },
    ReturnStackUnderflow { source: StackError },
    InvalidJumpTarget { source: InstructionAddressError },
    InvalidReturnTarget { source: InstructionAddressError },
}

impl Vm {
    pub(crate) fn new(
        instructions: InstructionView<'_>,
        entry: InstructionAddress,
    ) -> Result<Self, VmError> {
        instructions
            .validate_address(entry)
            .map_err(|source| VmError {
                address: entry,
                kind: VmErrorKind::InstructionFetch { source },
            })?;

        Ok(Self {
            instruction_pointer: entry,
            data_stack: DataStack::new(),
            return_stack: ReturnStack::new(),
            halted: false,
        })
    }

    pub(crate) fn step(
        &mut self,
        instructions: InstructionView<'_>,
    ) -> Result<StepOutcome, VmError> {
        if self.halted {
            return Ok(StepOutcome::Halted);
        }

        let address = self.instruction_pointer;
        let instruction = *instructions.get(address).map_err(|source| VmError {
            address,
            kind: VmErrorKind::InstructionFetch { source },
        })?;

        match instruction {
            Instruction::Push(value) => self.step_push(instructions, address, value),
            Instruction::Jump(target) => self.step_jump(instructions, address, target),
            Instruction::JumpIfZero(target) => {
                self.step_jump_if_zero(instructions, address, target)
            }
            Instruction::Return => self.step_return(instructions, address),
            Instruction::Halt => {
                self.halted = true;
                Ok(StepOutcome::Halted)
            }
        }
    }

    pub(crate) fn run(&mut self, instructions: InstructionView<'_>) -> Result<RunOutcome, VmError> {
        loop {
            match self.step(instructions)? {
                StepOutcome::Continued => {}
                StepOutcome::Halted => return Ok(RunOutcome::Halted),
            }
        }
    }

    pub(crate) const fn instruction_pointer(&self) -> InstructionAddress {
        self.instruction_pointer
    }

    pub(crate) const fn is_halted(&self) -> bool {
        self.halted
    }

    pub(crate) fn data_stack_depth(&self) -> usize {
        self.data_stack.depth()
    }

    pub(crate) fn return_stack_depth(&self) -> usize {
        self.return_stack.depth()
    }

    #[cfg(test)]
    fn push_return_frame(&mut self, frame: ReturnFrame) {
        self.return_stack.push(frame);
    }

    pub(crate) fn peek_data(&self) -> Result<Value, StackError> {
        self.data_stack.peek()
    }

    pub(crate) fn pop_data(&mut self) -> Result<Value, StackError> {
        self.data_stack.pop()
    }

    fn step_push(
        &mut self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
        value: Value,
    ) -> Result<StepOutcome, VmError> {
        let next = self.valid_next_address(instructions, address)?;

        self.data_stack.push(value);
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_jump(
        &mut self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<StepOutcome, VmError> {
        let target = self.valid_jump_target(instructions, address, target)?;

        self.instruction_pointer = target;

        Ok(StepOutcome::Continued)
    }

    fn step_jump_if_zero(
        &mut self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<StepOutcome, VmError> {
        self.data_stack.require_depth(1).map_err(|source| VmError {
            address,
            kind: VmErrorKind::DataStackUnderflow { source },
        })?;

        let condition = self
            .data_stack
            .peek()
            .expect("depth was checked before reading JumpIfZero condition");
        let next = if condition.is_zero() {
            self.valid_jump_target(instructions, address, target)?
        } else {
            self.valid_next_address(instructions, address)?
        };

        self.data_stack
            .pop()
            .expect("depth was checked before consuming JumpIfZero condition");
        self.instruction_pointer = next;

        Ok(StepOutcome::Continued)
    }

    fn step_return(
        &mut self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
    ) -> Result<StepOutcome, VmError> {
        let frame = self.return_stack.peek().map_err(|source| VmError {
            address,
            kind: VmErrorKind::ReturnStackUnderflow { source },
        })?;
        let target = self.valid_return_target(instructions, address, frame.return_address())?;

        self.return_stack
            .pop()
            .expect("return frame was checked before consuming Return frame");
        self.instruction_pointer = target;

        Ok(StepOutcome::Continued)
    }

    fn valid_next_address(
        &self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
    ) -> Result<InstructionAddress, VmError> {
        instructions
            .checked_next_address(address)
            .map_err(|source| VmError {
                address,
                kind: VmErrorKind::UnexpectedEndOfCode { source },
            })
    }

    fn valid_jump_target(
        &self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<InstructionAddress, VmError> {
        instructions
            .validate_address(target)
            .map_err(|source| VmError {
                address,
                kind: VmErrorKind::InvalidJumpTarget { source },
            })
    }

    fn valid_return_target(
        &self,
        instructions: InstructionView<'_>,
        address: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<InstructionAddress, VmError> {
        instructions
            .validate_address(target)
            .map_err(|source| VmError {
                address,
                kind: VmErrorKind::InvalidReturnTarget { source },
            })
    }
}

impl VmError {
    pub(crate) const fn address(self) -> InstructionAddress {
        self.address
    }

    pub(crate) const fn kind(self) -> VmErrorKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::InstructionSequence;

    fn value(value: i16) -> Value {
        Value::integer(value)
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    fn new_vm(code: &InstructionSequence, entry: InstructionAddress) -> Vm {
        Vm::new(code.view(), entry).expect("test entry should be valid")
    }

    fn assert_clean_control(vm: &Vm, expected_ip: InstructionAddress, halted: bool) {
        assert_eq!(vm.instruction_pointer(), expected_ip);
        assert_eq!(vm.is_halted(), halted);
        assert_eq!(vm.return_stack_depth(), 0);
    }

    fn return_frame(return_address: InstructionAddress) -> ReturnFrame {
        ReturnFrame::new(return_address)
    }

    #[test]
    fn new_rejects_invalid_initial_instruction_pointer() {
        let code = InstructionSequence::new();
        let entry = address(0);

        assert_eq!(
            Vm::new(code.view(), entry).expect_err("empty code should reject entry"),
            VmError {
                address: entry,
                kind: VmErrorKind::InstructionFetch {
                    source: InstructionAddressError::EndAddress { address: entry }
                }
            }
        );
    }

    #[test]
    fn push_step_stores_value_and_advances_to_existing_next_instruction() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(10)));
        let next = code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_clean_control(&vm, next, false);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(10)));
    }

    #[test]
    fn multiple_pushes_preserve_order() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(1)));
        code.append(Instruction::Push(value(2)));
        code.append(Instruction::Push(value(3)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_eq!(vm.pop_data(), Ok(value(3)));
        assert_eq!(vm.pop_data(), Ok(value(2)));
        assert_eq!(vm.pop_data(), Ok(value(1)));
    }

    #[test]
    fn push_at_end_reports_unexpected_end_without_mutation() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(7)));
        let mut vm = new_vm(&code, entry);

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::UnexpectedEndOfCode {
                    source: InstructionAddressError::EndAddress {
                        address: address(1)
                    }
                }
            })
        );
        assert_clean_control(&vm, entry, false);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn step_rejects_invalid_current_instruction_pointer_without_mutation() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let other = InstructionSequence::new();
        let mut vm = new_vm(&code, entry);

        let result = vm.step(other.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::InstructionFetch {
                    source: InstructionAddressError::EndAddress { address: entry }
                }
            })
        );
        assert_clean_control(&vm, entry, false);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn jump_moves_to_valid_target() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let jump = code.append(Instruction::Jump(entry));
        let mut vm = new_vm(&code, jump);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_clean_control(&vm, entry, false);
    }

    #[test]
    fn jump_can_move_backward_in_one_step() {
        let mut code = InstructionSequence::new();
        let target = code.append(Instruction::Halt);
        let entry = code.append(Instruction::Jump(target));
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_eq!(vm.instruction_pointer(), target);
    }

    #[test]
    fn jump_rejects_invalid_target_without_mutation() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Jump(address(10)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::InvalidJumpTarget {
                    source: InstructionAddressError::InvalidAddress {
                        address: address(10)
                    }
                }
            })
        );
        assert_clean_control(&vm, entry, false);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn jump_rejects_end_target_without_mutation() {
        let mut code = InstructionSequence::new();
        let end = address(1);
        let entry = code.append(Instruction::Jump(end));
        let mut vm = new_vm(&code, entry);

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::InvalidJumpTarget {
                    source: InstructionAddressError::EndAddress { address: end }
                }
            })
        );
        assert_clean_control(&vm, entry, false);
    }

    #[test]
    fn jump_if_zero_takes_target_and_consumes_condition() {
        let mut code = InstructionSequence::new();
        let push = code.append(Instruction::Push(value(0)));
        let branch = code.append(Instruction::JumpIfZero(address(3)));
        code.append(Instruction::Halt);
        let target = code.append(Instruction::Halt);
        let mut vm = new_vm(&code, push);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_clean_control(&vm, target, false);
        assert_eq!(vm.data_stack_depth(), 0);
        assert_eq!(branch.as_index(), 1);
    }

    #[test]
    fn jump_if_zero_falls_through_on_non_zero_and_consumes_condition() {
        let mut code = InstructionSequence::new();
        let push = code.append(Instruction::Push(value(1)));
        let branch = code.append(Instruction::JumpIfZero(address(3)));
        let next = code.append(Instruction::Halt);
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, push);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_clean_control(&vm, next, false);
        assert_eq!(vm.data_stack_depth(), 0);
        assert_eq!(branch.as_index(), 1);
    }

    #[test]
    fn jump_if_zero_underflow_preserves_state() {
        let mut code = InstructionSequence::new();
        let target = code.append(Instruction::Halt);
        let entry = code.append(Instruction::JumpIfZero(target));
        let mut vm = new_vm(&code, entry);

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::DataStackUnderflow {
                    source: StackError::DataStackUnderflow
                }
            })
        );
        assert_clean_control(&vm, entry, false);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn jump_if_zero_invalid_target_does_not_consume_condition() {
        let mut code = InstructionSequence::new();
        let push = code.append(Instruction::Push(value(0)));
        let branch = code.append(Instruction::JumpIfZero(address(99)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, push);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: branch,
                kind: VmErrorKind::InvalidJumpTarget {
                    source: InstructionAddressError::InvalidAddress {
                        address: address(99)
                    }
                }
            })
        );
        assert_clean_control(&vm, branch, false);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(0)));
    }

    #[test]
    fn jump_if_zero_missing_fallthrough_does_not_consume_condition() {
        let mut code = InstructionSequence::new();
        let push = code.append(Instruction::Push(value(1)));
        let branch = code.append(Instruction::JumpIfZero(push));
        let mut vm = new_vm(&code, push);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));
        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: branch,
                kind: VmErrorKind::UnexpectedEndOfCode {
                    source: InstructionAddressError::EndAddress {
                        address: address(2)
                    }
                }
            })
        );
        assert_clean_control(&vm, branch, false);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(1)));
    }

    #[test]
    fn return_moves_to_valid_frame_target_and_preserves_data_and_halted_state() {
        let mut code = InstructionSequence::new();
        let target = code.append(Instruction::Halt);
        let entry = code.append(Instruction::Return);
        let mut vm = new_vm(&code, entry);
        vm.data_stack.push(value(11));
        vm.push_return_frame(return_frame(target));

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_clean_control(&vm, target, false);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(11)));
    }

    #[test]
    fn return_pops_only_top_frame_and_uses_lifo_order() {
        let mut code = InstructionSequence::new();
        let first_target = code.append(Instruction::Halt);
        let second_target = code.append(Instruction::Halt);
        let entry = code.append(Instruction::Return);
        let mut vm = new_vm(&code, entry);

        vm.push_return_frame(return_frame(first_target));
        vm.push_return_frame(return_frame(second_target));

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_eq!(vm.instruction_pointer(), second_target);
        assert!(!vm.is_halted());
        assert_eq!(vm.return_stack_depth(), 1);

        vm.instruction_pointer = entry;
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Continued));

        assert_clean_control(&vm, first_target, false);
    }

    #[test]
    fn return_underflow_reports_error_without_mutation_or_implicit_halt() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Return);
        let mut vm = new_vm(&code, entry);

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::ReturnStackUnderflow {
                    source: StackError::ReturnStackUnderflow
                }
            })
        );
        assert_clean_control(&vm, entry, false);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn return_rejects_end_target_without_popping_frame_or_mutation() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Return);
        let end = address(code.len());
        let mut vm = new_vm(&code, entry);
        vm.data_stack.push(value(3));
        vm.push_return_frame(return_frame(end));

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::InvalidReturnTarget {
                    source: InstructionAddressError::EndAddress { address: end }
                }
            })
        );
        assert_eq!(vm.instruction_pointer(), entry);
        assert!(!vm.is_halted());
        assert_eq!(vm.return_stack_depth(), 1);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(3)));
    }

    #[test]
    fn return_rejects_out_of_range_target_without_popping_frame_or_mutation() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Return);
        let invalid = address(usize::MAX);
        let mut vm = new_vm(&code, entry);
        vm.push_return_frame(return_frame(invalid));

        let result = vm.step(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::InvalidReturnTarget {
                    source: InstructionAddressError::InvalidAddress { address: invalid }
                }
            })
        );
        assert_eq!(vm.instruction_pointer(), entry);
        assert!(!vm.is_halted());
        assert_eq!(vm.return_stack_depth(), 1);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn return_rejects_target_missing_from_current_instruction_view() {
        let mut full_code = InstructionSequence::new();
        let entry = full_code.append(Instruction::Return);
        let target = full_code.append(Instruction::Halt);
        let mut shorter_code = InstructionSequence::new();
        shorter_code.append(Instruction::Return);
        let mut vm = new_vm(&full_code, entry);
        vm.push_return_frame(return_frame(target));

        let result = vm.step(shorter_code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::InvalidReturnTarget {
                    source: InstructionAddressError::EndAddress { address: target }
                }
            })
        );
        assert_eq!(vm.instruction_pointer(), entry);
        assert!(!vm.is_halted());
        assert_eq!(vm.return_stack_depth(), 1);
    }

    #[test]
    fn halt_sets_halted_without_changing_stacks_or_instruction_pointer() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Halted));

        assert_clean_control(&vm, entry, true);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn halted_step_and_run_are_idempotent() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Halted));
        assert_eq!(vm.step(code.view()), Ok(StepOutcome::Halted));
        assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

        assert_clean_control(&vm, entry, true);
        assert_eq!(vm.data_stack_depth(), 0);
    }

    #[test]
    fn run_executes_until_halt() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(1)));
        code.append(Instruction::Push(value(2)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

        assert!(vm.is_halted());
        assert_eq!(vm.data_stack_depth(), 2);
        assert_eq!(vm.pop_data(), Ok(value(2)));
        assert_eq!(vm.pop_data(), Ok(value(1)));
    }

    #[test]
    fn run_uses_jump_semantics() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Jump(address(2)));
        code.append(Instruction::Push(value(99)));
        code.append(Instruction::Push(value(7)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(7)));
    }

    #[test]
    fn run_uses_conditional_branch_semantics() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(0)));
        code.append(Instruction::JumpIfZero(address(3)));
        code.append(Instruction::Halt);
        code.append(Instruction::Push(value(42)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);

        assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(42)));
    }

    #[test]
    fn run_continues_after_return_until_halt() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Return);
        code.append(Instruction::Push(value(99)));
        let target = code.append(Instruction::Push(value(8)));
        code.append(Instruction::Halt);
        let mut vm = new_vm(&code, entry);
        vm.push_return_frame(return_frame(target));

        assert_eq!(vm.run(code.view()), Ok(RunOutcome::Halted));

        assert!(vm.is_halted());
        assert_eq!(vm.return_stack_depth(), 0);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(8)));
    }

    #[test]
    fn run_reports_return_underflow_as_vm_error() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Return);
        let mut vm = new_vm(&code, entry);

        let result = vm.run(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: entry,
                kind: VmErrorKind::ReturnStackUnderflow {
                    source: StackError::ReturnStackUnderflow
                }
            })
        );
        assert_clean_control(&vm, entry, false);
    }

    #[test]
    fn run_preserves_successful_steps_before_invalid_return_target() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(5)));
        let bad_return = code.append(Instruction::Return);
        let invalid = address(100);
        let mut vm = new_vm(&code, entry);
        vm.push_return_frame(return_frame(invalid));

        let result = vm.run(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: bad_return,
                kind: VmErrorKind::InvalidReturnTarget {
                    source: InstructionAddressError::InvalidAddress { address: invalid }
                }
            })
        );
        assert_eq!(vm.instruction_pointer(), bad_return);
        assert!(!vm.is_halted());
        assert_eq!(vm.return_stack_depth(), 1);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(5)));
    }

    #[test]
    fn run_keeps_successful_steps_before_error_and_failed_step_atomic() {
        let mut code = InstructionSequence::new();
        let entry = code.append(Instruction::Push(value(5)));
        let bad_branch = code.append(Instruction::JumpIfZero(address(100)));
        let mut vm = new_vm(&code, entry);

        let result = vm.run(code.view());

        assert_eq!(
            result,
            Err(VmError {
                address: bad_branch,
                kind: VmErrorKind::UnexpectedEndOfCode {
                    source: InstructionAddressError::EndAddress {
                        address: address(2)
                    }
                }
            })
        );
        assert_clean_control(&vm, bad_branch, false);
        assert_eq!(vm.data_stack_depth(), 1);
        assert_eq!(vm.peek_data(), Ok(value(5)));
    }

    #[test]
    fn vm_error_exposes_failed_instruction_address_and_kind() {
        let address = address(3);
        let error = VmError {
            address,
            kind: VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow,
            },
        };

        assert_eq!(error.address(), address);
        assert_eq!(
            error.kind(),
            VmErrorKind::DataStackUnderflow {
                source: StackError::DataStackUnderflow
            }
        );
    }
}
