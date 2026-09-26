use super::{CodePosition, LogicalInstruction, PrimitiveOp, StaticImage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BytecodeArtifact {
    code: Vec<u8>,
    entry_offset: u16,
    global_slot_count: u16,
}

impl BytecodeArtifact {
    pub(crate) fn code(&self) -> &[u8] {
        &self.code
    }

    pub(crate) const fn entry_offset(&self) -> u16 {
        self.entry_offset
    }

    pub(crate) const fn global_slot_count(&self) -> u16 {
        self.global_slot_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EncodeError {
    UnsupportedInstruction(usize),
    UnsupportedPrimitive(usize),
    GlobalSlotOutOfRange(usize),
    CallBaseOffsetOutOfRange(usize),
    InvalidCodeTarget(usize),
    InvalidEntry(usize),
    ImageTooLarge,
}

pub(crate) fn encode(image: &StaticImage) -> Result<BytecodeArtifact, EncodeError> {
    let mut offsets = Vec::with_capacity(image.code.len() + 1);
    let mut byte_len = 0usize;
    for (index, instruction) in image.code.iter().enumerate() {
        offsets.push(byte_len);
        byte_len = byte_len
            .checked_add(encoded_len(instruction, index)?)
            .ok_or(EncodeError::ImageTooLarge)?;
    }
    offsets.push(byte_len);

    if byte_len > usize::from(u16::MAX) + 1 || image.global_count > usize::from(u8::MAX) + 1 {
        return Err(EncodeError::ImageTooLarge);
    }
    let entry_offset = offsets
        .get(image.entry.0)
        .copied()
        .filter(|offset| image.entry.0 < image.code.len() && *offset <= usize::from(u16::MAX))
        .ok_or(EncodeError::InvalidEntry(image.entry.0))? as u16;
    let mut code = Vec::with_capacity(byte_len);
    for (index, instruction) in image.code.iter().enumerate() {
        let target = |position: CodePosition| {
            offsets
                .get(position.0)
                .copied()
                .filter(|offset| position.0 < image.code.len() && *offset <= usize::from(u16::MAX))
                .map(|offset| offset as u16)
                .ok_or(EncodeError::InvalidCodeTarget(position.0))
        };
        match instruction {
            LogicalInstruction::PushI16(value) => {
                code.push(0x02);
                code.extend_from_slice(&value.to_le_bytes());
            }
            LogicalInstruction::LoadGlobal(slot) | LogicalInstruction::StoreGlobal(slot) => {
                if slot.0 >= image.global_count {
                    return Err(EncodeError::GlobalSlotOutOfRange(slot.0));
                }
                let slot =
                    u8::try_from(slot.0).map_err(|_| EncodeError::GlobalSlotOutOfRange(slot.0))?;
                code.push(
                    if matches!(instruction, LogicalInstruction::LoadGlobal(_)) {
                        0x10
                    } else {
                        0x11
                    },
                );
                code.push(slot);
            }
            LogicalInstruction::CallCode(position) => {
                code.push(0x20);
                code.extend_from_slice(&target(*position)?.to_le_bytes());
            }
            LogicalInstruction::CopyCallBase(offset) => {
                code.push(0x21);
                code.push(
                    u8::try_from(*offset)
                        .map_err(|_| EncodeError::CallBaseOffsetOutOfRange(*offset))?,
                );
            }
            LogicalInstruction::Jump(position) | LogicalInstruction::JumpIfZero(position) => {
                code.push(if matches!(instruction, LogicalInstruction::Jump(_)) {
                    0x30
                } else {
                    0x31
                });
                code.extend_from_slice(&target(*position)?.to_le_bytes());
            }
            LogicalInstruction::CallPrimitive(operation) => {
                let opcode =
                    primitive_opcode(*operation).ok_or(EncodeError::UnsupportedPrimitive(index))?;
                code.push(opcode);
            }
            LogicalInstruction::Return => code.push(0x22),
            LogicalInstruction::Halt => code.push(0x01),
            _ => return Err(EncodeError::UnsupportedInstruction(index)),
        }
    }
    Ok(BytecodeArtifact {
        code,
        entry_offset,
        global_slot_count: image.global_count as u16,
    })
}

fn encoded_len(instruction: &LogicalInstruction, index: usize) -> Result<usize, EncodeError> {
    match instruction {
        LogicalInstruction::PushI16(_) => Ok(3),
        LogicalInstruction::LoadGlobal(_) | LogicalInstruction::StoreGlobal(_) => Ok(2),
        LogicalInstruction::CallCode(_)
        | LogicalInstruction::Jump(_)
        | LogicalInstruction::JumpIfZero(_) => Ok(3),
        LogicalInstruction::CopyCallBase(offset) => u8::try_from(*offset)
            .map(|_| 2)
            .map_err(|_| EncodeError::CallBaseOffsetOutOfRange(*offset)),
        LogicalInstruction::CallPrimitive(operation) if primitive_opcode(*operation).is_some() => {
            Ok(1)
        }
        LogicalInstruction::CallPrimitive(_) => Err(EncodeError::UnsupportedPrimitive(index)),
        LogicalInstruction::Return | LogicalInstruction::Halt => Ok(1),
        _ => Err(EncodeError::UnsupportedInstruction(index)),
    }
}

fn primitive_opcode(operation: PrimitiveOp) -> Option<u8> {
    Some(match operation {
        PrimitiveOp::Add => 0x40,
        PrimitiveOp::Multiply => 0x41,
        PrimitiveOp::Remainder => 0x42,
        PrimitiveOp::Equal => 0x48,
        PrimitiveOp::Less => 0x49,
        PrimitiveOp::LessEqual => 0x4a,
        PrimitiveOp::GreaterEqual => 0x4b,
        PrimitiveOp::Drop => 0x50,
        PrimitiveOp::PutDec => 0x60,
        PrimitiveOp::Cr => 0x61,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(code: Vec<LogicalInstruction>) -> StaticImage {
        StaticImage {
            code,
            entry: super::CodePosition(0),
            texts: Vec::new(),
            global_count: 0,
            array_lengths: Vec::new(),
        }
    }

    fn image_with_globals(code: Vec<LogicalInstruction>, global_count: usize) -> StaticImage {
        StaticImage {
            global_count,
            ..image(code)
        }
    }

    fn image_with_entry(code: Vec<LogicalInstruction>, entry: usize) -> StaticImage {
        StaticImage {
            entry: CodePosition(entry),
            ..image(code)
        }
    }

    #[test]
    fn encodes_each_m32_instruction_and_primitive_opcode() {
        let instructions = vec![
            LogicalInstruction::Halt,
            LogicalInstruction::PushI16(i16::MIN),
            LogicalInstruction::LoadGlobal(super::super::GlobalSlot(255)),
            LogicalInstruction::StoreGlobal(super::super::GlobalSlot(0)),
            LogicalInstruction::CallCode(CodePosition(0)),
            LogicalInstruction::CopyCallBase(255),
            LogicalInstruction::Return,
            LogicalInstruction::Jump(CodePosition(0)),
            LogicalInstruction::JumpIfZero(CodePosition(0)),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Add),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Multiply),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Remainder),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Equal),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Less),
            LogicalInstruction::CallPrimitive(PrimitiveOp::LessEqual),
            LogicalInstruction::CallPrimitive(PrimitiveOp::GreaterEqual),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Drop),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutDec),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Cr),
        ];
        // Relocations target the final instruction, whose byte offset is fixed by preceding widths.
        let mut instructions = instructions;
        instructions[4] = LogicalInstruction::CallCode(CodePosition(18));
        instructions[7] = LogicalInstruction::Jump(CodePosition(18));
        instructions[8] = LogicalInstruction::JumpIfZero(CodePosition(18));
        let bytes = encode(&image_with_globals(instructions, 256)).expect("subset encodes");
        assert_eq!(
            bytes.code(),
            &[
                0x01, 0x02, 0x00, 0x80, 0x10, 0xff, 0x11, 0x00, 0x20, 0x1d, 0x00, 0x21, 0xff, 0x22,
                0x30, 0x1d, 0x00, 0x31, 0x1d, 0x00, 0x40, 0x41, 0x42, 0x48, 0x49, 0x4a, 0x4b, 0x50,
                0x60, 0x61,
            ]
        );
        assert_eq!(bytes.entry_offset(), 0);
    }

    #[test]
    fn encodes_signed_immediates_as_little_endian() {
        let artifact = encode(&image(vec![
            LogicalInstruction::PushI16(0x1234),
            LogicalInstruction::PushI16(-2),
            LogicalInstruction::PushI16(i16::MAX),
            LogicalInstruction::Halt,
        ]))
        .expect("immediates encode");
        assert_eq!(
            artifact.code(),
            &[0x02, 0x34, 0x12, 0x02, 0xfe, 0xff, 0x02, 0xff, 0x7f, 0x01]
        );
    }

    #[test]
    fn relocations_resolve_variable_width_forward_and_backward_targets() {
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(3)),
            LogicalInstruction::PushI16(9),
            LogicalInstruction::JumpIfZero(CodePosition(0)),
            LogicalInstruction::CallCode(CodePosition(1)),
            LogicalInstruction::Return,
        ]))
        .expect("relocations resolve");
        assert_eq!(
            artifact.code(),
            &[0x30, 0x09, 0x00, 0x02, 9, 0, 0x31, 0, 0, 0x20, 3, 0, 0x22]
        );
    }

    #[test]
    fn rejects_unsupported_logical_instructions_and_primitives() {
        assert_eq!(
            encode(&image(vec![LogicalInstruction::WriteText(
                super::super::TextSlot(0)
            )]),),
            Err(EncodeError::UnsupportedInstruction(0))
        );
        assert_eq!(
            encode(&image(vec![LogicalInstruction::CallPrimitive(
                PrimitiveOp::Divide
            )]),),
            Err(EncodeError::UnsupportedPrimitive(0))
        );
    }

    #[test]
    fn rejects_unrepresentable_operands_targets_and_image_sizes() {
        assert_eq!(
            encode(&image(vec![LogicalInstruction::LoadGlobal(
                super::super::GlobalSlot(256)
            )]),),
            Err(EncodeError::GlobalSlotOutOfRange(256))
        );
        assert_eq!(
            encode(&image(vec![LogicalInstruction::CopyCallBase(256)]),),
            Err(EncodeError::CallBaseOffsetOutOfRange(256))
        );
        assert_eq!(
            encode(&image(vec![LogicalInstruction::Jump(CodePosition(1))]),),
            Err(EncodeError::InvalidCodeTarget(1))
        );
        let mut too_many_globals = image(vec![LogicalInstruction::Halt]);
        too_many_globals.global_count = 257;
        assert_eq!(encode(&too_many_globals), Err(EncodeError::ImageTooLarge));
        let too_many_bytes = image(vec![LogicalInstruction::PushI16(0); 21_846]);
        assert_eq!(encode(&too_many_bytes), Err(EncodeError::ImageTooLarge));
    }

    #[test]
    fn encoding_is_deterministic_and_reports_global_count() {
        let mut source = image(vec![LogicalInstruction::Halt]);
        source.global_count = 256;
        let first = encode(&source).expect("image encodes");
        let second = encode(&source).expect("same image encodes");
        assert_eq!(first, second);
        assert_eq!(first.global_slot_count(), 256);
        assert_eq!(first.code(), &[0x01]);
    }

    #[test]
    fn resolves_nonzero_entry_after_variable_width_instructions() {
        let mut source = image_with_globals(
            vec![
                LogicalInstruction::PushI16(-4),
                LogicalInstruction::LoadGlobal(super::super::GlobalSlot(0)),
                LogicalInstruction::Halt,
            ],
            1,
        );
        source.entry = super::CodePosition(1);
        let artifact = encode(&source).expect("nonzero entry resolves");

        assert_eq!(artifact.entry_offset(), 3);
    }

    #[test]
    fn rejects_invalid_entry_positions() {
        assert_eq!(
            encode(&image_with_entry(vec![LogicalInstruction::Halt], 1)),
            Err(EncodeError::InvalidEntry(1))
        );
    }
}
