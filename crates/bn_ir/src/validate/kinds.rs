// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_types::Type;

use crate::{Diagnostic, Module, invalid_ir};

/// Structural rules for `FunctionKind` (bucket 0.5.1c §3.2). Backends select
/// entry points, constructors and destructors by kind, so a mislabelled
/// function is invalid IR, not a backend surprise.
pub(super) fn validate_function_kinds(module: &Module) -> Result<(), Diagnostic> {
    use crate::FunctionKind::{
        Constructor, Default, Destructor, Entry, FieldInit, Init, ReleaseFields, User,
    };
    let mut entries = 0usize;
    for function in &module.functions {
        match function.kind {
            Entry => {
                entries += 1;
                if entries > 1 {
                    return Err(invalid_ir(
                        "module declares more than one entry function",
                        function.span,
                    ));
                }
                if !function.parameters.is_empty() {
                    return Err(invalid_ir(
                        "entry function must not take parameters",
                        function.span,
                    ));
                }
            }
            Constructor | Destructor | FieldInit | Init | ReleaseFields => {
                if function.owner.is_none() {
                    return Err(invalid_ir(
                        "constructor, destructor, field-init, field-release and init functions need an owner class",
                        function.span,
                    ));
                }
                // `Init` allocates and returns the object, so it has no SELF.
                if function.kind != Init && function.parameters.is_empty() {
                    return Err(invalid_ir(
                        "constructor, destructor, field-init and field-release functions take SELF first",
                        function.span,
                    ));
                }
                if matches!(function.kind, Destructor | ReleaseFields)
                    && !matches!(&function.return_type, Type::Named(name) if name == "VOID")
                {
                    return Err(invalid_ir(
                        if function.kind == Destructor {
                            "destructor must return VOID"
                        } else {
                            "field release must return VOID"
                        },
                        function.span,
                    ));
                }
            }
            Default => {
                if function.owner.is_none() {
                    return Err(invalid_ir(
                        "default constructor needs an owner struct",
                        function.span,
                    ));
                }
                if !function.parameters.is_empty() {
                    return Err(invalid_ir(
                        "default constructor must not take parameters",
                        function.span,
                    ));
                }
            }
            User => {}
        }
    }
    Ok(())
}
