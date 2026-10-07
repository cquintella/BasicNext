// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What each instruction defines, reads, and types: the operand tables the
//! validator's definite-assignment, type, and ownership checks share.

use bn_types::Type;

pub(super) fn instruction_defines(instruction: &crate::Instruction) -> Option<crate::ValueId> {
    match instruction {
        crate::Instruction::Constant { destination, .. }
        | crate::Instruction::Default { destination, .. }
        | crate::Instruction::Phi { destination, .. }
        | crate::Instruction::Load { destination, .. }
        | crate::Instruction::Copy { destination, .. }
        | crate::Instruction::Unary { destination, .. }
        | crate::Instruction::Binary { destination, .. }
        | crate::Instruction::Cast { destination, .. }
        | crate::Instruction::Call { destination, .. }
        | crate::Instruction::DispatchSubmit { destination, .. }
        | crate::Instruction::DispatchAwait { destination, .. }
        | crate::Instruction::Input { destination, .. }
        | crate::Instruction::Vector { destination, .. }
        | crate::Instruction::Index { destination, .. }
        | crate::Instruction::Member { destination, .. }
        | crate::Instruction::Length { destination, .. }
        | crate::Instruction::SizeOf { destination, .. }
        | crate::Instruction::Allocate { destination, .. }
        | crate::Instruction::Retain { destination, .. }
        | crate::Instruction::Take { destination, .. }
        | crate::Instruction::TakeMember { destination, .. }
        | crate::Instruction::LoadStatic { destination, .. } => Some(*destination),
        crate::Instruction::Store { previous, .. }
        | crate::Instruction::SetIndex { previous, .. }
        | crate::Instruction::SetMemberIndex { previous, .. }
        | crate::Instruction::SetFieldIndex { previous, .. }
        | crate::Instruction::SetStaticIndex { previous, .. }
        | crate::Instruction::SetMember { previous, .. }
        | crate::Instruction::SetField { previous, .. }
        | crate::Instruction::StoreStatic { previous, .. } => *previous,
        crate::Instruction::Print { .. }
        | crate::Instruction::ClearScreen { .. }
        | crate::Instruction::Beep { .. }
        | crate::Instruction::Release { .. }
        | crate::Instruction::EndBinding { .. }
        | crate::Instruction::EnsureClass { .. } => None,
    }
}

/// Enumerates every SSA operand read by an instruction.
#[must_use]
pub fn instruction_uses(instruction: &crate::Instruction) -> Vec<crate::ValueId> {
    match instruction {
        crate::Instruction::Copy { source, .. }
        | crate::Instruction::Unary {
            operand: source, ..
        }
        | crate::Instruction::Cast { value: source, .. }
        | crate::Instruction::Length { vector: source, .. }
        | crate::Instruction::SizeOf { value: source, .. }
        | crate::Instruction::Store { value: source, .. }
        | crate::Instruction::Retain { value: source, .. }
        | crate::Instruction::Release { value: source, .. } => vec![*source],
        crate::Instruction::Binary { left, right, .. } => vec![*left, *right],
        crate::Instruction::Call {
            callee, arguments, ..
        } => {
            let mut used = vec![*callee];
            used.extend(arguments.iter().copied());
            used
        }
        crate::Instruction::DispatchSubmit {
            callee,
            queue,
            task,
            arguments,
            ..
        } => {
            let mut used = vec![*callee, *queue, *task];
            used.extend(arguments.iter().copied());
            used
        }
        crate::Instruction::DispatchAwait {
            callee,
            ticket,
            timeout,
            ..
        } => vec![*callee, *ticket, *timeout],
        crate::Instruction::Vector { values, .. } | crate::Instruction::Print { values, .. } => {
            values.clone()
        }
        crate::Instruction::Index { object, index, .. } => vec![*object, *index],
        crate::Instruction::Member { object, .. }
        | crate::Instruction::TakeMember { object, .. }
        | crate::Instruction::ClearScreen {
            console: object, ..
        }
        | crate::Instruction::Beep {
            console: object, ..
        } => vec![*object],
        crate::Instruction::SetIndex { indices, value, .. }
        | crate::Instruction::SetFieldIndex { indices, value, .. }
        | crate::Instruction::SetStaticIndex { indices, value, .. } => {
            let mut used = indices.clone();
            used.push(*value);
            used
        }
        crate::Instruction::SetMemberIndex {
            object,
            indices,
            value,
            ..
        } => {
            let mut used = vec![*object];
            used.extend(indices.iter().copied());
            used.push(*value);
            used
        }
        crate::Instruction::SetMember { object, value, .. } => vec![*object, *value],
        crate::Instruction::SetField { value, .. }
        | crate::Instruction::StoreStatic { value, .. } => vec![*value],
        crate::Instruction::Allocate { arguments, .. } => arguments.clone(),
        crate::Instruction::Default {
            dynamic_dimensions, ..
        } => dynamic_dimensions.clone(),
        crate::Instruction::Phi { incoming, .. } => {
            incoming.iter().map(|(_, value)| *value).collect()
        }
        crate::Instruction::Input { prompt, .. } => prompt.iter().copied().collect(),
        crate::Instruction::EnsureClass { .. }
        | crate::Instruction::LoadStatic { .. }
        | crate::Instruction::Constant { .. }
        | crate::Instruction::Take { .. }
        | crate::Instruction::EndBinding { .. }
        | crate::Instruction::Load { .. } => Vec::new(),
    }
}

pub(super) fn instruction_type(instruction: &crate::Instruction) -> Option<&Type> {
    match instruction {
        crate::Instruction::Constant { ty, .. }
        | crate::Instruction::Default { ty, .. }
        | crate::Instruction::Phi { ty, .. }
        | crate::Instruction::Load { ty, .. }
        | crate::Instruction::Copy { ty, .. }
        | crate::Instruction::Unary { ty, .. }
        | crate::Instruction::Binary { ty, .. }
        | crate::Instruction::Cast { ty, .. }
        | crate::Instruction::Call { ty, .. }
        | crate::Instruction::DispatchSubmit { ty, .. }
        | crate::Instruction::DispatchAwait { ty, .. }
        | crate::Instruction::Input { ty, .. }
        | crate::Instruction::Vector { ty, .. }
        | crate::Instruction::Index { ty, .. }
        | crate::Instruction::Member { ty, .. }
        | crate::Instruction::Allocate { ty, .. }
        | crate::Instruction::Retain { ty, .. }
        | crate::Instruction::Take { ty, .. }
        | crate::Instruction::TakeMember { ty, .. }
        | crate::Instruction::LoadStatic { ty, .. } => Some(ty),
        // A write defines its previous content, of the written type.
        crate::Instruction::Store { ty, previous, .. }
        | crate::Instruction::SetIndex { ty, previous, .. }
        | crate::Instruction::SetMemberIndex { ty, previous, .. }
        | crate::Instruction::SetFieldIndex { ty, previous, .. }
        | crate::Instruction::SetStaticIndex { ty, previous, .. }
        | crate::Instruction::SetMember { ty, previous, .. }
        | crate::Instruction::SetField { ty, previous, .. }
        | crate::Instruction::StoreStatic { ty, previous, .. } => previous.as_ref().map(|_| ty),
        _ => None,
    }
}

/// The value an instruction defines and its type, if it defines one.
#[must_use]
pub fn instruction_result(instruction: &crate::Instruction) -> Option<(crate::ValueId, &Type)> {
    Some((
        instruction_defines(instruction)?,
        instruction_type(instruction)?,
    ))
}
