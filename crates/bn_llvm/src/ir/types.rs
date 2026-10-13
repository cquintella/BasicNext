// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Strongly typed representations of LLVM types and their canonical formatting.

use std::fmt;

/// Representation of an LLVM IR type.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LlvmType {
    Void,
    I1,
    I8,
    I16,
    I32,
    I64,
    I128,
    Float,
    Double,
    Ptr,
    Array(usize, Box<LlvmType>),
    Struct(Vec<LlvmType>),
}

impl LlvmType {
    /// The bit width of an integer type (`i1` … `i128`).
    #[must_use]
    pub const fn integer_bits(&self) -> Option<u32> {
        match self {
            Self::I1 => Some(1),
            Self::I8 => Some(8),
            Self::I16 => Some(16),
            Self::I32 => Some(32),
            Self::I64 => Some(64),
            Self::I128 => Some(128),
            _ => None,
        }
    }

    /// Whether the type is a BN integer layout: `i8`, `i16`, `i32` or `i64`.
    #[must_use]
    pub const fn is_integer(&self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    /// Whether the type is `float` or `double`.
    #[must_use]
    pub const fn is_floating(&self) -> bool {
        matches!(self, Self::Float | Self::Double)
    }
}

impl fmt::Display for LlvmType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::I1 => write!(f, "i1"),
            Self::I8 => write!(f, "i8"),
            Self::I16 => write!(f, "i16"),
            Self::I32 => write!(f, "i32"),
            Self::I64 => write!(f, "i64"),
            Self::I128 => write!(f, "i128"),
            Self::Float => write!(f, "float"),
            Self::Double => write!(f, "double"),
            Self::Ptr => write!(f, "ptr"),
            Self::Array(len, elem) => write!(f, "[{len} x {elem}]"),
            Self::Struct(fields) => {
                write!(f, "{{ ")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field}")?;
                }
                write!(f, " }}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_types_display() {
        assert_eq!(LlvmType::Void.to_string(), "void");
        assert_eq!(LlvmType::I1.to_string(), "i1");
        assert_eq!(LlvmType::I8.to_string(), "i8");
        assert_eq!(LlvmType::I16.to_string(), "i16");
        assert_eq!(LlvmType::I32.to_string(), "i32");
        assert_eq!(LlvmType::I64.to_string(), "i64");
        assert_eq!(LlvmType::I128.to_string(), "i128");
        assert_eq!(LlvmType::Float.to_string(), "float");
        assert_eq!(LlvmType::Double.to_string(), "double");
        assert_eq!(LlvmType::Ptr.to_string(), "ptr");
    }

    #[test]
    fn composite_types_display() {
        let arr = LlvmType::Array(10, Box::new(LlvmType::I32));
        assert_eq!(arr.to_string(), "[10 x i32]");

        let tuple = LlvmType::Struct(vec![LlvmType::I1, LlvmType::Double]);
        assert_eq!(tuple.to_string(), "{ i1, double }");

        let nested = LlvmType::Struct(vec![
            LlvmType::I1,
            LlvmType::Struct(vec![LlvmType::Ptr, LlvmType::I64]),
        ]);
        assert_eq!(nested.to_string(), "{ i1, { ptr, i64 } }");
    }
}
