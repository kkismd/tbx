use crate::instruction::{BranchTargetPatchError, InstructionAddress, InstructionAddressError};
use crate::source::SourceSpan;
use crate::source_mapping::SourceMappedCode;
use std::collections::HashMap;

/// Owner-local compile-time line number identifier.
///
/// ADR #1456 keeps line numbers out of runtime `Value`. This type is used only
/// by builders that resolve local control-flow targets inside one instruction
/// owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LocalLineNumber {
    raw: u64,
}

#[derive(Debug, Default)]
pub(crate) struct LocalLineNumberTable {
    definitions: HashMap<LocalLineNumber, LineNumberDefinition>,
    patches: Vec<UnresolvedLineNumberPatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineNumberDefinition {
    target: InstructionAddress,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnresolvedLineNumberPatch {
    line_number: LocalLineNumber,
    branch: InstructionAddress,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineNumberError {
    Duplicate {
        line_number: LocalLineNumber,
        original_span: SourceSpan,
        duplicate_span: SourceSpan,
    },
    Undefined {
        line_number: LocalLineNumber,
        span: SourceSpan,
    },
    InvalidDefinitionTarget {
        line_number: LocalLineNumber,
        span: SourceSpan,
        source: InstructionAddressError,
    },
    Patch {
        line_number: LocalLineNumber,
        span: SourceSpan,
        source: BranchTargetPatchError,
    },
}

impl LocalLineNumber {
    pub(crate) const fn new(raw: u64) -> Self {
        Self { raw }
    }

    pub(crate) const fn raw(self) -> u64 {
        self.raw
    }
}

impl LocalLineNumberTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn define(
        &mut self,
        code: &SourceMappedCode,
        line_number: LocalLineNumber,
        target: InstructionAddress,
        span: SourceSpan,
    ) -> Result<(), LineNumberError> {
        code.validate_address(target).map_err(|source| {
            LineNumberError::InvalidDefinitionTarget {
                line_number,
                span,
                source,
            }
        })?;

        if let Some(existing) = self.definitions.get(&line_number) {
            return Err(LineNumberError::Duplicate {
                line_number,
                original_span: existing.span,
                duplicate_span: span,
            });
        }

        self.definitions
            .insert(line_number, LineNumberDefinition { target, span });
        Ok(())
    }

    pub(crate) fn add_patch(
        &mut self,
        line_number: LocalLineNumber,
        branch: InstructionAddress,
        span: SourceSpan,
    ) {
        self.patches.push(UnresolvedLineNumberPatch {
            line_number,
            branch,
            span,
        });
    }

    pub(crate) fn resolve(&self, code: &mut SourceMappedCode) -> Result<(), LineNumberError> {
        let mut resolved = Vec::with_capacity(self.patches.len());
        for patch in &self.patches {
            let Some(definition) = self.definitions.get(&patch.line_number) else {
                return Err(LineNumberError::Undefined {
                    line_number: patch.line_number,
                    span: patch.span,
                });
            };
            resolved.push((*patch, definition.target));
        }

        for (patch, target) in resolved {
            code.patch_branch_target(patch.branch, target)
                .map_err(|source| LineNumberError::Patch {
                    line_number: patch.line_number,
                    span: patch.span,
                    source,
                })?;
        }

        Ok(())
    }
}

impl LineNumberError {
    pub(crate) const fn primary_span(self) -> SourceSpan {
        match self {
            Self::Duplicate { duplicate_span, .. } => duplicate_span,
            Self::Undefined { span, .. }
            | Self::InvalidDefinitionTarget { span, .. }
            | Self::Patch { span, .. } => span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{Instruction, InstructionAddress};
    use crate::source::{SourceId, SourceTexts, SourceView};
    use crate::value::Value;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn address(index: usize) -> InstructionAddress {
        InstructionAddress::from_index(index)
    }

    fn line(raw: u64) -> LocalLineNumber {
        LocalLineNumber::new(raw)
    }

    fn append(code: &mut SourceMappedCode, instruction: Instruction) -> InstructionAddress {
        code.append_unmapped(instruction)
            .expect("test instruction should append")
    }

    #[test]
    fn resolves_same_owner_forward_line_number_patch() {
        let (sources, source_id) = source("BIF 0, 200\n200 DONE");
        let branch_span = span(sources.view(), source_id, 7, 10);
        let target_span = span(sources.view(), source_id, 11, 14);
        let mut code = SourceMappedCode::new();
        let placeholder = append(&mut code, Instruction::Halt);
        let branch = append(&mut code, Instruction::JumpIfZero(placeholder));
        let target = append(&mut code, Instruction::Halt);
        let mut table = LocalLineNumberTable::new();

        table.add_patch(line(200), branch, branch_span);
        table
            .define(&code, line(200), target, target_span)
            .expect("target should define");
        table.resolve(&mut code).expect("patch should resolve");

        assert_eq!(
            code.instruction_view().get(branch),
            Ok(&Instruction::JumpIfZero(target))
        );
    }

    #[test]
    fn resolves_same_owner_backward_line_number_patch() {
        let (sources, source_id) = source("100 START\nBIF 0, 100");
        let target_span = span(sources.view(), source_id, 0, 3);
        let branch_span = span(sources.view(), source_id, 17, 20);
        let mut code = SourceMappedCode::new();
        let target = append(&mut code, Instruction::Halt);
        let branch = append(&mut code, Instruction::Jump(address(0)));
        let mut table = LocalLineNumberTable::new();

        table
            .define(&code, line(100), target, target_span)
            .expect("target should define");
        table.add_patch(line(100), branch, branch_span);
        table.resolve(&mut code).expect("patch should resolve");

        assert_eq!(
            code.instruction_view().get(branch),
            Ok(&Instruction::Jump(target))
        );
    }

    #[test]
    fn duplicate_line_number_reports_both_spans() {
        let (sources, source_id) = source("100 A\n100 B");
        let first_span = span(sources.view(), source_id, 0, 3);
        let second_span = span(sources.view(), source_id, 6, 9);
        let mut code = SourceMappedCode::new();
        let target = append(&mut code, Instruction::Halt);
        let mut table = LocalLineNumberTable::new();

        table
            .define(&code, line(100), target, first_span)
            .expect("first definition should succeed");

        assert_eq!(
            table.define(&code, line(100), target, second_span),
            Err(LineNumberError::Duplicate {
                line_number: line(100),
                original_span: first_span,
                duplicate_span: second_span,
            })
        );
    }

    #[test]
    fn undefined_line_number_reports_operand_span() {
        let (sources, source_id) = source("BIF 0, 200");
        let branch_span = span(sources.view(), source_id, 7, 10);
        let mut code = SourceMappedCode::new();
        let branch = append(&mut code, Instruction::JumpIfZero(address(0)));
        let mut table = LocalLineNumberTable::new();

        table.add_patch(line(200), branch, branch_span);

        assert_eq!(
            table.resolve(&mut code),
            Err(LineNumberError::Undefined {
                line_number: line(200),
                span: branch_span,
            })
        );
    }

    #[test]
    fn line_number_identifier_is_not_limited_to_runtime_integer_range() {
        let large = line(u64::from(i16::MAX as u16) + 1);
        let (sources, source_id) = source("40000 TARGET");
        let target_span = span(sources.view(), source_id, 0, 5);
        let mut code = SourceMappedCode::new();
        let target = append(&mut code, Instruction::Halt);
        let mut table = LocalLineNumberTable::new();

        table
            .define(&code, large, target, target_span)
            .expect("large line number should be compile-time-only");

        assert_eq!(large.raw(), 32768);
    }

    #[test]
    fn definition_rejects_end_and_out_of_range_targets() {
        let (sources, source_id) = source("100 A");
        let line_span = span(sources.view(), source_id, 0, 3);
        let mut code = SourceMappedCode::new();
        append(&mut code, Instruction::Halt);
        let mut table = LocalLineNumberTable::new();

        assert_eq!(
            table.define(&code, line(100), address(1), line_span),
            Err(LineNumberError::InvalidDefinitionTarget {
                line_number: line(100),
                span: line_span,
                source: InstructionAddressError::EndAddress {
                    address: address(1)
                },
            })
        );
        assert_eq!(
            table.define(&code, line(100), address(2), line_span),
            Err(LineNumberError::InvalidDefinitionTarget {
                line_number: line(100),
                span: line_span,
                source: InstructionAddressError::InvalidAddress {
                    address: address(2)
                },
            })
        );
    }

    #[test]
    fn separate_owners_can_reuse_the_same_line_number() {
        let (sources, source_id) = source("100 A\n100 B");
        let first_span = span(sources.view(), source_id, 0, 3);
        let second_span = span(sources.view(), source_id, 6, 9);
        let mut first_code = SourceMappedCode::new();
        let mut second_code = SourceMappedCode::new();
        let first_target = append(&mut first_code, Instruction::Halt);
        let second_target = append(&mut second_code, Instruction::Push(Value::integer(1)));
        let mut first_table = LocalLineNumberTable::new();
        let mut second_table = LocalLineNumberTable::new();

        first_table
            .define(&first_code, line(100), first_target, first_span)
            .expect("first owner should define line");
        second_table
            .define(&second_code, line(100), second_target, second_span)
            .expect("second owner should define same local line");
    }

    #[test]
    fn patch_error_keeps_line_number_operand_span() {
        let (sources, source_id) = source("BIF 0, 100");
        let branch_span = span(sources.view(), source_id, 7, 10);
        let target_span = span(sources.view(), source_id, 7, 10);
        let mut code = SourceMappedCode::new();
        let non_branch = append(&mut code, Instruction::Push(Value::integer(1)));
        let target = append(&mut code, Instruction::Halt);
        let mut table = LocalLineNumberTable::new();

        table
            .define(&code, line(100), target, target_span)
            .expect("line should define");
        table.add_patch(line(100), non_branch, branch_span);

        assert_eq!(
            table.resolve(&mut code),
            Err(LineNumberError::Patch {
                line_number: line(100),
                span: branch_span,
                source: BranchTargetPatchError::NonBranchInstruction {
                    address: non_branch,
                    instruction: Instruction::Push(Value::integer(1)),
                },
            })
        );
    }
}
