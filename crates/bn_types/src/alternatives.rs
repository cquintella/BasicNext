// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stable codes for the members of an alternative type. A backend that tags
//! an alternative stores the code of the member it holds; because the code
//! depends only on the member, the order of an alternative's members does
//! not matter and widening an alternative (0.6.md, "Alternative types:
//! identity and assignment") keeps the tag.

use crate::{FloatType, IntegerType, Type};

/// Code of an object member (class, `STRUCT`, interface, HOST handle). Two
/// object members of one alternative share it; their class name tells them
/// apart at run time.
pub const OBJECT: u32 = 0x20;
/// Code of a function-value member.
pub const FUNCTION: u32 = 0x21;
/// Base of vector member codes: `VECTOR + code of the element`.
pub const VECTOR: u32 = 0x100;
/// Base of pointer member codes: `POINTER + code of the element`.
pub const POINTER: u32 = 0x200;

/// The code of `member`, or `None` for a type that is not a storable member
/// (an unresolved literal, a module, a nested alternative, a vector of
/// vectors).
#[must_use]
pub fn member_code(member: &Type) -> Option<u32> {
    let code = match member {
        Type::Integer(width) => match width {
            IntegerType::Byte => 0x01,
            IntegerType::Int8 => 0x02,
            IntegerType::Int16 => 0x03,
            IntegerType::Int32 => 0x04,
            IntegerType::Int64 => 0x05,
            IntegerType::UInt16 => 0x06,
            IntegerType::UInt32 => 0x07,
            IntegerType::UInt64 => 0x08,
        },
        Type::Float(FloatType::Float32) => 0x09,
        Type::Float(FloatType::Float64) => 0x0A,
        Type::Boolean => 0x0B,
        Type::String => 0x0C,
        Type::Null => 0x0D,
        Type::NotAvailable => 0x0E,
        Type::EndOfFile => 0x0F,
        Type::Named(name) | Type::TypeName(name) => named_code(name),
        Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } => {
            if name == "Error" { 0x10 } else { OBJECT }
        }
        Type::Function { .. } => FUNCTION,
        Type::Vector { element, .. } => VECTOR + element_code(element)?,
        Type::Pointer { element, .. } => POINTER + element_code(element)?,
        _ => return None,
    };
    Some(code)
}

fn named_code(name: &str) -> u32 {
    match name {
        "Error" => 0x10,
        "DATE" => 0x11,
        "TIME" => 0x12,
        "TIMEZONE" => 0x13,
        _ => OBJECT,
    }
}

/// An element code fits below `VECTOR`, so vectors and pointers of vectors
/// have no code.
fn element_code(element: &Type) -> Option<u32> {
    member_code(element).filter(|code| *code < VECTOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar_members() -> Vec<Type> {
        let mut members = [
            IntegerType::Byte,
            IntegerType::Int8,
            IntegerType::Int16,
            IntegerType::Int32,
            IntegerType::Int64,
            IntegerType::UInt16,
            IntegerType::UInt32,
            IntegerType::UInt64,
        ]
        .map(Type::Integer)
        .to_vec();
        members.extend([
            Type::Float(FloatType::Float32),
            Type::Float(FloatType::Float64),
            Type::Boolean,
            Type::String,
            Type::Null,
            Type::NotAvailable,
            Type::EndOfFile,
            Type::Named("Error".into()),
            Type::Named("DATE".into()),
            Type::Named("TIME".into()),
            Type::Named("TIMEZONE".into()),
            Type::Named("Box".into()),
        ]);
        members
    }

    #[test]
    fn every_distinct_member_kind_has_its_own_code() {
        let codes = scalar_members()
            .iter()
            .map(|member| member_code(member).expect("storable member"))
            .collect::<Vec<_>>();
        let unique = codes.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), codes.len(), "codes collide: {codes:?}");
    }

    #[test]
    fn vectors_and_pointers_carry_their_element() {
        let vector = |element: Type| Type::Vector {
            element: Box::new(element),
            dimensions: vec![u64::MAX],
        };
        let ints = vector(Type::Integer(IntegerType::Int32));
        let strings = vector(Type::String);
        assert_ne!(member_code(&ints), member_code(&strings));
        assert_eq!(member_code(&ints), Some(VECTOR + 0x04));
        assert_eq!(
            member_code(&vector(ints)),
            None,
            "no code for nested vectors"
        );
    }

    #[test]
    fn imported_and_local_spellings_share_a_code() {
        let imported_error = Type::ImportedNamed {
            module: crate::ModuleId(3),
            name: "Error".into(),
        };
        assert_eq!(
            member_code(&imported_error),
            member_code(&Type::Named("Error".into()))
        );
        assert_eq!(member_code(&Type::IntegerLiteral("1".into())), None);
    }
}
