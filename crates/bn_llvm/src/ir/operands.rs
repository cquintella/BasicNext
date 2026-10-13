// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed representations of LLVM operands and values.

use std::fmt;

use super::types::LlvmType;

/// LLVM string escaping, shared by `c"..."` constants and metadata strings:
/// `"`, `\` and bytes outside printable ASCII become `\XX`.
#[must_use]
pub fn escape_llvm(value: &str) -> String {
    value.bytes().map(escape_byte).collect()
}

/// One byte of a C string constant: printable ASCII but `"` and `\` as is,
/// anything else as `\XX`.
fn escape_byte(byte: u8) -> String {
    match byte {
        b' '..=b'!' | b'#'..=b'[' | b']'..=b'~' => char::from(byte).to_string(),
        _ => format!("\\{byte:02X}"),
    }
}

/// Representation of an LLVM operand or constant.
#[derive(Clone, Debug, PartialEq)]
pub enum LlvmOperand {
    ConstInt(i64),
    ConstUint(u64),
    /// A 128-bit integer constant (the widening checks of `i128` arithmetic).
    ConstI128(i128),
    /// A constant array: `[i32 2, i32 3]`.
    ConstArray(Vec<(LlvmType, LlvmOperand)>),
    ConstFloat(f64),
    ConstBool(bool),
    /// The bytes of a C string constant: `c"NA\00"` (escaped by [`escape_llvm`]).
    ConstBytes(Vec<u8>),
    /// A constant structure: `{ ptr @g, i32 4 }`.
    ConstStruct(Vec<(LlvmType, LlvmOperand)>),
    /// A constant address: `getelementptr inbounds (T, ptr @g, i32 0, i64 3)`.
    ConstGep {
        element: LlvmType,
        base: String,
        indices: Vec<(LlvmType, LlvmOperand)>,
    },
    Reg(String),
    Global(String),
    Null,
    Undef,
    ZeroInitializer,
}

impl LlvmOperand {
    /// Creates an SSA register operand: `%<name>`.
    #[must_use]
    pub fn reg(name: impl Into<String>) -> Self {
        Self::Reg(name.into())
    }

    /// Creates a global identifier operand: `@<name>`.
    #[must_use]
    pub fn global(name: impl Into<String>) -> Self {
        Self::Global(name.into())
    }

    /// Creates a signed integer constant.
    #[must_use]
    pub fn int(val: i64) -> Self {
        Self::ConstInt(val)
    }

    /// Creates an unsigned integer constant.
    #[must_use]
    pub fn uint(val: u64) -> Self {
        Self::ConstUint(val)
    }

    /// Creates a 128-bit integer constant.
    #[must_use]
    pub const fn int128(val: i128) -> Self {
        Self::ConstI128(val)
    }

    /// Creates a constant array of typed elements.
    #[must_use]
    pub const fn array(elements: Vec<(LlvmType, LlvmOperand)>) -> Self {
        Self::ConstArray(elements)
    }

    /// Creates a floating point constant.
    #[must_use]
    pub fn float(val: f64) -> Self {
        Self::ConstFloat(val)
    }

    /// Creates a boolean constant (`true` or `false`).
    #[must_use]
    pub fn bool(val: bool) -> Self {
        Self::ConstBool(val)
    }

    /// Creates a null pointer constant (`null`).
    #[must_use]
    pub fn null() -> Self {
        Self::Null
    }

    /// Creates an undefined value constant (`undef`).
    #[must_use]
    pub fn undef() -> Self {
        Self::Undef
    }

    /// Creates a zeroinitializer constant (`zeroinitializer`).
    #[must_use]
    pub fn zero_initializer() -> Self {
        Self::ZeroInitializer
    }
}

impl fmt::Display for LlvmOperand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConstInt(i) => write!(f, "{i}"),
            Self::ConstUint(u) => write!(f, "{u}"),
            Self::ConstI128(value) => write!(f, "{value}"),
            Self::ConstArray(elements) => {
                let items: Vec<String> = (elements.iter())
                    .map(|(ty, value)| format!("{ty} {value}"))
                    .collect();
                write!(f, "[{}]", items.join(", "))
            }
            Self::ConstFloat(v) => {
                if v.is_nan() {
                    write!(f, "0x7FF8000000000000")
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        write!(f, "0x7FF0000000000000")
                    } else {
                        write!(f, "0xFFF0000000000000")
                    }
                } else {
                    let s = v.to_string();
                    if s.contains('.') || s.contains('e') || s.contains('E') {
                        write!(f, "{s}")
                    } else {
                        write!(f, "{s}.0")
                    }
                }
            }
            Self::ConstBool(b) => write!(f, "{b}"),
            Self::ConstBytes(bytes) => {
                let text: String = bytes.iter().map(|&byte| escape_byte(byte)).collect();
                write!(f, "c\"{text}\"")
            }
            Self::ConstStruct(fields) => {
                let items: Vec<String> = (fields.iter())
                    .map(|(ty, value)| format!("{ty} {value}"))
                    .collect();
                write!(f, "{{ {} }}", items.join(", "))
            }
            Self::ConstGep {
                element,
                base,
                indices,
            } => {
                let items: Vec<String> = (indices.iter())
                    .map(|(ty, value)| format!(", {ty} {value}"))
                    .collect();
                write!(
                    f,
                    "getelementptr inbounds ({element}, ptr @{base}{})",
                    items.concat()
                )
            }
            Self::Reg(r) => {
                if r.starts_with('%') {
                    write!(f, "{r}")
                } else {
                    write!(f, "%{r}")
                }
            }
            Self::Global(g) => {
                if g.starts_with('@') {
                    write!(f, "{g}")
                } else {
                    write!(f, "@{g}")
                }
            }
            Self::Null => write!(f, "null"),
            Self::Undef => write!(f, "undef"),
            Self::ZeroInitializer => write!(f, "zeroinitializer"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_rendering() {
        assert_eq!(LlvmOperand::reg("v1").to_string(), "%v1");
        assert_eq!(LlvmOperand::reg("%v2").to_string(), "%v2");
    }

    #[test]
    fn global_rendering() {
        assert_eq!(LlvmOperand::global("printf").to_string(), "@printf");
        assert_eq!(LlvmOperand::global("@puts").to_string(), "@puts");
    }

    #[test]
    fn constants_rendering() {
        assert_eq!(LlvmOperand::int(42).to_string(), "42");
        assert_eq!(LlvmOperand::int(-100).to_string(), "-100");
        assert_eq!(
            LlvmOperand::uint(18_446_744_073_709_551_615).to_string(),
            "18446744073709551615"
        );
        assert_eq!(LlvmOperand::bool(true).to_string(), "true");
        assert_eq!(LlvmOperand::bool(false).to_string(), "false");
        assert_eq!(LlvmOperand::null().to_string(), "null");
        assert_eq!(LlvmOperand::undef().to_string(), "undef");
        assert_eq!(
            LlvmOperand::zero_initializer().to_string(),
            "zeroinitializer"
        );
    }

    #[test]
    fn float_rendering() {
        assert_eq!(LlvmOperand::float(0.0).to_string(), "0.0");
        assert_eq!(LlvmOperand::float(1.25).to_string(), "1.25");
        assert_eq!(LlvmOperand::float(10.0).to_string(), "10.0");
    }
}
