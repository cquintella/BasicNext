// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed C-ABI signatures: a `RuntimeFn<N>` names a C function with `N`
//! parameters, so a call with the wrong argument count does not compile and
//! every argument takes its LLVM type from the signature.

use std::fmt;

use super::{instructions::LlvmInst, operands::LlvmOperand, types::LlvmType};

/// The scalar types that cross the C boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbiType {
    Void,
    I1,
    I8,
    I16,
    I32,
    I64,
    Float,
    Double,
    Ptr,
}

impl From<AbiType> for LlvmType {
    fn from(ty: AbiType) -> Self {
        match ty {
            AbiType::Void => Self::Void,
            AbiType::I1 => Self::I1,
            AbiType::I8 => Self::I8,
            AbiType::I16 => Self::I16,
            AbiType::I32 => Self::I32,
            AbiType::I64 => Self::I64,
            AbiType::Float => Self::Float,
            AbiType::Double => Self::Double,
            AbiType::Ptr => Self::Ptr,
        }
    }
}

impl fmt::Display for AbiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", LlvmType::from(*self))
    }
}

/// A C function signature with `N` fixed parameters.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeFn<const N: usize> {
    pub name: &'static str,
    pub ret: AbiType,
    pub params: [AbiType; N],
}

impl<const N: usize> RuntimeFn<N> {
    /// A call whose arity the compiler checks and whose argument types come
    /// from the signature.
    #[must_use]
    pub fn call(&self, args: [LlvmOperand; N]) -> LlvmInst {
        LlvmInst::call(
            self.ret.into(),
            self.name,
            self.params
                .iter()
                .zip(args)
                .map(|(ty, arg)| (LlvmType::from(*ty), arg))
                .collect(),
        )
    }
}

/// A function the module declares: its name and typed signature, rendered
/// as `declare <ret> @<name>(<params>[, ...])`.
#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    pub name: &'static str,
    pub ret: LlvmType,
    pub params: Vec<LlvmType>,
    pub variadic: bool,
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut params: Vec<String> = self.params.iter().map(ToString::to_string).collect();
        if self.variadic {
            params.push("...".into());
        }
        write!(
            f,
            "declare {} @{}({})",
            self.ret,
            self.name,
            params.join(", ")
        )
    }
}

/// Anything the module can declare, independent of its arity, so
/// signatures of different `N` can be listed together.
pub trait Declaration {
    fn signature(&self) -> Signature;

    fn name(&self) -> &'static str {
        self.signature().name
    }

    /// The `declare` line.
    fn declaration(&self) -> String {
        self.signature().to_string()
    }
}

/// The LLVM types of C parameters.
fn llvm_params(params: &[AbiType]) -> Vec<LlvmType> {
    params.iter().map(|&ty| LlvmType::from(ty)).collect()
}

impl<const N: usize> Declaration for RuntimeFn<N> {
    fn signature(&self) -> Signature {
        let (name, ret, params) = (self.name, self.ret.into(), llvm_params(&self.params));
        Signature {
            name,
            ret,
            params,
            variadic: false,
        }
    }
}

/// A variadic C function with `N` fixed parameters (`printf`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct VariadicFn<const N: usize> {
    pub name: &'static str,
    pub ret: AbiType,
    pub params: [AbiType; N],
}

impl<const N: usize> Declaration for VariadicFn<N> {
    fn signature(&self) -> Signature {
        let (name, ret, params) = (self.name, self.ret.into(), llvm_params(&self.params));
        Signature {
            name,
            ret,
            params,
            variadic: true,
        }
    }
}

/// An LLVM intrinsic and its signature, whose types (aggregates, `i128`,
/// `float`) do not all cross the C boundary. Intrinsics order by their
/// `declare` line, so a module declares them in a stable order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Intrinsic {
    pub name: &'static str,
    pub ret: LlvmType,
    pub params: Vec<LlvmType>,
}

impl Declaration for Intrinsic {
    fn signature(&self) -> Signature {
        let (name, ret, params) = (self.name, self.ret.clone(), self.params.clone());
        Signature {
            name,
            ret,
            params,
            variadic: false,
        }
    }
}

impl Ord for Intrinsic {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.declaration().cmp(&other.declaration())
    }
}

impl PartialOrd for Intrinsic {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Declares every signature of `group` in `module`, in order.
pub(crate) fn write_declarations(module: &mut super::LlvmModule, group: &[&dyn Declaration]) {
    for signature in group {
        module.declare(signature.signature());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXEC_RUN: RuntimeFn<4> = RuntimeFn {
        name: "bn_rt_exec_run",
        ret: AbiType::I32,
        params: [AbiType::Ptr, AbiType::Ptr, AbiType::I32, AbiType::Ptr],
    };

    #[test]
    fn signature_types_every_argument_and_renders_its_declaration() {
        let call = EXEC_RUN.call([
            LlvmOperand::reg("v0"),
            LlvmOperand::reg("args"),
            LlvmOperand::reg("argc"),
            LlvmOperand::reg("out"),
        ]);
        assert_eq!(
            call.to_string(),
            "call i32 @bn_rt_exec_run(ptr %v0, ptr %args, i32 %argc, ptr %out)"
        );
        assert_eq!(
            EXEC_RUN.declaration(),
            "declare i32 @bn_rt_exec_run(ptr, ptr, i32, ptr)"
        );
        let none: RuntimeFn<0> = RuntimeFn {
            name: "bn_rt_none",
            ret: AbiType::Void,
            params: [],
        };
        assert_eq!(none.declaration(), "declare void @bn_rt_none()");
    }
}
