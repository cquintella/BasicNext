#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn llvm_vector_dimension_supported(length: usize) -> bool {
    length <= usize::try_from(u32::MAX).unwrap_or(usize::MAX)
}

pub(crate) fn instruction_destination(instruction: &Instruction) -> Option<ValueId> {
    match instruction {
        Instruction::Constant { destination, .. }
        | Instruction::Default { destination, .. }
        | Instruction::Phi { destination, .. }
        | Instruction::Load { destination, .. }
        | Instruction::Retain { destination, .. }
        | Instruction::Take { destination, .. }
        | Instruction::TakeMember { destination, .. }
        | Instruction::Copy { destination, .. }
        | Instruction::Unary { destination, .. }
        | Instruction::Binary { destination, .. }
        | Instruction::Cast { destination, .. }
        | Instruction::Call { destination, .. }
        | Instruction::DispatchSubmit { destination, .. }
        | Instruction::DispatchAwait { destination, .. }
        | Instruction::Input { destination, .. }
        | Instruction::Vector { destination, .. }
        | Instruction::Index { destination, .. }
        | Instruction::Member { destination, .. }
        | Instruction::Length { destination, .. }
        | Instruction::SizeOf { destination, .. }
        | Instruction::Allocate { destination, .. }
        | Instruction::LoadStatic { destination, .. } => Some(*destination),
        Instruction::Store { .. }
        | Instruction::SetIndex { .. }
        | Instruction::SetMemberIndex { .. }
        | Instruction::SetFieldIndex { .. }
        | Instruction::SetStaticIndex { .. }
        | Instruction::SetMember { .. }
        | Instruction::SetField { .. }
        | Instruction::Print { .. }
        | Instruction::ClearScreen { .. }
        | Instruction::Beep { .. }
        | Instruction::Release { .. }
        | Instruction::EndBinding { .. }
        | Instruction::EnsureClass { .. }
        | Instruction::StoreStatic { .. } => None,
    }
}
