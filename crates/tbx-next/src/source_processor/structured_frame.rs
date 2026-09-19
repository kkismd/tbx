use std::cell::RefCell;
use std::rc::Rc;

use crate::block_code::BlockCodeBuildError;
use crate::instruction::{Instruction, InstructionAddress};
use crate::instruction_builder::{InstructionBuildError, InstructionBuildTarget};
use crate::line_number::LocalLineNumberTable;
use crate::source::SourceSpan;
use crate::source_mapping::SourceMappedCode;
use crate::source_word::{
    NativeStructuredSourceWordOwner, SourceWordSyntaxMarker, StructuredBodyCapabilities,
    StructuredBuildTargetScope, StructuredLineNumberScope, StructuredOwnerLocalTarget,
};
use crate::structured_grammar::GrammarProgress;

use super::SourceProcessorError;

pub(super) struct StructuredSourceFrame {
    pub(super) syntax_markers: Vec<SourceWordSyntaxMarker>,
    pub(super) progress: GrammarProgress,
    pub(super) owner: Box<dyn NativeStructuredSourceWordOwner>,
    pub(super) enclosing_target: BuildTargetHandle,
    pub(super) body_target: BuildTargetHandle,
    pub(super) enclosing_capabilities: StructuredBodyCapabilities,
    pub(super) body_capabilities: StructuredBodyCapabilities,
    pub(super) enclosing_line_numbers: Rc<RefCell<LocalLineNumberTable>>,
    pub(super) current_line_numbers: Rc<RefCell<LocalLineNumberTable>>,
    owner_line_numbers: Vec<OwnerLocalLineNumberScope>,
    owner_targets: Vec<Rc<RefCell<OwnerLocalBuildTarget>>>,
    pub(super) exit_target: bool,
    pub(super) control_value_ownership: usize,
    pub(super) pending_exit_branches: Vec<InstructionAddress>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StructuredExitMetadata {
    pub(super) exit_target: bool,
    pub(super) control_value_ownership: usize,
}

#[derive(Debug, Clone)]
pub(super) enum BuildTargetHandle {
    Parent,
    OwnerLocal(Rc<RefCell<OwnerLocalBuildTarget>>),
}

#[derive(Debug)]
struct OwnerLocalLineNumberScope {
    table: Rc<RefCell<LocalLineNumberTable>>,
    target: BuildTargetHandle,
}

#[derive(Debug)]
pub(super) struct OwnerLocalBuildTarget {
    code: SourceMappedCode,
    unresolved_patches: Vec<InstructionAddress>,
}

pub(super) struct SharedOwnerLocalBuildTarget {
    pub(super) target: Rc<RefCell<OwnerLocalBuildTarget>>,
}

impl StructuredSourceFrame {
    pub(super) fn new(
        syntax_markers: Vec<SourceWordSyntaxMarker>,
        progress: GrammarProgress,
        owner: Box<dyn NativeStructuredSourceWordOwner>,
        enclosing_target: BuildTargetHandle,
        enclosing_line_numbers: Rc<RefCell<LocalLineNumberTable>>,
        enclosing_capabilities: StructuredBodyCapabilities,
        exit_metadata: StructuredExitMetadata,
    ) -> Self {
        Self {
            syntax_markers,
            progress,
            owner,
            enclosing_target: enclosing_target.clone(),
            body_target: enclosing_target,
            enclosing_capabilities,
            body_capabilities: enclosing_capabilities,
            current_line_numbers: enclosing_line_numbers.clone(),
            enclosing_line_numbers,
            owner_line_numbers: Vec::new(),
            owner_targets: Vec::new(),
            exit_target: exit_metadata.exit_target,
            control_value_ownership: exit_metadata.control_value_ownership,
            pending_exit_branches: Vec::new(),
        }
    }

    pub(super) fn apply_owner_context(&mut self) {
        let context = self.owner.current_body_context();
        self.body_target = match context.build_target() {
            StructuredBuildTargetScope::Enclosing => self.enclosing_target.clone(),
            StructuredBuildTargetScope::OwnerLocal(index) => {
                self.owner_target(index);
                BuildTargetHandle::OwnerLocal(self.owner_targets[index].clone())
            }
        };
        self.body_capabilities = context
            .capabilities()
            .intersect(self.enclosing_capabilities);
        self.current_line_numbers = match context.line_number_scope() {
            StructuredLineNumberScope::Enclosing => self.enclosing_line_numbers.clone(),
            StructuredLineNumberScope::OwnerLocal(index) => {
                self.owner_line_number_scope(index).table.clone()
            }
        };
    }

    pub(super) fn resolve_owner_line_numbers(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
    ) -> Result<(), SourceProcessorError> {
        for scope in &mut self.owner_line_numbers {
            let mut owner_target;
            let target = match &scope.target {
                BuildTargetHandle::Parent => &mut *code,
                BuildTargetHandle::OwnerLocal(target) => {
                    owner_target = SharedOwnerLocalBuildTarget {
                        target: target.clone(),
                    };
                    &mut owner_target as &mut dyn InstructionBuildTarget
                }
            };
            scope.table.borrow_mut().resolve(target).map_err(|source| {
                SourceProcessorError::from(super::line_number_compile_error(source))
            })?;
        }
        Ok(())
    }

    fn owner_target(&mut self, index: usize) -> Rc<RefCell<OwnerLocalBuildTarget>> {
        while self.owner_targets.len() <= index {
            self.owner_targets
                .push(Rc::new(RefCell::new(OwnerLocalBuildTarget::new())));
        }
        self.owner_targets[index].clone()
    }

    fn owner_line_number_scope(&mut self, index: usize) -> &OwnerLocalLineNumberScope {
        while self.owner_line_numbers.len() <= index {
            self.owner_line_numbers.push(OwnerLocalLineNumberScope {
                table: Rc::new(RefCell::new(LocalLineNumberTable::new())),
                target: self.body_target.clone(),
            });
        }
        &self.owner_line_numbers[index]
    }

    pub(super) fn owner_local_target_snapshots(
        &self,
    ) -> Result<Vec<StructuredOwnerLocalTarget>, SourceProcessorError> {
        self.owner_targets
            .iter()
            .map(|target| target.borrow().snapshot())
            .collect()
    }

    pub(super) fn patch_pending_exit_branches(
        &mut self,
        code: &mut dyn InstructionBuildTarget,
    ) -> Result<(), SourceProcessorError> {
        let target = code.current_address();
        for branch in self.pending_exit_branches.drain(..) {
            code.patch_branch_target(branch, target)?;
        }
        Ok(())
    }
}

impl OwnerLocalBuildTarget {
    fn new() -> Self {
        Self {
            code: SourceMappedCode::new(),
            unresolved_patches: Vec::new(),
        }
    }

    fn current_len(&self) -> usize {
        self.code.len()
    }

    fn append_branch_placeholder(
        &mut self,
        instruction: Instruction,
        span: Option<SourceSpan>,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        let branch = match span {
            Some(span) => self.code.append_mapped(instruction, span),
            None => self.code.append_unmapped(instruction),
        }
        .map_err(|source| InstructionBuildError::BlockCodeBuild {
            source: BlockCodeBuildError::SourceMappingAppend { source },
        })?;
        self.unresolved_patches.push(branch);
        Ok(branch)
    }

    fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        self.validate_local_target(branch)?;
        self.validate_local_branch_target(target)?;
        let Some(position) = self
            .unresolved_patches
            .iter()
            .position(|pending| *pending == branch)
        else {
            return Err(InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::UnknownBranchPatch { branch },
            });
        };
        self.code
            .patch_branch_target(branch, target)
            .map_err(|source| InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::BranchTargetPatch { source },
            })?;
        self.unresolved_patches.swap_remove(position);
        Ok(())
    }

    fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        if address.as_index() >= self.code.len() {
            return Err(InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::AddressOutsideCurrentBlock { address },
            });
        }
        Ok(())
    }

    fn validate_local_branch_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        if address.as_index() > self.code.len() {
            return Err(InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::AddressOutsideCurrentBlock { address },
            });
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<StructuredOwnerLocalTarget, SourceProcessorError> {
        if let Some(branch) = self.unresolved_patches.first().copied() {
            return Err(InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::UnresolvedBranchPatch { branch },
            }
            .into());
        }

        let mut instructions = Vec::with_capacity(self.code.len());
        for index in 0..self.code.len() {
            let address = InstructionAddress::from_index(index);
            let (instruction, span) = self.code.mapped_instruction(address)?;
            instructions.push((instruction.clone(), span));
        }
        Ok(StructuredOwnerLocalTarget::new(instructions))
    }
}

impl InstructionBuildTarget for SharedOwnerLocalBuildTarget {
    fn current_len(&self) -> usize {
        self.target.borrow().current_len()
    }

    fn append_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        reject_owner_local_direct_branch(&instruction)?;
        self.target
            .borrow_mut()
            .code
            .append_mapped(instruction, span)
            .map_err(|source| InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::SourceMappingAppend { source },
            })
    }

    fn append_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        reject_owner_local_direct_branch(&instruction)?;
        self.target
            .borrow_mut()
            .code
            .append_unmapped(instruction)
            .map_err(|source| InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::SourceMappingAppend { source },
            })
    }

    fn append_resolved_mapped(
        &mut self,
        instruction: Instruction,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        self.target
            .borrow_mut()
            .code
            .append_mapped(instruction, span)
            .map_err(|source| InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::SourceMappingAppend { source },
            })
    }

    fn append_resolved_unmapped(
        &mut self,
        instruction: Instruction,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        self.target
            .borrow_mut()
            .code
            .append_unmapped(instruction)
            .map_err(|source| InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::SourceMappingAppend { source },
            })
    }

    fn append_mapped_jump_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        self.target.borrow_mut().append_branch_placeholder(
            Instruction::Jump(InstructionAddress::from_index(0)),
            Some(span),
        )
    }

    fn append_mapped_jump_if_zero_placeholder(
        &mut self,
        span: SourceSpan,
    ) -> Result<InstructionAddress, InstructionBuildError> {
        self.target.borrow_mut().append_branch_placeholder(
            Instruction::JumpIfZero(InstructionAddress::from_index(0)),
            Some(span),
        )
    }

    fn patch_branch_target(
        &mut self,
        branch: InstructionAddress,
        target: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        self.target.borrow_mut().patch_branch_target(branch, target)
    }

    fn validate_local_target(
        &self,
        address: InstructionAddress,
    ) -> Result<(), InstructionBuildError> {
        self.target.borrow().validate_local_target(address)
    }
}

fn reject_owner_local_direct_branch(
    instruction: &Instruction,
) -> Result<(), InstructionBuildError> {
    match instruction {
        Instruction::Jump(_) | Instruction::JumpIfZero(_) => {
            Err(InstructionBuildError::BlockCodeBuild {
                source: BlockCodeBuildError::BranchInstructionRequiresPatch {
                    instruction: instruction.clone(),
                },
            })
        }
        Instruction::Push(_)
        | Instruction::WriteFixedText(_)
        | Instruction::LoadVar(_)
        | Instruction::StoreVar(_)
        | Instruction::LoadArrayElement(_)
        | Instruction::StoreArrayElement(_)
        | Instruction::Call(_)
        | Instruction::CopyFromCallBase { .. }
        | Instruction::TruncateDataStackToCallBase
        | Instruction::PushControlValue
        | Instruction::CopyControlValue
        | Instruction::DropControlValue
        | Instruction::Return
        | Instruction::Halt => Ok(()),
    }
}
