use crate::block_code::{BlockCodeBuildError, BlockCodeBuilder, CompletedBlockCode};
use crate::instruction::{Instruction, InstructionAddress, InstructionView};
use crate::source::SourceSpan;
use crate::source_mapping::{
    InstructionSourceMappingView, SourceMappedCode, SourceMappingLookupError,
};

/// Immutable unpublished code artifact for a static quotation.
///
/// Per #1516/#1518, a quotation is completed as local source-mapped block code,
/// but it is not a published runtime code space and carries no `WordId` or
/// executable entry binding. Parent attachment explicitly re-expresses local
/// branch targets in the parent's instruction owner.
#[derive(Debug)]
pub(crate) struct StaticQuotation {
    code: SourceMappedCode,
    completed: CompletedBlockCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StaticQuotationBuildError {
    Build { source: BlockCodeBuildError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StaticQuotationAttachError {
    LocalSourceMapping { source: SourceMappingLookupError },
    InvalidLocalTarget { target: InstructionAddress },
    TargetRebaseOverflow { target: InstructionAddress },
    ParentAppend { source: BlockCodeBuildError },
}

impl StaticQuotation {
    pub(crate) fn build(
        build: impl FnOnce(&mut BlockCodeBuilder<'_>) -> Result<(), BlockCodeBuildError>,
    ) -> Result<Self, StaticQuotationBuildError> {
        let mut code = SourceMappedCode::new();
        let completed = {
            let mut builder = BlockCodeBuilder::new(&mut code);
            build(&mut builder).map_err(|source| StaticQuotationBuildError::Build { source })?;
            builder
                .finish()
                .map_err(|source| StaticQuotationBuildError::Build { source })?
        };

        Ok(Self { code, completed })
    }

    pub(crate) fn try_build<E>(
        build: impl FnOnce(&mut BlockCodeBuilder<'_>) -> Result<(), E>,
    ) -> Result<Self, E>
    where
        E: From<StaticQuotationBuildError>,
    {
        let mut code = SourceMappedCode::new();
        let completed = {
            let mut builder = BlockCodeBuilder::new(&mut code);
            build(&mut builder)?;
            builder
                .finish()
                .map_err(|source| StaticQuotationBuildError::Build { source })?
        };

        Ok(Self { code, completed })
    }

    pub(crate) fn attach_to(
        &self,
        parent: &mut BlockCodeBuilder<'_>,
    ) -> Result<(), StaticQuotationAttachError> {
        let parent_start = parent.current_address();
        let instructions = self.rebased_instructions(parent_start)?;

        for MappedInstruction { instruction, span } in instructions {
            if let Some(span) = span {
                parent
                    .append_resolved_mapped(instruction, span)
                    .map_err(|source| StaticQuotationAttachError::ParentAppend { source })?;
            } else {
                parent
                    .append_resolved_unmapped(instruction)
                    .map_err(|source| StaticQuotationAttachError::ParentAppend { source })?;
            }
        }

        Ok(())
    }

    pub(crate) fn len(&self) -> usize {
        self.completed.len()
    }

    pub(crate) fn instruction_view(&self) -> InstructionView<'_> {
        self.code.instruction_view()
    }

    pub(crate) fn source_mapping(&self) -> InstructionSourceMappingView<'_> {
        self.code.source_mapping()
    }

    fn rebased_instructions(
        &self,
        parent_start: InstructionAddress,
    ) -> Result<Vec<MappedInstruction>, StaticQuotationAttachError> {
        let mut rebased = Vec::with_capacity(self.completed.len());

        for offset in 0..self.completed.len() {
            let local = InstructionAddress::from_index(self.completed.entry().as_index() + offset);
            let (instruction, span) = self
                .code
                .mapped_instruction(local)
                .map_err(|source| StaticQuotationAttachError::LocalSourceMapping { source })?;
            let instruction = self.rebase_instruction(*instruction, parent_start)?;
            rebased.push(MappedInstruction { instruction, span });
        }

        Ok(rebased)
    }

    fn rebase_instruction(
        &self,
        instruction: Instruction,
        parent_start: InstructionAddress,
    ) -> Result<Instruction, StaticQuotationAttachError> {
        match instruction {
            Instruction::Jump(target) => self
                .rebase_target(target, parent_start)
                .map(Instruction::Jump),
            Instruction::JumpIfZero(target) => self
                .rebase_target(target, parent_start)
                .map(Instruction::JumpIfZero),
            Instruction::Push(_)
            | Instruction::LoadVar(_)
            | Instruction::StoreVar(_)
            | Instruction::Call(_)
            | Instruction::Return
            | Instruction::Halt => Ok(instruction),
        }
    }

    fn rebase_target(
        &self,
        target: InstructionAddress,
        parent_start: InstructionAddress,
    ) -> Result<InstructionAddress, StaticQuotationAttachError> {
        if !self.completed.contains(target) && target != self.completed.end() {
            return Err(StaticQuotationAttachError::InvalidLocalTarget { target });
        }

        let local_offset = target.as_index() - self.completed.entry().as_index();
        let parent_index = parent_start
            .as_index()
            .checked_add(local_offset)
            .ok_or(StaticQuotationAttachError::TargetRebaseOverflow { target })?;

        Ok(InstructionAddress::from_index(parent_index))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MappedInstruction {
    instruction: Instruction,
    span: Option<SourceSpan>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_code::BlockCodeBuildError;
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

    fn push(value: i16) -> Instruction {
        Instruction::Push(Value::integer(value))
    }

    fn build_parent(
        build: impl FnOnce(&mut BlockCodeBuilder<'_>) -> Result<(), BlockCodeBuildError>,
    ) -> SourceMappedCode {
        let mut code = SourceMappedCode::new();
        {
            let mut builder = BlockCodeBuilder::new(&mut code);
            build(&mut builder).expect("parent build should succeed");
            builder.finish().expect("parent block should complete");
        }
        code
    }

    #[test]
    fn empty_quotation_completes_and_attaches_without_parent_instructions() {
        let quotation = StaticQuotation::build(|_| Ok(())).expect("empty quotation completes");
        let parent =
            build_parent(|builder| quotation.attach_to(builder).map_err(|_| unreachable!()));

        assert_eq!(quotation.len(), 0);
        assert_eq!(parent.len(), 0);
    }

    #[test]
    fn mapped_and_unmapped_instructions_attach_in_order() {
        let (sources, source_id) = source("A B");
        let first_span = span(sources.view(), source_id, 0, 1);
        let quotation = StaticQuotation::build(|builder| {
            builder.append_mapped(push(1), first_span)?;
            builder.append_unmapped(Instruction::Return)?;
            Ok(())
        })
        .expect("quotation completes");

        let parent =
            build_parent(|builder| quotation.attach_to(builder).map_err(|_| unreachable!()));

        assert_eq!(parent.instruction_view().get(address(0)), Ok(&push(1)));
        assert_eq!(
            parent.instruction_view().get(address(1)),
            Ok(&Instruction::Return)
        );
        assert_eq!(
            parent
                .source_mapping()
                .source_span(parent.instruction_view().location(address(0))),
            Ok(Some(first_span))
        );
        assert_eq!(
            parent
                .source_mapping()
                .source_span(parent.instruction_view().location(address(1))),
            Ok(None)
        );
    }

    #[test]
    fn resolved_forward_and_backward_branches_rebase_to_parent_start() {
        let quotation = StaticQuotation::build(|builder| {
            let forward = builder.append_unmapped_jump_placeholder()?;
            let backward_target = builder.append_unmapped(push(1))?;
            let backward = builder.append_unmapped_jump_if_zero_placeholder()?;
            let forward_target = builder.append_unmapped(Instruction::Return)?;
            builder.patch_branch_target(forward, forward_target)?;
            builder.patch_branch_target(backward, backward_target)?;
            Ok(())
        })
        .expect("quotation completes");

        let parent = build_parent(|builder| {
            builder.append_unmapped(push(99))?;
            quotation.attach_to(builder).map_err(|_| unreachable!())?;
            Ok(())
        });

        assert_eq!(
            parent.instruction_view().get(address(1)),
            Ok(&Instruction::Jump(address(4)))
        );
        assert_eq!(
            parent.instruction_view().get(address(3)),
            Ok(&Instruction::JumpIfZero(address(2)))
        );
    }

    #[test]
    fn unresolved_branch_rejects_quotation_completion() {
        let error = StaticQuotation::build(|builder| {
            builder.append_unmapped_jump_placeholder()?;
            Ok(())
        })
        .expect_err("unresolved quotation must not complete");

        assert_eq!(
            error,
            StaticQuotationBuildError::Build {
                source: BlockCodeBuildError::UnresolvedBranchPatch { branch: address(0) }
            }
        );
    }

    #[test]
    fn invalid_local_branch_target_rejects_attachment_without_parent_mutation() {
        let mut code = SourceMappedCode::new();
        code.append_unmapped(Instruction::Jump(address(99)))
            .expect("test quotation instruction should append");
        let quotation = StaticQuotation {
            code,
            completed: CompletedBlockCode::test_new(address(0), address(1)),
        };
        let mut parent_code = SourceMappedCode::new();
        let mut parent = BlockCodeBuilder::new(&mut parent_code);
        parent
            .append_unmapped(push(10))
            .expect("prefix should append");

        assert_eq!(
            quotation.attach_to(&mut parent),
            Err(StaticQuotationAttachError::InvalidLocalTarget {
                target: address(99)
            })
        );
        assert_eq!(parent.current_len(), 1);
        parent.finish().expect("parent should still complete");
        assert_eq!(parent_code.len(), 1);
    }

    #[test]
    fn invalid_completed_range_rejects_attachment_without_parent_mutation() {
        let mut code = SourceMappedCode::new();
        code.append_unmapped(push(1))
            .expect("test quotation instruction should append");
        let quotation = StaticQuotation {
            code,
            completed: CompletedBlockCode::test_new(address(0), address(2)),
        };
        let mut parent_code = SourceMappedCode::new();
        let mut parent = BlockCodeBuilder::new(&mut parent_code);

        assert!(matches!(
            quotation.attach_to(&mut parent),
            Err(StaticQuotationAttachError::LocalSourceMapping { .. })
        ));
        assert_eq!(parent.current_len(), 0);
        parent.finish().expect("parent should still complete");
        assert_eq!(parent_code.len(), 0);
    }

    #[test]
    fn nested_quotations_attach_sequentially() {
        let child = StaticQuotation::build(|builder| {
            builder.append_unmapped(push(7))?;
            Ok(())
        })
        .expect("child quotation completes");
        let parent_quotation = StaticQuotation::build(|builder| {
            builder.append_unmapped(push(1))?;
            child.attach_to(builder).map_err(|_| unreachable!())?;
            builder.append_unmapped(Instruction::Return)?;
            Ok(())
        })
        .expect("parent quotation completes");

        let parent = build_parent(|builder| {
            parent_quotation
                .attach_to(builder)
                .map_err(|_| unreachable!())
        });

        assert_eq!(parent.instruction_view().get(address(0)), Ok(&push(1)));
        assert_eq!(parent.instruction_view().get(address(1)), Ok(&push(7)));
        assert_eq!(
            parent.instruction_view().get(address(2)),
            Ok(&Instruction::Return)
        );
    }
}
