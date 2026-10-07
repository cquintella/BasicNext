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
    Vector(usize, Box<LlvmType>),
}

impl LlvmType {
    /// Returns a conventional natural alignment in bytes for this type.
    #[must_use]
    pub fn default_alignment(&self) -> usize {
        match self {
            Self::I1 | Self::I8 => 1,
            Self::I16 => 2,
            Self::I32 | Self::Float => 4,
            Self::I128 => 16,
            Self::I64 | Self::Double | Self::Ptr => 8,
            Self::Array(_, elem) => elem.default_alignment(),
            Self::Struct(fields) => fields
                .iter()
                .map(Self::default_alignment)
                .max()
                .unwrap_or(1),
            Self::Vector(len, elem) => (elem.default_alignment() * len).next_power_of_two(),
            Self::Void => 1,
        }
    }

    /// Parses a canonical LLVM type string (e.g. "i32", "ptr", "{ i1, double }").
    #[must_use]
    pub fn parse_canonical(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        match trimmed {
            "void" => Some(Self::Void),
            "i1" => Some(Self::I1),
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "i128" => Some(Self::I128),
            "float" => Some(Self::Float),
            "double" => Some(Self::Double),
            "ptr" => Some(Self::Ptr),
            _ if trimmed.starts_with('{') && trimmed.ends_with('}') => {
                let inner = &trimmed[1..trimmed.len() - 1].trim();
                if inner.is_empty() {
                    return Some(Self::Struct(Vec::new()));
                }
                let mut fields = Vec::new();
                for part in split_top_level_comma(inner) {
                    fields.push(Self::parse_canonical(part.trim())?);
                }
                Some(Self::Struct(fields))
            }
            _ if trimmed.starts_with('[') && trimmed.ends_with(']') => {
                let inner = &trimmed[1..trimmed.len() - 1].trim();
                let parts: Vec<&str> = inner.splitn(2, " x ").collect();
                if parts.len() == 2 {
                    let len = parts[0].trim().parse::<usize>().ok()?;
                    let elem = Self::parse_canonical(parts[1].trim())?;
                    Some(Self::Array(len, Box::new(elem)))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Splits a comma-separated list of types respecting nested braces and brackets.
fn split_top_level_comma(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: usize = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '[' | '<' => depth += 1,
            '}' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
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
            Self::Vector(len, elem) => write!(f, "<{len} x {elem}>"),
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
        assert_eq!(LlvmType::parse_canonical("i128"), Some(LlvmType::I128));
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

        let vec = LlvmType::Vector(4, Box::new(LlvmType::Float));
        assert_eq!(vec.to_string(), "<4 x float>");
    }

    #[test]
    fn parse_canonical_roundtrip() {
        let cases = [
            "void",
            "i1",
            "i8",
            "i16",
            "i32",
            "i64",
            "float",
            "double",
            "ptr",
            "[5 x i32]",
            "{ i1, double }",
            "{ i1, ptr, i64 }",
        ];
        for case in cases {
            let parsed = LlvmType::parse_canonical(case).expect("should parse canonical type");
            assert_eq!(parsed.to_string(), case);
        }
    }

    #[test]
    fn alignment_checks() {
        assert_eq!(LlvmType::I1.default_alignment(), 1);
        assert_eq!(LlvmType::I32.default_alignment(), 4);
        assert_eq!(LlvmType::I64.default_alignment(), 8);
        assert_eq!(LlvmType::Ptr.default_alignment(), 8);
        let s = LlvmType::Struct(vec![LlvmType::I1, LlvmType::I64]);
        assert_eq!(s.default_alignment(), 8);
    }
}
