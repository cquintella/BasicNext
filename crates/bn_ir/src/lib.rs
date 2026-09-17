// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If applicable, see the MPL-2.0 license.

//! Language-owned BN IR model, validation, and backend handoff proof.

use bn_diag::{DiagId, Diagnostic, Label, LabelStyle};
use bn_source::Span;

mod model;
pub mod names;
mod validate;

pub use model::{
    BasicBlock, BlockId, Constant, Function, Instruction, Module, ModuleId, SymbolId, Terminator,
    ValueId,
};
pub use validate::{instruction_uses, validate};

pub use bn_types::{FloatType, IntegerType, PointerLength, Type};

/// IR that has passed the language-level validation contract.
#[derive(Debug)]
pub struct ValidatedModule {
    module: Module,
}

impl ValidatedModule {
    #[must_use]
    pub const fn as_module(&self) -> &Module {
        &self.module
    }

    #[must_use]
    pub fn into_module(self) -> Module {
        self.module
    }
}

/// Constructs the stable language-level diagnostic for malformed IR.
pub(crate) fn invalid_ir(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::structured(
        DiagId::INVALID_IR,
        vec![("detail".into(), message.into().into())],
        vec![Label {
            span,
            style: LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("invalid IR diagnostic schema is registered")
}

/// Proves that a module satisfies the language-level IR contract.
///
/// # Errors
///
/// Returns `INVALID_IR` when the module is not well formed.
pub fn validate_module(module: Module) -> Result<ValidatedModule, Diagnostic> {
    validate(&module)?;
    Ok(ValidatedModule { module })
}
