#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn block_is_cyclic(function: &Function, candidate: BlockId) -> bool {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id.0, block))
        .collect::<HashMap<_, _>>();
    let Some(block) = blocks.get(&candidate.0) else {
        return false;
    };
    let mut pending = match block.terminator {
        Terminator::Jump { target } => vec![target.0],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block.0, else_block.0],
        Terminator::Return { .. } | Terminator::Stop { .. } => Vec::new(),
    };
    let mut visited = HashSet::new();
    while let Some(block_id) = pending.pop() {
        if block_id == candidate.0 {
            return true;
        }
        if !visited.insert(block_id) {
            continue;
        }
        let Some(block) = blocks.get(&block_id) else {
            continue;
        };
        match block.terminator {
            Terminator::Jump { target } => pending.push(target.0),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(then_block.0);
                pending.push(else_block.0);
            }
            Terminator::Return { .. } | Terminator::Stop { .. } => {}
        }
    }
    false
}

pub(crate) fn llvm_vector_dimension_supported(length: usize) -> bool {
    length <= usize::try_from(u32::MAX).unwrap_or(usize::MAX)
}

pub(crate) fn instruction_destination(instruction: &Instruction) -> Option<ValueId> {
    match instruction {
        Instruction::Constant { destination, .. }
        | Instruction::Default { destination, .. }
        | Instruction::Phi { destination, .. }
        | Instruction::Load { destination, .. }
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
        | Instruction::EnsureClass { .. }
        | Instruction::StoreStatic { .. } => None,
    }
}
