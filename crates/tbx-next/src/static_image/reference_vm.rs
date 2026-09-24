use super::{CodePosition, LogicalInstruction, PrimitiveOp, StaticImage};
use crate::random::RandomState;
use crate::runtime_input::RuntimeInput;
use crate::runtime_output::RuntimeOutput;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReturnFrame {
    position: CodePosition,
    data_depth: usize,
    control_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeError {
    pub(super) position: CodePosition,
    pub(super) kind: RuntimeErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeErrorKind {
    InvalidPosition,
    InvalidTarget,
    DataUnderflow,
    ControlUnderflow,
    ReturnUnderflow,
    InvalidCallBase,
    DataBelowCallBase,
    InvalidGlobal,
    InvalidArray,
    InvalidArrayIndex,
    InvalidText,
    Arithmetic,
    InputUnavailable,
    InputFailed,
    OutputUnavailable,
    OutputFailed,
    RandomUnavailable,
    InvalidRandomBound,
    InvalidCharacter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunOutcome {
    Continued,
    Halted,
}

pub(super) struct Capabilities<'a> {
    output: Option<&'a mut dyn RuntimeOutput>,
    input: Option<&'a mut dyn RuntimeInput>,
    random: Option<&'a mut RandomState>,
}

/// Private executable model for static images. It owns target-local state only.
pub(super) struct ReferenceVm {
    image: StaticImage,
    position: CodePosition,
    data: Vec<i16>,
    control: Vec<i16>,
    returns: Vec<ReturnFrame>,
    globals: Vec<i16>,
    arrays: Vec<Vec<i16>>,
    halted: bool,
}

impl ReferenceVm {
    pub(super) fn new(image: StaticImage, entry: CodePosition) -> Result<Self, RuntimeError> {
        if entry.0 >= image.code.len() {
            return Err(RuntimeError {
                position: entry,
                kind: RuntimeErrorKind::InvalidPosition,
            });
        }
        Ok(Self {
            globals: vec![0; image.global_count],
            arrays: image
                .array_lengths
                .iter()
                .map(|&len| vec![0; len])
                .collect(),
            image,
            position: entry,
            data: Vec::new(),
            control: Vec::new(),
            returns: Vec::new(),
            halted: false,
        })
    }

    pub(super) fn position(&self) -> CodePosition {
        self.position
    }

    pub(super) fn is_halted(&self) -> bool {
        self.halted
    }

    #[cfg(test)]
    pub(super) fn data_stack(&self) -> &[i16] {
        &self.data
    }

    pub(super) fn step(
        &mut self,
        capabilities: &mut Capabilities<'_>,
    ) -> Result<RunOutcome, RuntimeError> {
        if self.halted {
            return Ok(RunOutcome::Halted);
        }
        let position = self.position;
        let instruction = self
            .image
            .code
            .get(position.0)
            .ok_or(RuntimeError {
                position,
                kind: RuntimeErrorKind::InvalidPosition,
            })?
            .clone();
        let next = || {
            let candidate = CodePosition(position.0.saturating_add(1));
            if candidate.0 < self.image.code.len() {
                Ok(candidate)
            } else {
                Err(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidTarget,
                })
            }
        };
        match instruction {
            LogicalInstruction::PushI16(value) => {
                let next = next()?;
                self.data.push(value);
                self.position = next;
            }
            LogicalInstruction::WriteText(slot) => {
                let text = self.image.texts.get(slot.0).ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidText,
                })?;
                let next = next()?;
                capabilities
                    .output
                    .as_mut()
                    .ok_or(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::OutputUnavailable,
                    })?
                    .write(text)
                    .map_err(|_| RuntimeError {
                        position,
                        kind: RuntimeErrorKind::OutputFailed,
                    })?;
                self.position = next;
            }
            LogicalInstruction::LoadGlobal(slot) => {
                let value = *self.globals.get(slot.0).ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidGlobal,
                })?;
                let next = next()?;
                self.data.push(value);
                self.position = next;
            }
            LogicalInstruction::StoreGlobal(slot) => {
                let value = *self.data.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::DataUnderflow,
                })?;
                let next = next()?;
                let global = self.globals.get_mut(slot.0).ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidGlobal,
                })?;
                *global = value;
                self.data.pop();
                self.position = next;
            }
            LogicalInstruction::LoadArray(slot) => {
                let index = *self.data.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::DataUnderflow,
                })?;
                let array = self.arrays.get(slot.0).ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidArray,
                })?;
                let value = surface_index(index, array.len())
                    .and_then(|i| array.get(i).copied())
                    .ok_or(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::InvalidArrayIndex,
                    })?;
                let next = next()?;
                *self.data.last_mut().expect("index checked") = value;
                self.position = next;
            }
            LogicalInstruction::StoreArray(slot) => {
                if self.data.len() < 2 {
                    return Err(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::DataUnderflow,
                    });
                }
                let depth = self.data.len();
                let index = self.data[depth - 2];
                let value = self.data[depth - 1];
                let next = next()?;
                let array = self.arrays.get_mut(slot.0).ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidArray,
                })?;
                let element = surface_index(index, array.len())
                    .and_then(|i| array.get_mut(i))
                    .ok_or(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::InvalidArrayIndex,
                    })?;
                *element = value;
                self.data.truncate(depth - 2);
                self.position = next;
            }
            LogicalInstruction::CallPrimitive(op) => {
                self.call_primitive(op, position, next()?, capabilities)?;
            }
            LogicalInstruction::CallCode(target) => {
                self.validate_target(target, position)?;
                let next = next()?;
                self.returns.push(ReturnFrame {
                    position: next,
                    data_depth: self.data.len(),
                    control_depth: self.control.len(),
                });
                self.position = target;
            }
            LogicalInstruction::CopyCallBase(offset) => {
                let frame = self.returns.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidCallBase,
                })?;
                if offset == 0 || offset > frame.data_depth {
                    return Err(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::InvalidCallBase,
                    });
                }
                let index = frame.data_depth - offset;
                let value = *self.data.get(index).ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::InvalidCallBase,
                })?;
                let next = next()?;
                self.data.push(value);
                self.position = next;
            }
            LogicalInstruction::TruncateCallBase => {
                let base = self
                    .returns
                    .last()
                    .ok_or(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::InvalidCallBase,
                    })?
                    .data_depth;
                if self.data.len() < base {
                    return Err(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::DataBelowCallBase,
                    });
                }
                let next = next()?;
                self.data.truncate(base);
                self.position = next;
            }
            LogicalInstruction::ControlPush => {
                let value = *self.data.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::DataUnderflow,
                })?;
                let next = next()?;
                self.data.pop();
                self.control.push(value);
                self.position = next;
            }
            LogicalInstruction::ControlCopy => {
                let value = *self.control.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::ControlUnderflow,
                })?;
                let next = next()?;
                self.data.push(value);
                self.position = next;
            }
            LogicalInstruction::ControlDrop => {
                if self.control.is_empty() {
                    return Err(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::ControlUnderflow,
                    });
                }
                let next = next()?;
                self.control.pop();
                self.position = next;
            }
            LogicalInstruction::Jump(target) => {
                self.validate_target(target, position)?;
                self.position = target;
            }
            LogicalInstruction::JumpIfZero(target) => {
                let condition = *self.data.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::DataUnderflow,
                })?;
                let destination = if condition == 0 { target } else { next()? };
                self.validate_target(destination, position)?;
                self.data.pop();
                self.position = destination;
            }
            LogicalInstruction::Return => {
                let frame = *self.returns.last().ok_or(RuntimeError {
                    position,
                    kind: RuntimeErrorKind::ReturnUnderflow,
                })?;
                self.validate_target(frame.position, position)?;
                if self.control.len() < frame.control_depth {
                    return Err(RuntimeError {
                        position,
                        kind: RuntimeErrorKind::ControlUnderflow,
                    });
                }
                let next = frame.position;
                self.control.truncate(frame.control_depth);
                self.returns.pop();
                self.position = next;
            }
            LogicalInstruction::Halt => self.halted = true,
        }
        Ok(if self.halted {
            RunOutcome::Halted
        } else {
            RunOutcome::Continued
        })
    }

    pub(super) fn run<'a>(
        &mut self,
        output: Option<&'a mut dyn RuntimeOutput>,
        input: Option<&'a mut dyn RuntimeInput>,
        random: Option<&'a mut RandomState>,
    ) -> Result<RunOutcome, RuntimeError> {
        let mut capabilities = Capabilities {
            output,
            input,
            random,
        };
        while !self.halted {
            self.step(&mut capabilities)?;
        }
        Ok(RunOutcome::Halted)
    }

    fn validate_target(
        &self,
        target: CodePosition,
        position: CodePosition,
    ) -> Result<(), RuntimeError> {
        if target.0 < self.image.code.len() {
            Ok(())
        } else {
            Err(RuntimeError {
                position,
                kind: RuntimeErrorKind::InvalidTarget,
            })
        }
    }

    fn call_primitive(
        &mut self,
        op: PrimitiveOp,
        position: CodePosition,
        next: CodePosition,
        capabilities: &mut Capabilities<'_>,
    ) -> Result<(), RuntimeError> {
        let fail = |kind| RuntimeError { position, kind };
        let depth = self.data.len();
        let unary = || {
            self.data
                .last()
                .copied()
                .ok_or_else(|| fail(RuntimeErrorKind::DataUnderflow))
        };
        let binary = || {
            if depth < 2 {
                Err(fail(RuntimeErrorKind::DataUnderflow))
            } else {
                Ok((self.data[depth - 2], self.data[depth - 1]))
            }
        };
        match op {
            PrimitiveOp::Add
            | PrimitiveOp::Subtract
            | PrimitiveOp::Multiply
            | PrimitiveOp::Divide
            | PrimitiveOp::Remainder
            | PrimitiveOp::Equal
            | PrimitiveOp::NotEqual
            | PrimitiveOp::Less
            | PrimitiveOp::LessEqual
            | PrimitiveOp::Greater
            | PrimitiveOp::GreaterEqual
            | PrimitiveOp::And
            | PrimitiveOp::Or => {
                let (a, b) = binary()?;
                let result = match op {
                    PrimitiveOp::Add => Value::integer(a)
                        .checked_add(Value::integer(b))
                        .map(Value::as_integer),
                    PrimitiveOp::Subtract => Value::integer(a)
                        .checked_sub(Value::integer(b))
                        .map(Value::as_integer),
                    PrimitiveOp::Multiply => Value::integer(a)
                        .checked_mul(Value::integer(b))
                        .map(Value::as_integer),
                    PrimitiveOp::Divide => Value::integer(a)
                        .checked_div(Value::integer(b))
                        .map(Value::as_integer),
                    PrimitiveOp::Remainder => Value::integer(a)
                        .checked_rem(Value::integer(b))
                        .map(Value::as_integer),
                    PrimitiveOp::Equal => Ok(i16::from(a == b)),
                    PrimitiveOp::NotEqual => Ok(i16::from(a != b)),
                    PrimitiveOp::Less => Ok(i16::from(a < b)),
                    PrimitiveOp::LessEqual => Ok(i16::from(a <= b)),
                    PrimitiveOp::Greater => Ok(i16::from(a > b)),
                    PrimitiveOp::GreaterEqual => Ok(i16::from(a >= b)),
                    PrimitiveOp::And => Ok(i16::from(a != 0 && b != 0)),
                    PrimitiveOp::Or => Ok(i16::from(a != 0 || b != 0)),
                    _ => unreachable!(),
                }
                .map_err(|_| fail(RuntimeErrorKind::Arithmetic))?;
                self.data.truncate(depth - 2);
                self.data.push(result);
            }
            PrimitiveOp::Negate | PrimitiveOp::Abs | PrimitiveOp::Not => {
                let value = unary()?;
                let result = match op {
                    PrimitiveOp::Negate => Value::integer(value).checked_neg(),
                    PrimitiveOp::Abs => Value::integer(value).checked_abs(),
                    PrimitiveOp::Not => Ok(Value::integer(i16::from(value == 0))),
                    _ => unreachable!(),
                }
                .map(Value::as_integer)
                .map_err(|_| fail(RuntimeErrorKind::Arithmetic))?;
                *self.data.last_mut().expect("operand checked") = result;
            }
            PrimitiveOp::Dup => self.data.push(unary()?),
            PrimitiveOp::Drop => {
                unary()?;
                self.data.pop();
            }
            PrimitiveOp::Swap => {
                let _ = binary()?;
                self.data.swap(depth - 1, depth - 2);
            }
            PrimitiveOp::PutDec | PrimitiveOp::PutChr => {
                if let Some(&value) = self.data.last() {
                    let text = if op == PrimitiveOp::PutDec {
                        value.to_string()
                    } else {
                        let byte = u8::try_from(value)
                            .ok()
                            .filter(|byte| *byte <= 0x7f)
                            .ok_or_else(|| fail(RuntimeErrorKind::InvalidCharacter))?;
                        char::from(byte).to_string()
                    };
                    capabilities
                        .output
                        .as_mut()
                        .ok_or_else(|| fail(RuntimeErrorKind::OutputUnavailable))?
                        .write(&text)
                        .map_err(|_| fail(RuntimeErrorKind::OutputFailed))?;
                    self.data.pop();
                }
            }
            PrimitiveOp::Cr => capabilities
                .output
                .as_mut()
                .ok_or_else(|| fail(RuntimeErrorKind::OutputUnavailable))?
                .write("\n")
                .map_err(|_| fail(RuntimeErrorKind::OutputFailed))?,
            PrimitiveOp::InputQuestion => {
                let line = capabilities
                    .input
                    .as_mut()
                    .ok_or_else(|| fail(RuntimeErrorKind::InputUnavailable))?
                    .read_line()
                    .map_err(|_| fail(RuntimeErrorKind::InputFailed))?;
                let value = line
                    .as_deref()
                    .and_then(parse_i16)
                    .map_or((0, 0), |n| (n, 1));
                self.data.extend([value.0, value.1]);
            }
            PrimitiveOp::Rnd => {
                let upper = unary()?;
                let result = capabilities
                    .random
                    .as_mut()
                    .ok_or_else(|| fail(RuntimeErrorKind::RandomUnavailable))?
                    .next_inclusive(upper)
                    .map_err(|_| fail(RuntimeErrorKind::InvalidRandomBound))?;
                *self.data.last_mut().expect("bound checked") = result;
            }
        }
        self.position = next;
        Ok(())
    }
}

fn surface_index(index: i16, len: usize) -> Option<usize> {
    if index > 0 && index as usize <= len {
        Some(index as usize - 1)
    } else {
        None
    }
}

fn parse_i16(input: &str) -> Option<i16> {
    let input = input.trim_matches([' ', '\t']);
    let (sign, digits) = match input.as_bytes().first().copied() {
        Some(b'+') => (1i32, &input[1..]),
        Some(b'-') => (-1i32, &input[1..]),
        _ => (1i32, input),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.bytes().try_fold(0i32, |value, byte| {
        value.checked_mul(10)?.checked_add(i32::from(byte - b'0'))
    })?;
    i16::try_from(magnitude.checked_mul(sign)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_input::TestInput;
    use crate::runtime_output::{RuntimeOutputError, TestOutput};
    use crate::static_image::{ArraySlot, GlobalSlot, LogicalInstruction, StaticImage, TextSlot};

    fn image(code: Vec<LogicalInstruction>) -> StaticImage {
        StaticImage {
            code,
            texts: vec!["hello".into()],
            global_count: 2,
            array_lengths: vec![2],
        }
    }

    fn make_vm(code: Vec<LogicalInstruction>) -> ReferenceVm {
        ReferenceVm::new(image(code), CodePosition(0)).expect("entry should be valid")
    }

    fn tick(vm: &mut ReferenceVm) -> Result<RunOutcome, RuntimeError> {
        vm.step(&mut Capabilities {
            output: None,
            input: None,
            random: None,
        })
    }

    #[test]
    fn values_globals_and_arrays_preserve_i16_and_one_origin_bounds() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(i16::MIN),
            LogicalInstruction::StoreGlobal(GlobalSlot(0)),
            LogicalInstruction::LoadGlobal(GlobalSlot(0)),
            LogicalInstruction::PushI16(2),
            LogicalInstruction::PushI16(i16::MAX),
            LogicalInstruction::StoreArray(ArraySlot(0)),
            LogicalInstruction::PushI16(2),
            LogicalInstruction::LoadArray(ArraySlot(0)),
            LogicalInstruction::Halt,
        ]);
        for _ in 0..8 {
            tick(&mut vm).unwrap();
        }
        assert_eq!(vm.data, [i16::MIN, i16::MAX]);
        assert_eq!(vm.globals, [i16::MIN, 0]);
        assert_eq!(vm.arrays, [vec![0, i16::MAX]]);
        for (index, expected) in [(1, 17), (2, i16::MAX)] {
            let mut candidate = make_vm(vec![
                LogicalInstruction::PushI16(index),
                LogicalInstruction::LoadArray(ArraySlot(0)),
                LogicalInstruction::Halt,
            ]);
            candidate.arrays[0] = vec![17, i16::MAX];
            assert_eq!(candidate.run(None, None, None), Ok(RunOutcome::Halted));
            assert_eq!(candidate.data, [expected]);
        }
        for index in [0, -1, 3] {
            let mut candidate = make_vm(vec![
                LogicalInstruction::PushI16(index),
                LogicalInstruction::LoadArray(ArraySlot(0)),
            ]);
            let error = {
                tick(&mut candidate).unwrap();
                tick(&mut candidate).unwrap_err()
            };
            assert_eq!(error.position, CodePosition(1));
            assert_eq!(candidate.data, [index]);
        }
        for index in [0, -1, 3] {
            let mut candidate = make_vm(vec![
                LogicalInstruction::PushI16(index),
                LogicalInstruction::PushI16(9),
                LogicalInstruction::StoreArray(ArraySlot(0)),
                LogicalInstruction::Halt,
            ]);
            let error = {
                tick(&mut candidate).unwrap();
                tick(&mut candidate).unwrap();
                tick(&mut candidate).unwrap_err()
            };
            assert_eq!(error.position, CodePosition(2));
            assert_eq!(candidate.data, [index, 9]);
            assert_eq!(candidate.arrays, [vec![0, 0]]);
        }
    }

    #[test]
    fn nested_calls_restore_positions_and_control_depth_per_invocation() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(10),
            LogicalInstruction::ControlPush,
            LogicalInstruction::CallCode(CodePosition(5)),
            LogicalInstruction::ControlCopy,
            LogicalInstruction::Halt,
            LogicalInstruction::CallCode(CodePosition(7)),
            LogicalInstruction::Return,
            LogicalInstruction::PushI16(99),
            LogicalInstruction::ControlPush,
            LogicalInstruction::Return,
        ]);
        assert_eq!(vm.run(None, None, None), Ok(RunOutcome::Halted));
        assert_eq!(vm.data, [10]);
        assert_eq!(vm.control, [10]);
        assert_eq!(vm.returns, []);
    }

    #[test]
    fn recursive_calls_restore_each_invocations_control_depth() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(3),
            LogicalInstruction::CallCode(CodePosition(3)),
            LogicalInstruction::Halt,
            LogicalInstruction::CopyCallBase(1),
            LogicalInstruction::PushI16(1),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Subtract),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Dup),
            LogicalInstruction::JumpIfZero(CodePosition(12)),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Dup),
            LogicalInstruction::ControlPush,
            LogicalInstruction::CallCode(CodePosition(3)),
            LogicalInstruction::Return,
            LogicalInstruction::Return,
        ]);

        assert_eq!(vm.run(None, None, None), Ok(RunOutcome::Halted));
        assert_eq!(vm.data, [3, 2, 1, 0]);
        assert_eq!(vm.control, []);
        assert_eq!(vm.returns, []);
    }

    #[test]
    fn return_with_control_depth_below_call_base_is_atomic() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(7),
            LogicalInstruction::ControlPush,
            LogicalInstruction::CallCode(CodePosition(4)),
            LogicalInstruction::Halt,
            LogicalInstruction::ControlDrop,
            LogicalInstruction::Return,
        ]);
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        assert_eq!(vm.data, []);
        assert_eq!(vm.control, []);
        let saved_frames = vm.returns.clone();
        assert_eq!(saved_frames.len(), 1);
        let position = vm.position();

        let error = tick(&mut vm).unwrap_err();

        assert_eq!(error.position, position);
        assert_eq!(error.kind, RuntimeErrorKind::ControlUnderflow);
        assert_eq!(vm.position(), position);
        assert_eq!(vm.data, []);
        assert_eq!(vm.control, []);
        assert_eq!(vm.returns, saved_frames);
    }

    #[test]
    fn call_base_copy_uses_one_origin_offsets_and_truncates_to_each_frame() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(11),
            LogicalInstruction::PushI16(22),
            LogicalInstruction::CallCode(CodePosition(5)),
            LogicalInstruction::Halt,
            LogicalInstruction::Halt,
            LogicalInstruction::CopyCallBase(1),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::CopyCallBase(2),
            LogicalInstruction::TruncateCallBase,
            LogicalInstruction::Return,
        ]);

        assert_eq!(vm.run(None, None, None), Ok(RunOutcome::Halted));
        assert_eq!(vm.data, [11, 22]);
    }

    #[test]
    fn call_base_copy_reads_the_current_occupant_and_failures_are_atomic() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(11),
            LogicalInstruction::PushI16(22),
            LogicalInstruction::CallCode(CodePosition(5)),
            LogicalInstruction::Halt,
            LogicalInstruction::Halt,
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::PushI16(99),
            LogicalInstruction::PushI16(22),
            LogicalInstruction::CopyCallBase(2),
            LogicalInstruction::Return,
        ]);
        assert_eq!(vm.run(None, None, None), Ok(RunOutcome::Halted));
        assert_eq!(vm.data, [99, 22, 99]);

        for offset in [0, 3] {
            let mut candidate = make_vm(vec![
                LogicalInstruction::PushI16(4),
                LogicalInstruction::PushI16(5),
                LogicalInstruction::CallCode(CodePosition(4)),
                LogicalInstruction::Halt,
                LogicalInstruction::CopyCallBase(offset),
            ]);
            tick(&mut candidate).unwrap();
            tick(&mut candidate).unwrap();
            tick(&mut candidate).unwrap();
            let before = candidate.data.clone();
            let position = candidate.position();
            let error = tick(&mut candidate).unwrap_err();
            assert_eq!(error.kind, RuntimeErrorKind::InvalidCallBase);
            assert_eq!(candidate.data, before);
            assert_eq!(candidate.position(), position);
        }

        let mut out_of_depth = make_vm(vec![
            LogicalInstruction::PushI16(4),
            LogicalInstruction::CallCode(CodePosition(3)),
            LogicalInstruction::Halt,
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::CopyCallBase(1),
        ]);
        tick(&mut out_of_depth).unwrap();
        tick(&mut out_of_depth).unwrap();
        tick(&mut out_of_depth).unwrap();
        let position = out_of_depth.position();
        let error = tick(&mut out_of_depth).unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::InvalidCallBase);
        assert_eq!(out_of_depth.data, []);
        assert_eq!(out_of_depth.position(), position);
    }

    #[test]
    fn primitives_are_checked_and_atomic_and_halt_is_sticky() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(i16::MAX),
            LogicalInstruction::PushI16(1),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Add),
        ]);
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        let position = vm.position();
        let error = tick(&mut vm).unwrap_err();
        assert_eq!(error.position, position);
        assert_eq!(vm.data, [i16::MAX, 1]);
        let mut halted = make_vm(vec![
            LogicalInstruction::Halt,
            LogicalInstruction::PushI16(9),
        ]);
        tick(&mut halted).unwrap();
        tick(&mut halted).unwrap();
        assert_eq!(halted.position(), CodePosition(0));
        assert_eq!(halted.data, []);
    }

    #[test]
    fn checked_arithmetic_covers_normal_boundaries_and_preserves_failed_operands() {
        let successful = [
            (PrimitiveOp::Add, vec![2, 3], 5),
            (PrimitiveOp::Subtract, vec![7, 2], 5),
            (PrimitiveOp::Multiply, vec![-2, 3], -6),
            (PrimitiveOp::Divide, vec![7, 2], 3),
            (PrimitiveOp::Remainder, vec![7, 2], 1),
            (PrimitiveOp::Negate, vec![i16::MAX], -i16::MAX),
            (PrimitiveOp::Abs, vec![i16::MIN + 1], i16::MAX),
        ];
        for (op, operands, expected) in successful {
            let mut code: Vec<_> = operands
                .iter()
                .copied()
                .map(LogicalInstruction::PushI16)
                .collect();
            code.extend([
                LogicalInstruction::CallPrimitive(op),
                LogicalInstruction::Halt,
            ]);
            let mut vm = make_vm(code);
            assert_eq!(vm.run(None, None, None), Ok(RunOutcome::Halted), "{op:?}");
            assert_eq!(vm.data, [expected], "{op:?}");
        }

        let failures = [
            (PrimitiveOp::Add, vec![i16::MAX, 1]),
            (PrimitiveOp::Subtract, vec![i16::MIN, 1]),
            (PrimitiveOp::Multiply, vec![i16::MAX, 2]),
            (PrimitiveOp::Divide, vec![1, 0]),
            (PrimitiveOp::Divide, vec![i16::MIN, -1]),
            (PrimitiveOp::Remainder, vec![1, 0]),
            (PrimitiveOp::Remainder, vec![i16::MIN, -1]),
            (PrimitiveOp::Negate, vec![i16::MIN]),
            (PrimitiveOp::Abs, vec![i16::MIN]),
        ];
        for (op, operands) in failures {
            let mut code: Vec<_> = operands
                .iter()
                .copied()
                .map(LogicalInstruction::PushI16)
                .collect();
            code.extend([
                LogicalInstruction::CallPrimitive(op),
                LogicalInstruction::Halt,
            ]);
            let mut vm = make_vm(code);
            for _ in &operands {
                tick(&mut vm).unwrap();
            }
            let position = vm.position();
            let error = tick(&mut vm).unwrap_err();
            assert_eq!(error.position, position, "{op:?}");
            assert_eq!(error.kind, RuntimeErrorKind::Arithmetic, "{op:?}");
            assert_eq!(vm.data, operands, "{op:?}");
            assert_eq!(vm.position(), position, "{op:?}");
        }

        for op in [
            PrimitiveOp::Add,
            PrimitiveOp::Subtract,
            PrimitiveOp::Multiply,
            PrimitiveOp::Divide,
            PrimitiveOp::Remainder,
            PrimitiveOp::Negate,
            PrimitiveOp::Abs,
            PrimitiveOp::Not,
            PrimitiveOp::Dup,
            PrimitiveOp::Drop,
            PrimitiveOp::Swap,
            PrimitiveOp::Rnd,
        ] {
            let mut vm = make_vm(vec![
                LogicalInstruction::CallPrimitive(op),
                LogicalInstruction::Halt,
            ]);
            let error = tick(&mut vm).unwrap_err();
            assert_eq!(error.kind, RuntimeErrorKind::DataUnderflow, "{op:?}");
            assert_eq!(vm.data, [], "{op:?}");
            assert_eq!(vm.position(), CodePosition(0), "{op:?}");
        }
        for op in [
            PrimitiveOp::Add,
            PrimitiveOp::Subtract,
            PrimitiveOp::Multiply,
            PrimitiveOp::Divide,
            PrimitiveOp::Remainder,
            PrimitiveOp::Equal,
            PrimitiveOp::NotEqual,
            PrimitiveOp::Less,
            PrimitiveOp::LessEqual,
            PrimitiveOp::Greater,
            PrimitiveOp::GreaterEqual,
            PrimitiveOp::And,
            PrimitiveOp::Or,
        ] {
            let mut vm = make_vm(vec![
                LogicalInstruction::PushI16(77),
                LogicalInstruction::CallPrimitive(op),
                LogicalInstruction::Halt,
            ]);
            tick(&mut vm).unwrap();
            let error = tick(&mut vm).unwrap_err();
            assert_eq!(error.kind, RuntimeErrorKind::DataUnderflow, "{op:?}");
            assert_eq!(vm.data, [77], "{op:?}");
            assert_eq!(vm.position(), CodePosition(1), "{op:?}");
        }
    }

    #[test]
    fn comparison_logic_stack_and_output_primitives_have_expected_effects() {
        let successful = [
            (PrimitiveOp::Equal, vec![4, 4], vec![1]),
            (PrimitiveOp::NotEqual, vec![4, 5], vec![1]),
            (PrimitiveOp::Less, vec![4, 5], vec![1]),
            (PrimitiveOp::LessEqual, vec![5, 5], vec![1]),
            (PrimitiveOp::Greater, vec![6, 5], vec![1]),
            (PrimitiveOp::GreaterEqual, vec![5, 5], vec![1]),
            (PrimitiveOp::And, vec![1, -1], vec![1]),
            (PrimitiveOp::Or, vec![0, -1], vec![1]),
            (PrimitiveOp::Not, vec![0], vec![1]),
            (PrimitiveOp::Not, vec![-1], vec![0]),
            (PrimitiveOp::Dup, vec![7], vec![7, 7]),
            (PrimitiveOp::Drop, vec![7], vec![]),
            (PrimitiveOp::Swap, vec![1, 2], vec![2, 1]),
        ];
        for (op, operands, expected) in successful {
            let mut code: Vec<_> = operands
                .iter()
                .copied()
                .map(LogicalInstruction::PushI16)
                .collect();
            code.extend([
                LogicalInstruction::CallPrimitive(op),
                LogicalInstruction::Halt,
            ]);
            let mut vm = make_vm(code);
            assert_eq!(vm.run(None, None, None), Ok(RunOutcome::Halted), "{op:?}");
            assert_eq!(vm.data, expected, "{op:?}");
        }

        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(-42),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutDec),
            LogicalInstruction::PushI16(65),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutChr),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Cr),
            LogicalInstruction::Halt,
        ]);
        let mut output = TestOutput::new();
        assert_eq!(
            vm.run(Some(&mut output), None, None),
            Ok(RunOutcome::Halted)
        );
        assert_eq!(output.chunks(), ["-42", "A", "\n"]);
        assert_eq!(vm.data, []);

        let mut empty_output = make_vm(vec![
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutDec),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutChr),
            LogicalInstruction::Halt,
        ]);
        assert_eq!(
            empty_output.run(None, None, None),
            Ok(RunOutcome::Halted),
            "empty PUTDEC and PUTCHR are no-ops like the host primitives"
        );
        assert_eq!(empty_output.data, []);
    }

    #[test]
    fn output_primitive_failures_preserve_operands_and_instruction_position() {
        for op in [PrimitiveOp::PutDec, PrimitiveOp::PutChr, PrimitiveOp::Cr] {
            let mut vm = make_vm(vec![
                LogicalInstruction::PushI16(if op == PrimitiveOp::PutDec { 42 } else { 65 }),
                LogicalInstruction::CallPrimitive(op),
                LogicalInstruction::Halt,
            ]);
            tick(&mut vm).unwrap();
            let position = vm.position();
            let error = tick(&mut vm).unwrap_err();
            assert_eq!(error.kind, RuntimeErrorKind::OutputUnavailable, "{op:?}");
            assert_eq!(vm.position(), position, "{op:?}");
            if op != PrimitiveOp::Cr {
                assert_eq!(vm.data, [if op == PrimitiveOp::PutDec { 42 } else { 65 }]);
            }
        }

        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(42),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutDec),
            LogicalInstruction::Halt,
        ]);
        tick(&mut vm).unwrap();
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);
        let mut capabilities = Capabilities {
            output: Some(&mut output),
            input: None,
            random: None,
        };
        let position = vm.position();
        let error = vm.step(&mut capabilities).unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::OutputFailed);
        assert_eq!(vm.data, [42]);
        assert_eq!(vm.position(), position);

        let mut invalid_character = make_vm(vec![
            LogicalInstruction::PushI16(256),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutChr),
            LogicalInstruction::Halt,
        ]);
        tick(&mut invalid_character).unwrap();
        let position = invalid_character.position();
        let error = tick(&mut invalid_character).unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::InvalidCharacter);
        assert_eq!(invalid_character.data, [256]);
        assert_eq!(invalid_character.position(), position);
    }

    #[test]
    fn branches_conditions_and_text_use_injected_capabilities() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(-1),
            LogicalInstruction::JumpIfZero(CodePosition(4)),
            LogicalInstruction::WriteText(TextSlot(0)),
            LogicalInstruction::Halt,
            LogicalInstruction::PushI16(7),
            LogicalInstruction::Halt,
        ]);
        let mut output = TestOutput::new();
        vm.run(Some(&mut output), None, None).unwrap();
        assert_eq!(output.chunks(), ["hello"]);
        assert_eq!(vm.data, []);
    }

    #[test]
    fn backward_branch_and_invalid_target_are_checked_at_the_current_instruction() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(1),
            LogicalInstruction::Jump(CodePosition(0)),
        ]);
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        assert_eq!(vm.position(), CodePosition(0));
        assert_eq!(vm.data, [1]);

        let mut invalid = make_vm(vec![LogicalInstruction::Jump(CodePosition(7))]);
        let error = tick(&mut invalid).unwrap_err();
        assert_eq!(error.position, CodePosition(0));
        assert_eq!(error.kind, RuntimeErrorKind::InvalidTarget);
        assert_eq!(invalid.position(), CodePosition(0));
    }

    #[test]
    fn failed_text_output_keeps_instruction_position() {
        let mut vm = make_vm(vec![
            LogicalInstruction::WriteText(TextSlot(0)),
            LogicalInstruction::Halt,
        ]);
        let mut output = TestOutput::new();
        output.fail_next_write(RuntimeOutputError::Failed);
        let mut capabilities = Capabilities {
            output: Some(&mut output),
            input: None,
            random: None,
        };

        let error = vm.step(&mut capabilities).unwrap_err();
        assert_eq!(error.position, CodePosition(0));
        assert_eq!(error.kind, RuntimeErrorKind::OutputFailed);
        assert_eq!(vm.position(), CodePosition(0));
    }

    #[test]
    fn invalid_array_store_keeps_both_operands_and_array_unchanged() {
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(0),
            LogicalInstruction::PushI16(9),
            LogicalInstruction::StoreArray(ArraySlot(0)),
            LogicalInstruction::Halt,
        ]);
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        let error = tick(&mut vm).unwrap_err();
        assert_eq!(error.position, CodePosition(2));
        assert_eq!(vm.data, [0, 9]);
        assert_eq!(vm.arrays, [vec![0, 0]]);
    }

    #[test]
    fn input_and_random_are_injected() {
        let mut vm = make_vm(vec![
            LogicalInstruction::CallPrimitive(PrimitiveOp::InputQuestion),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::PushI16(1),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Rnd),
            LogicalInstruction::Halt,
        ]);
        let mut input = TestInput::new([Ok(Some("-32768".into()))]);
        let mut random = RandomState::seeded(42);
        vm.run(None, Some(&mut input), Some(&mut random)).unwrap();
        assert_eq!(vm.data, [1]);
    }

    #[test]
    fn return_and_truncate_fail_without_partial_stack_updates() {
        let mut vm = make_vm(vec![LogicalInstruction::Return]);
        assert_eq!(
            tick(&mut vm).unwrap_err().kind,
            RuntimeErrorKind::ReturnUnderflow
        );
        assert_eq!(vm.data, []);
        let mut vm = make_vm(vec![
            LogicalInstruction::PushI16(5),
            LogicalInstruction::CallCode(CodePosition(3)),
            LogicalInstruction::Halt,
            LogicalInstruction::TruncateCallBase,
            LogicalInstruction::Halt,
        ]);
        tick(&mut vm).unwrap();
        tick(&mut vm).unwrap();
        vm.data.clear();
        let error = tick(&mut vm).unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::DataBelowCallBase);
        assert_eq!(vm.data, []);
        assert_eq!(vm.returns.len(), 1);
    }
}
