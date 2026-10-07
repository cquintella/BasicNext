// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed representations of LLVM operands and values.

use std::fmt;

/// LLVM string escaping, shared by `c"..."` constants and metadata strings:
/// `"`, `\` and bytes outside printable ASCII become `\XX`.
#[must_use]
pub fn escape_llvm(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b' '..=b'!' | b'#'..=b'[' | b']'..=b'~' => char::from(byte).to_string(),
            _ => format!("\\{byte:02X}"),
        })
        .collect()
}

/// Representation of an LLVM operand or constant.
#[derive(Clone, Debug, PartialEq)]
pub enum LlvmOperand {
    ConstInt(i64),
    ConstUint(u64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstString(String),
    Reg(String),
    Global(String),
    Null,
    Undef,
    ZeroInitializer,
    /// Operand text produced by a backend helper that is still textual
    /// (`%v3`, `42`, `@.str`), rendered verbatim. It bridges migrated and
    /// unmigrated code without guessing what the text holds.
    Raw(String),
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

    /// Wraps operand text from a still-textual helper; see [`Self::Raw`].
    #[must_use]
    pub fn raw(text: impl Into<String>) -> Self {
        Self::Raw(text.into())
    }
}

impl fmt::Display for LlvmOperand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConstInt(i) => write!(f, "{i}"),
            Self::ConstUint(u) => write!(f, "{u}"),
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
            Self::ConstString(s) => write!(f, "{s}"),
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
            Self::Raw(text) => write!(f, "{text}"),
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
