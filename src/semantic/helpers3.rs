#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn expression_uses_super(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Super => true,
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { value: operand, .. }
        | ExpressionKind::Length { operand }
        | ExpressionKind::SizeOf { operand } => expression_uses_super(operand),
        ExpressionKind::Binary { left, right, .. } => {
            expression_uses_super(left) || expression_uses_super(right)
        }
        ExpressionKind::Call { callee, arguments } => {
            expression_uses_super(callee) || arguments.iter().any(expression_uses_super)
        }
        ExpressionKind::Member { object, .. } => expression_uses_super(object),
        ExpressionKind::Index { object, index } => {
            expression_uses_super(object) || expression_uses_super(index)
        }
        ExpressionKind::Vector { values }
        | ExpressionKind::New {
            arguments: values, ..
        } => values.iter().any(expression_uses_super),
        _ => false,
    }
}
pub(crate) fn expression_has_invalid_super(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Super => true,
        ExpressionKind::Call { callee, arguments }
            if matches!(callee.kind, ExpressionKind::Super)
                || matches!(callee.kind, ExpressionKind::Member { ref object, .. } if matches!(object.kind, ExpressionKind::Super)) =>
        {
            arguments.iter().any(expression_has_invalid_super)
        }
        ExpressionKind::Call { callee, arguments } => {
            expression_has_invalid_super(callee)
                || arguments.iter().any(expression_has_invalid_super)
        }
        ExpressionKind::Member { object, .. } => {
            matches!(object.kind, ExpressionKind::Super) || expression_has_invalid_super(object)
        }
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { value: operand, .. }
        | ExpressionKind::Length { operand }
        | ExpressionKind::SizeOf { operand } => expression_has_invalid_super(operand),
        ExpressionKind::Binary { left, right, .. } => {
            expression_has_invalid_super(left) || expression_has_invalid_super(right)
        }
        ExpressionKind::Index { object, index } => {
            expression_has_invalid_super(object) || expression_has_invalid_super(index)
        }
        ExpressionKind::Vector { values }
        | ExpressionKind::New {
            arguments: values, ..
        } => values.iter().any(expression_has_invalid_super),
        _ => false,
    }
}
