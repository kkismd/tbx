use crate::block_code::{BlockCodeBuildError, BlockCodeBuilder};
use crate::instruction::{Instruction, InstructionAddress};
use crate::published_code::{PublishedWordBuilder, WordBodyBuildError};
use crate::source::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstructionBuildError {
    BlockCodeBuild { source: BlockCodeBuildError },
    WordBodyBuild { source: WordBodyBuildError },
}

impl From<BlockCodeBuildError> for InstructionBuildError {
    fn from(source: BlockCodeBuildError) -> Self {
        Self::BlockCodeBuild { source }
    }
}

pub(crate) trait InstructionBuildTarget {
    fn current_address(&self) -> InstructionAddress {
        InstructionAddress::from_index(self.current_len())
    }

    fn current_len(&self) -> usize;

    fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError>;

    fn append_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError>;

    fn append_resolved_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError>;

    fn append_resolved_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError>;

    fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError>;

    fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError>;

    fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), InstructionBuildError>;

    fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), InstructionBuildError>;
}

impl InstructionBuildTarget for BlockCodeBuilder<'_> {
    fn current_len(&self) -> usize {
        self.current_len()
    }

    fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        BlockCodeBuilder::append_mapped(self, instruction, span)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn append_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        BlockCodeBuilder::append_unmapped(self, instruction)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn append_resolved_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        BlockCodeBuilder::append_resolved_mapped(self, instruction, span)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn append_resolved_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        BlockCodeBuilder::append_resolved_unmapped(self, instruction)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        BlockCodeBuilder::append_mapped_jump_placeholder(self, span)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        BlockCodeBuilder::append_mapped_jump_if_zero_placeholder(self, span)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        BlockCodeBuilder::patch_branch_target(self, branch, target)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }

    fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        BlockCodeBuilder::validate_local_target(self, address)
            .map_err(|source| InstructionBuildError::BlockCodeBuild { source })
    }
}

impl InstructionBuildTarget for PublishedWordBuilder<'_> {
    fn current_len(&self) -> usize {
        self.current_len()
    }

    fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        PublishedWordBuilder::append_mapped(self, instruction, span)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn append_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        PublishedWordBuilder::append_unmapped(self, instruction)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn append_resolved_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        PublishedWordBuilder::append_resolved_mapped(self, instruction, span)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn append_resolved_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        PublishedWordBuilder::append_resolved_unmapped(self, instruction)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        PublishedWordBuilder::append_mapped_jump_placeholder(self, span)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        PublishedWordBuilder::append_mapped_jump_if_zero_placeholder(self, span)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        PublishedWordBuilder::patch_branch_target(self, branch, target)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }

    fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        self.validate_local_target(address)
            .map_err(|source| InstructionBuildError::WordBodyBuild { source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Bindings;
    use crate::block_code::BlockCodeBuildError;
    use crate::instruction::Instruction;
    use crate::name::NormalizedName;
    use crate::published_code::{NewWordPublicationError, PublishedCode};
    use crate::source::{SourceId, SourceTexts, SourceView};
    use crate::source_mapping::SourceMappedCode;
    use crate::value::Value;
    use crate::word::PublishedWords;

    fn name(input: &str) -> NormalizedName {
        NormalizedName::new(input).expect("test input should be a valid word name")
    }

    fn source(text: &str) -> (SourceTexts, SourceId) {
        let mut sources = SourceTexts::new();
        let id = sources.register(text);
        (sources, id)
    }

    fn span(view: SourceView<'_>, source_id: SourceId, start: usize, end: usize) -> SourceSpan {
        view.span(source_id, start, end)
            .expect("test span should be valid")
    }

    fn push(value: i16) -> Instruction {
        Instruction::Push(Value::integer(value))
    }

    fn unwrap_published_build_error(error: InstructionBuildError) -> WordBodyBuildError {
        match error {
            InstructionBuildError::WordBodyBuild { source } => source,
            source => panic!("unexpected target error: {source:?}"),
        }
    }

    fn unwrap_block_build_error(error: InstructionBuildError) -> BlockCodeBuildError {
        match error {
            InstructionBuildError::BlockCodeBuild { source } => source,
            source => panic!("unexpected target error: {source:?}"),
        }
    }

    #[test]
    fn temporary_target_maps_placeholder_and_patches_without_mapping_change() {
        let (sources, source_id) = source("BIF X, 20\n20");
        let branch_span = span(sources.view(), source_id, 0, 3);
        let target_span = span(sources.view(), source_id, 10, 12);
        let mut code = SourceMappedCode::new();

        let mut builder = BlockCodeBuilder::new(&mut code);
        let branch = builder
            .append_mapped_jump_if_zero_placeholder(branch_span)
            .expect("branch placeholder should append");
        let target = builder
            .append_mapped(Instruction::Halt, target_span)
            .expect("target should append");
        builder
            .patch_branch_target(branch, target)
            .expect("branch should patch");
        builder.finish().expect("block should complete");

        assert_eq!(
            code.instruction_view().get(branch),
            Ok(&Instruction::JumpIfZero(target))
        );
        assert_eq!(
            code.source_mapping()
                .source_span(code.instruction_view().location(branch)),
            Ok(Some(branch_span))
        );
    }

    #[test]
    fn published_target_uses_builder_branch_placeholder_contract() {
        let (sources, source_id) = source("BIF X, 20\n20");
        let branch_span = span(sources.view(), source_id, 0, 3);
        let target_span = span(sources.view(), source_id, 10, 12);
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();

        let word = code
            .publish_new_word(&mut words, &mut bindings, name("BRANCH"), |_, builder| {
                let target: &mut dyn InstructionBuildTarget = builder;
                let branch = target
                    .append_mapped_jump_if_zero_placeholder(branch_span)
                    .map_err(unwrap_published_build_error)?;
                let destination = target
                    .append_mapped(Instruction::Return, target_span)
                    .map_err(unwrap_published_build_error)?;
                target
                    .patch_branch_target(branch, destination)
                    .map_err(unwrap_published_build_error)
            })
            .expect("word should publish");

        assert_eq!(
            code.instruction_view().get_location(word.entry()),
            Ok(&Instruction::JumpIfZero(InstructionAddress::from_index(1)))
        );
        assert_eq!(
            code.source_mapping().source_span(word.entry()),
            Ok(Some(branch_span))
        );
    }

    #[test]
    fn published_target_rejects_direct_branch_append_through_common_interface() {
        let (sources, source_id) = source("GOTO");
        let branch_span = span(sources.view(), source_id, 0, 4);
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let branch = Instruction::Jump(InstructionAddress::from_index(0));

        let result = code.publish_new_word(&mut words, &mut bindings, name("BAD"), |_, builder| {
            let target: &mut dyn InstructionBuildTarget = builder;
            target
                .append_mapped(branch, branch_span)
                .map(|_| ())
                .map_err(unwrap_published_build_error)
        });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::BranchInstructionRequiresPatch {
                    instruction: branch
                }
            })
        );
    }

    #[test]
    fn published_target_rejects_patch_to_previous_body_address() {
        let (sources, source_id) = source("GOTO OLD");
        let branch_span = span(sources.view(), source_id, 0, 4);
        let mut code = PublishedCode::new();
        let mut words = PublishedWords::new();
        let mut bindings = Bindings::new();
        let old = code
            .publish_new_word(&mut words, &mut bindings, name("OLD"), |_, builder| {
                builder.append_unmapped(push(1))?;
                builder.append_unmapped(Instruction::Return)?;
                Ok(())
            })
            .expect("old word should publish");

        let result = code.publish_new_word(&mut words, &mut bindings, name("NEW"), |_, builder| {
            let target: &mut dyn InstructionBuildTarget = builder;
            let branch = target
                .append_mapped_jump_placeholder(branch_span)
                .map_err(unwrap_published_build_error)?;
            target
                .patch_branch_target(branch, old.entry().address())
                .map_err(unwrap_published_build_error)
        });

        assert_eq!(
            result,
            Err(NewWordPublicationError::Build {
                source: WordBodyBuildError::AddressOutsideCurrentBody {
                    address: old.entry().address()
                }
            })
        );
    }

    #[test]
    fn block_target_rejects_direct_branch_append_through_common_interface() {
        let mut code = SourceMappedCode::new();
        let mut builder = BlockCodeBuilder::new(&mut code);
        let branch = Instruction::Jump(InstructionAddress::from_index(0));
        let target: &mut dyn InstructionBuildTarget = &mut builder;

        let result = target
            .append_unmapped(branch)
            .map(|_| ())
            .map_err(unwrap_block_build_error);

        assert_eq!(
            result,
            Err(BlockCodeBuildError::BranchInstructionRequiresPatch {
                instruction: branch
            })
        );
    }
}
