// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Which declared alternative a numeric literal takes (0.6.md, "Numeric
//! literals in alternative types"). Semantic analysis accepts or rejects the
//! store with it; the backends store the value as that alternative.

use crate::{FloatType, IntegerType, Type};

/// Integer or floating point: the class a numeric literal belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericClass {
    Integer,
    Float,
}

impl NumericClass {
    /// The class of a literal type (`IntegerLiteral`, `FloatLiteral`).
    #[must_use]
    pub const fn of_literal(ty: &Type) -> Option<Self> {
        match ty {
            Type::IntegerLiteral(_) => Some(Self::Integer),
            Type::FloatLiteral => Some(Self::Float),
            _ => None,
        }
    }

    /// `INTEGER` (`INT32`) or `FLOAT` (`FLOAT64`).
    #[must_use]
    pub const fn default_type(self) -> Type {
        match self {
            Self::Integer => Type::Integer(IntegerType::Int32),
            Self::Float => Type::Float(FloatType::Float64),
        }
    }

    const fn contains(self, ty: &Type) -> bool {
        matches!(
            (self, ty),
            (Self::Integer, Type::Integer(_)) | (Self::Float, Type::Float(_))
        )
    }
}

/// The alternative a literal of `class` takes in `alternatives`: its default
/// type when declared, else the only alternative of its class, else none
/// (the store is a `TYPE_MISMATCH`).
#[must_use]
pub fn numeric_alternative(alternatives: &[Type], class: NumericClass) -> Option<&Type> {
    let default = class.default_type();
    if let Some(found) = alternatives.iter().find(|ty| **ty == default) {
        return Some(found);
    }
    let mut same_class = alternatives.iter().filter(|ty| class.contains(ty));
    match (same_class.next(), same_class.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error() -> Type {
        Type::Named("Error".into())
    }

    #[test]
    fn default_type_wins_then_the_only_alternative_of_the_class() {
        let float32 = Type::Float(FloatType::Float32);
        let float64 = Type::Float(FloatType::Float64);
        let int32 = Type::Integer(IntegerType::Int32);
        let int64 = Type::Integer(IntegerType::Int64);
        let alternatives = [float32.clone(), float64.clone()];
        assert_eq!(
            numeric_alternative(&alternatives, NumericClass::Float),
            Some(&float64)
        );
        let alternatives = [int64.clone(), int32.clone()];
        assert_eq!(
            numeric_alternative(&alternatives, NumericClass::Integer),
            Some(&int32)
        );
        let alternatives = [int64.clone(), error()];
        assert_eq!(
            numeric_alternative(&alternatives, NumericClass::Integer),
            Some(&int64)
        );
        let alternatives = [float32.clone(), int32];
        assert_eq!(
            numeric_alternative(&alternatives, NumericClass::Float),
            Some(&float32)
        );
    }

    #[test]
    fn ambiguous_or_missing_class_takes_no_alternative() {
        let alternatives = [
            Type::Integer(IntegerType::Int8),
            Type::Integer(IntegerType::Int16),
        ];
        assert_eq!(
            numeric_alternative(&alternatives, NumericClass::Integer),
            None
        );
        let alternatives = [Type::String, error()];
        assert_eq!(
            numeric_alternative(&alternatives, NumericClass::Float),
            None
        );
    }
}
