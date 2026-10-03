use super::{CodePosition, LogicalInstruction, PrimitiveOp, ScratchSlot, StaticImage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BytecodeArtifact {
    code: Vec<u8>,
    entry_offset: u16,
    global_slot_count: u16,
    array_lengths: Vec<u16>,
    texts: Vec<FixedText>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FixedText {
    bytes: Vec<u8>,
    byte_length: u16,
}

impl FixedText {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) const fn byte_length(&self) -> u16 {
        self.byte_length
    }
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

    pub(crate) fn array_count(&self) -> usize {
        self.array_lengths.len()
    }

    pub(crate) fn array_lengths(&self) -> &[u16] {
        &self.array_lengths
    }

    pub(crate) fn array_storage_bytes(&self) -> Option<usize> {
        self.array_lengths
            .iter()
            .try_fold(0usize, |cells, length| {
                cells.checked_add(usize::from(*length))
            })?
            .checked_mul(2)
    }

    pub(crate) fn array_descriptor_bytes(&self) -> Option<usize> {
        self.array_count().checked_mul(4)
    }

    pub(crate) fn text_count(&self) -> usize {
        self.texts.len()
    }

    pub(crate) fn texts(&self) -> &[FixedText] {
        &self.texts
    }

    pub(crate) fn text_storage_bytes(&self) -> Option<usize> {
        self.texts.iter().try_fold(0usize, |bytes, text| {
            bytes.checked_add(usize::from(text.byte_length()))
        })
    }

    pub(crate) fn text_descriptor_bytes(&self) -> Option<usize> {
        self.text_count().checked_mul(4)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EncodeError {
    UnsupportedInstruction(usize),
    UnsupportedPrimitive(usize),
    GlobalSlotOutOfRange(usize),
    ArraySlotOutOfRange(usize),
    TooManyArrays(usize),
    ArrayLengthOutOfRange(usize),
    ArrayStorageTooLarge,
    TextSlotOutOfRange(usize),
    TooManyTexts(usize),
    TextLengthOutOfRange(usize),
    TextStorageTooLarge,
    CallBaseOffsetOutOfRange(usize),
    InvalidCodeTarget(usize),
    InvalidEntry(usize),
    ImageTooLarge,
}

pub(crate) fn encode(image: &StaticImage) -> Result<BytecodeArtifact, EncodeError> {
    let array_count = image.array_lengths.len();
    if array_count > usize::from(u8::MAX) + 1 {
        return Err(EncodeError::TooManyArrays(array_count));
    }
    let mut array_lengths = Vec::with_capacity(array_count);
    let mut array_cells = 0usize;
    for &length in &image.array_lengths {
        array_lengths
            .push(u16::try_from(length).map_err(|_| EncodeError::ArrayLengthOutOfRange(length))?);
        array_cells = array_cells
            .checked_add(length)
            .ok_or(EncodeError::ArrayStorageTooLarge)?;
    }
    array_cells
        .checked_mul(2)
        .filter(|bytes| *bytes <= usize::from(u16::MAX))
        .ok_or(EncodeError::ArrayStorageTooLarge)?;
    array_count
        .checked_mul(4)
        .ok_or(EncodeError::ArrayStorageTooLarge)?;

    let text_count = image.texts.len();
    if text_count > usize::from(u8::MAX) + 1 {
        return Err(EncodeError::TooManyTexts(text_count));
    }
    let mut texts = Vec::with_capacity(text_count);
    let mut text_bytes = 0usize;
    for text in &image.texts {
        let bytes = text.as_bytes();
        let byte_length = u16::try_from(bytes.len())
            .map_err(|_| EncodeError::TextLengthOutOfRange(bytes.len()))?;
        text_bytes = text_bytes
            .checked_add(bytes.len())
            .ok_or(EncodeError::TextStorageTooLarge)?;
        texts.push(FixedText {
            bytes: bytes.to_vec(),
            byte_length,
        });
    }
    text_count
        .checked_mul(4)
        .ok_or(EncodeError::TextStorageTooLarge)?;

    let mut offsets = Vec::with_capacity(image.code.len() + 1);
    let mut byte_len = 0usize;
    for (index, instruction) in image.code.iter().enumerate() {
        offsets.push(byte_len);
        byte_len = byte_len
            .checked_add(encoded_len(instruction, index, image)?)
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
            LogicalInstruction::LoadArray(slot) | LogicalInstruction::StoreArray(slot) => {
                if slot.0 >= image.array_lengths.len() {
                    return Err(EncodeError::ArraySlotOutOfRange(slot.0));
                }
                let slot =
                    u8::try_from(slot.0).map_err(|_| EncodeError::ArraySlotOutOfRange(slot.0))?;
                // 0x12 and 0x13 extend the existing 0x10/0x11 global access pair.
                code.push(if matches!(instruction, LogicalInstruction::LoadArray(_)) {
                    0x12
                } else {
                    0x13
                });
                code.push(slot);
            }
            LogicalInstruction::LoadScratch(slot) | LogicalInstruction::StoreScratch(slot) => {
                code.push(
                    if matches!(instruction, LogicalInstruction::LoadScratch(_)) {
                        0x14
                    } else {
                        0x15
                    },
                );
                code.push(scratch_slot_operand(*slot));
            }
            LogicalInstruction::WriteText(slot) => {
                let slot = text_slot_operand(slot.0, image.texts.len())?;
                code.push(0x63);
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
            LogicalInstruction::ControlPush => code.push(0x70),
            LogicalInstruction::ControlCopy => code.push(0x71),
            LogicalInstruction::ControlDrop => code.push(0x72),
            LogicalInstruction::Return => code.push(0x22),
            LogicalInstruction::Halt => code.push(0x01),
            _ => return Err(EncodeError::UnsupportedInstruction(index)),
        }
    }
    Ok(BytecodeArtifact {
        code,
        entry_offset,
        global_slot_count: image.global_count as u16,
        array_lengths,
        texts,
    })
}

fn encoded_len(
    instruction: &LogicalInstruction,
    index: usize,
    image: &StaticImage,
) -> Result<usize, EncodeError> {
    match instruction {
        LogicalInstruction::PushI16(_) => Ok(3),
        LogicalInstruction::LoadGlobal(_) | LogicalInstruction::StoreGlobal(_) => Ok(2),
        LogicalInstruction::LoadScratch(_) | LogicalInstruction::StoreScratch(_) => Ok(2),
        LogicalInstruction::LoadArray(slot) | LogicalInstruction::StoreArray(slot) => {
            if slot.0 >= image.array_lengths.len() {
                Err(EncodeError::ArraySlotOutOfRange(slot.0))
            } else {
                Ok(2)
            }
        }
        LogicalInstruction::WriteText(slot) => {
            text_slot_operand(slot.0, image.texts.len()).map(|_| 2)
        }
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
        LogicalInstruction::ControlPush
        | LogicalInstruction::ControlCopy
        | LogicalInstruction::ControlDrop
        | LogicalInstruction::Return
        | LogicalInstruction::Halt => Ok(1),
        _ => Err(EncodeError::UnsupportedInstruction(index)),
    }
}

fn scratch_slot_operand(slot: ScratchSlot) -> u8 {
    match slot {
        ScratchSlot::I => 0,
        ScratchSlot::J => 1,
        ScratchSlot::K => 2,
        ScratchSlot::L => 3,
        ScratchSlot::M => 4,
        ScratchSlot::N => 5,
        ScratchSlot::X => 6,
        ScratchSlot::Y => 7,
    }
}

fn text_slot_operand(slot: usize, text_count: usize) -> Result<u8, EncodeError> {
    if slot >= text_count {
        return Err(EncodeError::TextSlotOutOfRange(slot));
    }
    u8::try_from(slot).map_err(|_| EncodeError::TextSlotOutOfRange(slot))
}

fn primitive_opcode(operation: PrimitiveOp) -> Option<u8> {
    Some(match operation {
        PrimitiveOp::Add => 0x40,
        PrimitiveOp::Multiply => 0x41,
        PrimitiveOp::Remainder => 0x42,
        PrimitiveOp::Subtract => 0x43,
        PrimitiveOp::Divide => 0x44,
        PrimitiveOp::Negate => 0x45,
        PrimitiveOp::Abs => 0x46,
        PrimitiveOp::And => 0x47,
        PrimitiveOp::Equal => 0x48,
        PrimitiveOp::Less => 0x49,
        PrimitiveOp::LessEqual => 0x4a,
        PrimitiveOp::GreaterEqual => 0x4b,
        PrimitiveOp::Greater => 0x4c,
        PrimitiveOp::Or => 0x4d,
        PrimitiveOp::Swap => 0x4e,
        PrimitiveOp::NotEqual => 0x4f,
        PrimitiveOp::Drop => 0x50,
        PrimitiveOp::Rnd => 0x51,
        PrimitiveOp::TryInput => 0x52,
        PrimitiveOp::PutDec => 0x60,
        PrimitiveOp::Cr => 0x61,
        PrimitiveOp::PutChr => 0x62,
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

    fn image_with_arrays(code: Vec<LogicalInstruction>, array_lengths: Vec<usize>) -> StaticImage {
        StaticImage {
            array_lengths,
            ..image(code)
        }
    }

    fn image_with_texts(code: Vec<LogicalInstruction>, texts: Vec<String>) -> StaticImage {
        StaticImage {
            texts,
            ..image(code)
        }
    }

    #[test]
    fn encodes_write_text_and_preserves_utf8_metadata_in_slot_order() {
        let artifact = encode(&image_with_texts(
            vec![LogicalInstruction::WriteText(super::super::TextSlot(0))],
            vec![
                "hello".into(),
                "é".into(),
                String::new(),
                "A".into(),
                "A".into(),
            ],
        ))
        .expect("text encodes");

        assert_eq!(artifact.code(), &[0x63, 0]);
        assert_eq!(artifact.text_count(), 5);
        assert_eq!(
            artifact
                .texts()
                .iter()
                .map(|text| text.bytes())
                .collect::<Vec<_>>(),
            vec![
                b"hello".as_slice(),
                "é".as_bytes(),
                b"".as_slice(),
                b"A".as_slice(),
                b"A".as_slice()
            ]
        );
        assert_eq!(
            artifact
                .texts()
                .iter()
                .map(|text| text.byte_length())
                .collect::<Vec<_>>(),
            vec![5, 2, 0, 1, 1]
        );
        assert_eq!(artifact.text_storage_bytes(), Some(9));
        assert_eq!(artifact.text_descriptor_bytes(), Some(20));
    }

    #[test]
    fn supports_256_text_slots_and_rejects_the_257th() {
        let artifact = encode(&image_with_texts(
            vec![LogicalInstruction::WriteText(super::super::TextSlot(255))],
            vec!["x".into(); 256],
        ))
        .expect("256 text slots encode");
        assert_eq!(artifact.code(), &[0x63, 255]);
        assert_eq!(artifact.text_count(), 256);
        assert_eq!(artifact.text_storage_bytes(), Some(256));
        assert_eq!(artifact.text_descriptor_bytes(), Some(1024));

        assert_eq!(
            encode(&image_with_texts(
                vec![LogicalInstruction::Halt],
                vec![String::new(); 257]
            )),
            Err(EncodeError::TooManyTexts(257))
        );
    }

    #[test]
    fn accepts_maximum_text_length_without_limiting_total_text_storage() {
        let artifact = encode(&image_with_texts(
            vec![LogicalInstruction::Halt],
            vec!["x".repeat(usize::from(u16::MAX)), "y".into()],
        ))
        .expect("per-text lengths fit even when total exceeds 64 KiB");
        assert_eq!(artifact.texts()[0].byte_length(), u16::MAX);
        assert_eq!(
            artifact.text_storage_bytes(),
            Some(usize::from(u16::MAX) + 1)
        );
        assert_eq!(artifact.text_descriptor_bytes(), Some(8));
    }

    #[test]
    fn rejects_invalid_text_slots_and_unrepresentable_byte_lengths() {
        assert_eq!(
            encode(&image_with_texts(
                vec![LogicalInstruction::WriteText(super::super::TextSlot(0))],
                Vec::new(),
            )),
            Err(EncodeError::TextSlotOutOfRange(0))
        );
        assert_eq!(
            encode(&image_with_texts(
                vec![LogicalInstruction::WriteText(super::super::TextSlot(1))],
                vec!["x".into()],
            )),
            Err(EncodeError::TextSlotOutOfRange(1))
        );
        assert_eq!(
            encode(&image_with_texts(
                vec![LogicalInstruction::Halt],
                vec!["x".repeat(usize::from(u16::MAX) + 1)],
            )),
            Err(EncodeError::TextLengthOutOfRange(usize::from(u16::MAX) + 1))
        );
    }

    #[test]
    fn write_text_has_fixed_width_in_entry_call_and_jump_relocations() {
        let mut source = image_with_texts(
            vec![
                LogicalInstruction::Jump(CodePosition(4)),
                LogicalInstruction::WriteText(super::super::TextSlot(0)),
                LogicalInstruction::CallCode(CodePosition(1)),
                LogicalInstruction::JumpIfZero(CodePosition(1)),
                LogicalInstruction::WriteText(super::super::TextSlot(0)),
                LogicalInstruction::Return,
            ],
            vec!["x".into()],
        );
        source.entry = CodePosition(1);
        let artifact = encode(&source).expect("relocations encode");
        assert_eq!(artifact.entry_offset(), 3);
        assert_eq!(
            artifact.code(),
            &[0x30, 0x0b, 0x00, 0x63, 0, 0x20, 3, 0, 0x31, 3, 0, 0x63, 0, 0x22]
        );
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
    fn encodes_m34_mandelbrot_primitive_opcodes() {
        let artifact = encode(&image(vec![
            LogicalInstruction::CallPrimitive(PrimitiveOp::Subtract),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Divide),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Negate),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Abs),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Greater),
            LogicalInstruction::CallPrimitive(PrimitiveOp::PutChr),
        ]))
        .expect("M34 Mandelbrot primitives encode");

        assert_eq!(artifact.code(), &[0x43, 0x44, 0x45, 0x46, 0x4c, 0x62]);
    }

    #[test]
    fn encodes_and_or_as_single_byte_primitives() {
        let artifact = encode(&image(vec![
            LogicalInstruction::CallPrimitive(PrimitiveOp::And),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Or),
        ]))
        .expect("AND and OR encode");

        assert_eq!(artifact.code(), &[0x47, 0x4d]);
    }

    #[test]
    fn encodes_swap_and_not_equal_as_single_byte_primitives() {
        let artifact = encode(&image(vec![
            LogicalInstruction::CallPrimitive(PrimitiveOp::Swap),
            LogicalInstruction::CallPrimitive(PrimitiveOp::NotEqual),
        ]))
        .expect("SWAP and NotEqual encode");

        assert_eq!(artifact.code(), &[0x4e, 0x4f]);
    }

    #[test]
    fn encodes_rnd_as_a_single_byte_primitive() {
        let artifact = encode(&image(vec![LogicalInstruction::CallPrimitive(
            PrimitiveOp::Rnd,
        )]))
        .expect("RND encodes");

        assert_eq!(artifact.code(), &[0x51]);
    }

    #[test]
    fn encodes_array_accesses_and_reports_artifact_metadata() {
        let artifact = encode(&image_with_arrays(
            vec![
                LogicalInstruction::LoadArray(super::super::ArraySlot(0)),
                LogicalInstruction::StoreArray(super::super::ArraySlot(1)),
                LogicalInstruction::LoadArray(super::super::ArraySlot(1)),
                LogicalInstruction::Halt,
            ],
            vec![3, 7],
        ))
        .expect("array accesses encode");

        assert_eq!(artifact.code(), &[0x12, 0, 0x13, 1, 0x12, 1, 0x01]);
        assert_eq!(artifact.array_count(), 2);
        assert_eq!(artifact.array_lengths(), &[3, 7]);
        assert_eq!(artifact.array_storage_bytes(), Some(20));
        assert_eq!(artifact.array_descriptor_bytes(), Some(8));
    }

    #[test]
    fn supports_256_arrays_and_rejects_the_257th() {
        let lengths = vec![1; 256];
        let artifact = encode(&image_with_arrays(
            vec![LogicalInstruction::LoadArray(super::super::ArraySlot(255))],
            lengths.clone(),
        ))
        .expect("256 arrays fit in u8 slots");
        assert_eq!(artifact.code(), &[0x12, 255]);
        assert_eq!(artifact.array_count(), 256);
        assert_eq!(artifact.array_descriptor_bytes(), Some(1024));

        assert_eq!(
            encode(&image_with_arrays(
                vec![LogicalInstruction::Halt],
                vec![1; 257]
            )),
            Err(EncodeError::TooManyArrays(257))
        );
    }

    #[test]
    fn rejects_invalid_array_slots_lengths_and_storage_sizes() {
        assert_eq!(
            encode(&image_with_arrays(
                vec![LogicalInstruction::LoadArray(super::super::ArraySlot(0))],
                Vec::new(),
            )),
            Err(EncodeError::ArraySlotOutOfRange(0))
        );
        assert_eq!(
            encode(&image_with_arrays(
                vec![LogicalInstruction::StoreArray(super::super::ArraySlot(1))],
                vec![1]
            )),
            Err(EncodeError::ArraySlotOutOfRange(1))
        );
        assert_eq!(
            encode(&image_with_arrays(
                vec![LogicalInstruction::Halt],
                vec![u16::MAX as usize + 1]
            )),
            Err(EncodeError::ArrayLengthOutOfRange(u16::MAX as usize + 1))
        );
        assert_eq!(
            encode(&image_with_arrays(
                vec![LogicalInstruction::Halt],
                vec![32_768]
            )),
            Err(EncodeError::ArrayStorageTooLarge)
        );
    }

    #[test]
    fn empty_array_metadata_has_zero_sizes() {
        let artifact = encode(&image(vec![LogicalInstruction::Halt])).expect("empty image encodes");
        assert_eq!(artifact.code(), &[0x01]);
        assert_eq!(artifact.entry_offset(), 0);
        assert_eq!(artifact.global_slot_count(), 0);
        assert_eq!(artifact.array_count(), 0);
        assert_eq!(artifact.array_lengths(), &[]);
        assert_eq!(artifact.array_storage_bytes(), Some(0));
        assert_eq!(artifact.array_descriptor_bytes(), Some(0));
        assert_eq!(artifact.text_count(), 0);
        assert!(artifact.texts().is_empty());
        assert_eq!(artifact.text_storage_bytes(), Some(0));
        assert_eq!(artifact.text_descriptor_bytes(), Some(0));

        let one = encode(&image_with_arrays(vec![LogicalInstruction::Halt], vec![4]))
            .expect("one array encodes");
        assert_eq!(one.array_count(), 1);
        assert_eq!(one.array_lengths(), &[4]);
        assert_eq!(one.array_descriptor_bytes(), Some(4));
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
    fn control_value_opcodes_are_single_byte_and_preserve_relocations() {
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(5)),
            LogicalInstruction::ControlPush,
            LogicalInstruction::ControlCopy,
            LogicalInstruction::CallCode(CodePosition(1)),
            LogicalInstruction::JumpIfZero(CodePosition(0)),
            LogicalInstruction::ControlDrop,
            LogicalInstruction::Return,
        ]))
        .expect("control-value opcodes encode");

        assert_eq!(
            artifact.code(),
            &[0x30, 0x0b, 0x00, 0x70, 0x71, 0x20, 0x03, 0x00, 0x31, 0x00, 0x00, 0x72, 0x22]
        );
    }

    #[test]
    fn and_or_widths_are_included_in_forward_and_backward_relocations() {
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(5)),
            LogicalInstruction::CallPrimitive(PrimitiveOp::And),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Or),
            LogicalInstruction::JumpIfZero(CodePosition(0)),
            LogicalInstruction::CallCode(CodePosition(1)),
            LogicalInstruction::Return,
        ]))
        .expect("AND/OR relocation targets resolve");

        assert_eq!(
            artifact.code(),
            &[0x30, 0x0b, 0x00, 0x47, 0x4d, 0x31, 0x00, 0x00, 0x20, 0x03, 0x00, 0x22]
        );
    }

    #[test]
    fn swap_and_not_equal_widths_are_included_in_forward_and_backward_relocations() {
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(5)),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Swap),
            LogicalInstruction::CallPrimitive(PrimitiveOp::NotEqual),
            LogicalInstruction::JumpIfZero(CodePosition(0)),
            LogicalInstruction::CallCode(CodePosition(1)),
            LogicalInstruction::Return,
        ]))
        .expect("SWAP/NotEqual relocation targets resolve");

        assert_eq!(
            artifact.code(),
            &[0x30, 0x0b, 0x00, 0x4e, 0x4f, 0x31, 0x00, 0x00, 0x20, 0x03, 0x00, 0x22]
        );
    }

    #[test]
    fn rnd_width_is_included_in_forward_and_backward_relocations() {
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(4)),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Rnd),
            LogicalInstruction::JumpIfZero(CodePosition(0)),
            LogicalInstruction::CallCode(CodePosition(1)),
            LogicalInstruction::Return,
        ]))
        .expect("RND relocation targets resolve");

        assert_eq!(
            artifact.code(),
            &[0x30, 0x0a, 0x00, 0x51, 0x31, 0x00, 0x00, 0x20, 0x03, 0x00, 0x22]
        );
    }

    #[test]
    fn rejects_unsupported_logical_primitives() {
        assert_eq!(
            encode(&image(vec![LogicalInstruction::CallPrimitive(
                PrimitiveOp::Not
            )]),),
            Err(EncodeError::UnsupportedPrimitive(0))
        );
    }

    #[test]
    fn scratch_slots_have_explicit_operands_and_two_byte_width() {
        use super::super::ScratchSlot;
        let slots = [
            ScratchSlot::I,
            ScratchSlot::J,
            ScratchSlot::K,
            ScratchSlot::L,
            ScratchSlot::M,
            ScratchSlot::N,
            ScratchSlot::X,
            ScratchSlot::Y,
        ];
        let mut code = Vec::new();
        let mut expected = Vec::new();
        for (operand, slot) in slots.into_iter().enumerate() {
            code.push(LogicalInstruction::LoadScratch(slot));
            code.push(LogicalInstruction::StoreScratch(slot));
            expected.extend_from_slice(&[0x14, operand as u8, 0x15, operand as u8]);
        }
        assert_eq!(encode(&image(code)).unwrap().code(), expected);
    }

    #[test]
    fn scratch_width_is_included_in_branch_and_call_targets() {
        use super::super::ScratchSlot;
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(4)),
            LogicalInstruction::LoadScratch(ScratchSlot::N),
            LogicalInstruction::StoreScratch(ScratchSlot::N),
            LogicalInstruction::JumpIfZero(CodePosition(1)),
            LogicalInstruction::CallCode(CodePosition(2)),
            LogicalInstruction::Return,
        ]))
        .unwrap();
        assert_eq!(
            artifact.code(),
            &[0x30, 0x0a, 0x00, 0x14, 5, 0x15, 5, 0x31, 0x03, 0x00, 0x20, 0x05, 0x00, 0x22]
        );
    }

    #[test]
    fn try_input_is_a_single_byte_primitive_and_preserves_relocations() {
        let artifact = encode(&image(vec![
            LogicalInstruction::Jump(CodePosition(3)),
            LogicalInstruction::CallPrimitive(PrimitiveOp::TryInput),
            LogicalInstruction::CallPrimitive(PrimitiveOp::Rnd),
            LogicalInstruction::Return,
        ]))
        .expect("input primitive encodes");

        assert_eq!(artifact.code(), &[0x30, 0x05, 0x00, 0x52, 0x51, 0x22]);
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
