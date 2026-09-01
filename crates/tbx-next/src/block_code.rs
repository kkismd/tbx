use crate::instruction::{BranchTargetPatchError, Instruction, InstructionAddress};
use crate::source::SourceSpan;
use crate::source_mapping::{SourceMappedCode, SourceMappingAppendError};

/// Common unpublished builder for one owner-local instruction block.
///
/// Per #1518, completing a block only proves its local instruction/mapping and
/// branch-patch contract. Runtime word publication remains a separate boundary.
#[derive(Debug)]
pub(crate) struct BlockCodeBuilder<'a> {
    code: &'a mut SourceMappedCode,
    block_start: InstructionAddress,
    unresolved_patches: Vec<InstructionAddress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletedBlockCode {
    entry: InstructionAddress,
    end: InstructionAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockCodeBuildError {
    SourceMappingAppend { source: SourceMappingAppendError },
    BranchTargetPatch { source: BranchTargetPatchError },
    BranchInstructionRequiresPatch { instruction: Instruction },
    AddressOutsideCurrentBlock { address: InstructionAddress },
    UnknownBranchPatch { branch: InstructionAddress },
    UnresolvedBranchPatch { branch: InstructionAddress },
}

impl<'a> BlockCodeBuilder<'a> {
    pub(crate) fn new(code: &'a mut SourceMappedCode) -> Self {
        let block_start = InstructionAddress::from_index(code.len());
        Self {
            code,
            block_start,
            unresolved_patches: Vec::new(),
        }
    }

    pub(crate) fn current_address(&self) -> InstructionAddress {
        InstructionAddress::from_index(self.code.len())
    }

    pub(crate) fn current_len(&self) -> usize {
        self.code.len()
    }

    pub(crate) fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        reject_direct_branch_instruction(instruction)?;
        self.code
            .append_mapped(instruction, span)
            .map_err(|source| BlockCodeBuildError::SourceMappingAppend { source })
    }

    pub(crate) fn append_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        reject_direct_branch_instruction(instruction)?;
        self.code
            .append_unmapped(instruction)
            .map_err(|source| BlockCodeBuildError::SourceMappingAppend { source })
    }

    pub(crate) fn append_resolved_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        // #1516/#1518: completed child artifacts may attach already-resolved
        // branches after rebasing. Ordinary builders must still use placeholders
        // so unresolved patches cannot masquerade as completed block code.
        self.code
            .append_mapped(instruction, span)
            .map_err(|source| BlockCodeBuildError::SourceMappingAppend { source })
    }

    pub(crate) fn append_resolved_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        // See `append_resolved_mapped`; this is only for validated attachment of
        // completed local code, not for ordinary branch construction.
        self.code
            .append_unmapped(instruction)
            .map_err(|source| BlockCodeBuildError::SourceMappingAppend { source })
    }

    pub(crate) fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        self.append_branch_placeholder(
            Instruction::Jump(InstructionAddress::from_index(0)),
            Some(span),
        )
    }

    pub(crate) fn append_unmapped_jump_placeholder(
        &mut self,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        self.append_branch_placeholder(Instruction::Jump(InstructionAddress::from_index(0)), None)
    }

    pub(crate) fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        self.append_branch_placeholder(
            Instruction::JumpIfZero(InstructionAddress::from_index(0)),
            Some(span),
        )
    }

    pub(crate) fn append_unmapped_jump_if_zero_placeholder(
        &mut self,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        self.append_branch_placeholder(
            Instruction::JumpIfZero(InstructionAddress::from_index(0)),
            None,
        )
    }

    pub(crate) fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), BlockCodeBuildError> {
        self.validate_local_target(branch)?;
        self.validate_local_branch_target(target)?;
        let Some(position) = self
            .unresolved_patches
            .iter()
            .position(|pending| *pending == branch)
        else {
            return Err(BlockCodeBuildError::UnknownBranchPatch { branch });
        };

        self.code
            .patch_branch_target(branch, target)
            .map_err(|source| BlockCodeBuildError::BranchTargetPatch { source })?;

        self.unresolved_patches.swap_remove(position);
        Ok(())
    }

    pub(crate) fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), BlockCodeBuildError> {
        if address.as_index() < self.block_start.as_index() || address.as_index() >= self.code.len()
        {
            return Err(BlockCodeBuildError::AddressOutsideCurrentBlock { address });
        }

        Ok(())
    }

    fn validate_local_branch_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), BlockCodeBuildError> {
        if address.as_index() < self.block_start.as_index() || address.as_index() > self.code.len()
        {
            return Err(BlockCodeBuildError::AddressOutsideCurrentBlock { address });
        }

        Ok(())
    }

    pub(crate) fn finish(self) -> Result<CompletedBlockCode, BlockCodeBuildError> {
        if let Some(branch) = self.unresolved_patches.first().copied() {
            return Err(BlockCodeBuildError::UnresolvedBranchPatch { branch });
        }

        Ok(CompletedBlockCode {
            entry: self.block_start,
            end: self.current_address(),
        })
    }

    fn append_branch_placeholder(
        &mut self,
        instruction: Instruction,
        span: Option<SourceSpan>,
    ) -> Result<InstructionAddress, BlockCodeBuildError> {
        let branch = if let Some(span) = span {
            self.code.append_mapped(instruction, span)
        } else {
            self.code.append_unmapped(instruction)
        }
        .map_err(|source| BlockCodeBuildError::SourceMappingAppend { source })?;

        self.unresolved_patches.push(branch);
        Ok(branch)
    }
}

impl CompletedBlockCode {
    pub(crate) const fn entry(self) -> InstructionAddress {
        self.entry
    }

    #[cfg(test)]
    pub(crate) const fn test_new(entry: InstructionAddress, end: InstructionAddress) -> Self {
        Self { entry, end }
    }

    pub(crate) const fn end(self) -> InstructionAddress {
        self.end
    }

    pub(crate) fn len(self) -> usize {
        self.end.as_index() - self.entry.as_index()
    }

    pub(crate) fn contains(self, address: InstructionAddress) -> bool {
        address.as_index() >= self.entry.as_index() && address.as_index() < self.end.as_index()
    }
}

fn reject_direct_branch_instruction(instruction: Instruction) -> Result<(), BlockCodeBuildError> {
    match instruction {
        Instruction::Jump(_) | Instruction::JumpIfZero(_) => {
            Err(BlockCodeBuildError::BranchInstructionRequiresPatch { instruction })
        }
        Instruction::Push(_)
        | Instruction::LoadVar(_)
        | Instruction::StoreVar(_)
        | Instruction::Call(_)
        | Instruction::Return
        | Instruction::Halt => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceId, SourceTexts, SourceView};
    use crate::value::Value;

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text, "test.tbx");
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

    #[test]
    fn mapped_and_unmapped_instructions_keep_local_address_mapping_order() {
        let (sources, source_id) = source("A B");
        let first_span = span(sources.view(), source_id, 0, 1);
        let mut code = SourceMappedCode::new();

        let completed = {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let first = builder
                .append_mapped(push(1), first_span)
                .expect("mapped instruction should append");
            let second = builder
                .append_unmapped(Instruction::Return)
                .expect("unmapped instruction should append");
            assert_eq!(first, address(0));
            assert_eq!(second, address(1));
            builder.finish().expect("block should complete")
        };

        assert_eq!(completed.entry(), address(0));
        assert_eq!(code.len(), code.source_mapping().len());
        assert_eq!(code.instruction_view().get(address(0)), Ok(&push(1)));
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(address(0))),
            Ok(Some(first_span))
        );
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(address(1))),
            Ok(None)
        );
    }

    #[test]
    fn branch_placeholder_can_patch_and_complete() {
        let (sources, source_id) = source("BIF X, 20\n20");
        let branch_span = span(sources.view(), source_id, 0, 3);
        let target_span = span(sources.view(), source_id, 10, 12);
        let mut code = SourceMappedCode::new();

        let branch = {
            let mut builder = BlockCodeBuilder::new(&mut code);
            let branch = builder
                .append_mapped_jump_if_zero_placeholder(branch_span)
                .expect("placeholder should append");
            let target = builder
                .append_mapped(Instruction::Halt, target_span)
                .expect("target should append");
            builder
                .patch_branch_target(branch, target)
                .expect("branch should patch");
            builder.finish().expect("block should complete");
            branch
        };

        assert_eq!(
            code.instruction_view().get(branch),
            Ok(&Instruction::JumpIfZero(address(1)))
        );
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(branch)),
            Ok(Some(branch_span))
        );
    }

    #[test]
    fn direct_branch_append_is_rejected() {
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);
        let branch = Instruction::Jump(address(0));

        assert_eq!(
            builder.append_unmapped(branch),
            Err(BlockCodeBuildError::BranchInstructionRequiresPatch {
                instruction: branch
            })
        );
    }

    #[test]
    fn patch_target_must_be_inside_current_block() {
        let mut code = SourceMappedCode::new();
        {
            let mut first = BlockCodeBuilder::new(&mut code);
            first
                .append_unmapped(Instruction::Return)
                .expect("old instruction should append");
            first.finish().expect("old block should complete");
        }

        let mut second = BlockCodeBuilder::new(&mut code);
        let branch = second
            .append_unmapped_jump_placeholder()
            .expect("placeholder should append");
        assert_eq!(
            second.patch_branch_target(branch, address(0)),
            Err(BlockCodeBuildError::AddressOutsideCurrentBlock {
                address: address(0)
            })
        );
    }

    #[test]
    fn unknown_and_duplicate_patches_are_rejected() {
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);
        let branch = builder
            .append_unmapped_jump_placeholder()
            .expect("placeholder should append");
        let target = builder
            .append_unmapped(Instruction::Return)
            .expect("target should append");

        builder
            .patch_branch_target(branch, target)
            .expect("first patch should succeed");
        assert_eq!(
            builder.patch_branch_target(branch, target),
            Err(BlockCodeBuildError::UnknownBranchPatch { branch })
        );
    }

    #[test]
    fn unresolved_patch_rejects_completion() {
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);
        let branch = builder
            .append_unmapped_jump_placeholder()
            .expect("placeholder should append");
        builder
            .append_unmapped(Instruction::Return)
            .expect("target should append");

        assert_eq!(
            builder.finish(),
            Err(BlockCodeBuildError::UnresolvedBranchPatch { branch })
        );
    }
}
