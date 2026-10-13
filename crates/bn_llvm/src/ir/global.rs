// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Global variables and constants of an LLVM module: linkage, attributes,
//! type and initializer (bucket 0.6.5 T.5.2).

use std::fmt;

use super::{operands::LlvmOperand, types::LlvmType};

/// How a global is linked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Linkage {
    /// Visible to other modules (no keyword).
    Default,
    /// `private`: this module only, no symbol.
    Private,
    /// `internal`: this module only, with a local symbol.
    Internal,
    /// `external`: defined elsewhere; the global has no initializer.
    External,
}

/// A global: `@name = [linkage] [thread_local] [unnamed_addr] global|constant
/// <type> [initializer]`.
#[derive(Clone, Debug, PartialEq)]
pub struct LlvmGlobal {
    pub name: String,
    pub linkage: Linkage,
    pub thread_local: bool,
    pub unnamed_addr: bool,
    pub constant: bool,
    pub ty: LlvmType,
    pub init: Option<LlvmOperand>,
}

impl LlvmGlobal {
    /// A mutable global with an initializer and default linkage.
    #[must_use]
    pub fn variable(name: impl Into<String>, ty: LlvmType, init: LlvmOperand) -> Self {
        Self {
            name: name.into(),
            linkage: Linkage::Default,
            thread_local: false,
            unnamed_addr: false,
            constant: false,
            ty,
            init: Some(init),
        }
    }

    /// A private C string constant: `private [unnamed_addr] constant
    /// [N x i8] c"..."`, the bytes ending with their NUL.
    #[must_use]
    pub fn c_string(name: impl Into<String>, bytes: Vec<u8>, unnamed_addr: bool) -> Self {
        let ty = LlvmType::Array(bytes.len(), Box::new(LlvmType::I8));
        Self {
            name: name.into(),
            linkage: Linkage::Private,
            thread_local: false,
            unnamed_addr,
            constant: true,
            ty,
            init: Some(LlvmOperand::ConstBytes(bytes)),
        }
    }

    /// A private constant holding `text` and its terminating NUL.
    #[must_use]
    pub fn text(name: impl Into<String>, text: &str, unnamed_addr: bool) -> Self {
        let bytes = text.bytes().chain(std::iter::once(0)).collect();
        Self::c_string(name, bytes, unnamed_addr)
    }

    /// A global defined in another module.
    #[must_use]
    pub fn external(name: impl Into<String>, ty: LlvmType) -> Self {
        Self {
            name: name.into(),
            linkage: Linkage::External,
            thread_local: false,
            unnamed_addr: false,
            constant: false,
            ty,
            init: None,
        }
    }

    #[must_use]
    pub const fn with_linkage(mut self, linkage: Linkage) -> Self {
        self.linkage = linkage;
        self
    }

    #[must_use]
    pub const fn thread_local(mut self) -> Self {
        self.thread_local = true;
        self
    }
}

impl fmt::Display for LlvmGlobal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.name.strip_prefix('@').unwrap_or(&self.name);
        write!(f, "@{name} = ")?;
        match self.linkage {
            Linkage::Default => {}
            Linkage::Private => write!(f, "private ")?,
            Linkage::Internal => write!(f, "internal ")?,
            Linkage::External => write!(f, "external ")?,
        }
        if self.thread_local {
            write!(f, "thread_local ")?;
        }
        if self.unnamed_addr {
            write!(f, "unnamed_addr ")?;
        }
        let kind = if self.constant { "constant" } else { "global" };
        write!(f, "{kind} {}", self.ty)?;
        if let Some(init) = &self.init {
            write!(f, " {init}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Linkage, LlvmGlobal};
    use crate::ir::{LlvmOperand, LlvmType};

    #[test]
    fn globals_render_as_the_backend_wrote_them() {
        let na = LlvmGlobal::c_string(".bn_na", b"NA\0".to_vec(), true);
        assert_eq!(
            na.to_string(),
            "@.bn_na = private unnamed_addr constant [3 x i8] c\"NA\\00\""
        );
        let flag = LlvmGlobal::variable("bn_init_Box", LlvmType::I1, LlvmOperand::bool(false));
        assert_eq!(flag.to_string(), "@bn_init_Box = global i1 false");
        let out = LlvmGlobal::external("__stdoutp", LlvmType::Ptr);
        assert_eq!(out.to_string(), "@__stdoutp = external global ptr");
        let buffer =
            LlvmGlobal::variable("bn_vret_f", LlvmType::I64, LlvmOperand::zero_initializer())
                .with_linkage(Linkage::Internal)
                .thread_local();
        assert_eq!(
            buffer.to_string(),
            "@bn_vret_f = internal thread_local global i64 zeroinitializer"
        );
    }
}
