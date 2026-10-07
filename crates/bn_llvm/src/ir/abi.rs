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
pub enum AbiType {
    Void,
    I1,
    I8,
    I32,
    I64,
    Double,
    Ptr,
}

impl From<AbiType> for LlvmType {
    fn from(ty: AbiType) -> Self {
        match ty {
            AbiType::Void => Self::Void,
            AbiType::I1 => Self::I1,
            AbiType::I8 => Self::I8,
            AbiType::I32 => Self::I32,
            AbiType::I64 => Self::I64,
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
pub struct RuntimeFn<const N: usize> {
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

/// The `declare` line of a signature, independent of its arity, so
/// signatures of different `N` can be listed together.
pub trait Declaration {
    fn declaration(&self) -> String;
}

impl<const N: usize> Declaration for RuntimeFn<N> {
    fn declaration(&self) -> String {
        let params = self
            .params
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        format!("declare {} @{}({params})", self.ret, self.name)
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
