// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What a call names, resolved once per module. A call's callee is a
//! `Constant::Function` spelling (`Area`, `Shape.Area`, `#3.MEAN`,
//! `@super:Base.Init`, `HOST.Clock.Now`); [`CalleeTable`] turns every
//! spelling the module uses into a [`Callee`] and every (class, method) pair
//! into the function a receiver of that class runs, so a backend never parses
//! a callee name or scans the function list to make a call. Names stay for
//! diagnostics.

use std::collections::HashMap;

use bn_source::Span;

use crate::{Constant, Diagnostic, Instruction, Module, ModuleId, invalid_ir};

/// The prefix of a module-qualified callee: `#<module id>.<member>`.
const MODULE_PREFIX: char = '#';
/// The prefix of a call that skips dynamic dispatch: `@super:<function>`.
const SUPER_PREFIX: &str = "@super:";

/// The index of a function in `Module::functions`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct FunctionId(pub u32);

impl FunctionId {
    #[must_use]
    pub fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

/// The target of a direct call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Callee {
    /// A function of the module. `method` is the member name a receiver's
    /// runtime class re-resolves (`Area` of `Shape.Area`); `None` for a free
    /// function and for the lifecycle functions, which never re-dispatch.
    Function {
        id: FunctionId,
        method: Option<String>,
    },
    /// `@super:<function>`: that function, with no dynamic dispatch.
    Super(FunctionId),
    /// A member a standard library's provider serves.
    Library { module: ModuleId, member: String },
    /// A member of a HOST capability.
    Host { capability: String, member: String },
    /// A method of a class or interface the module declares no function for
    /// (`IShape.Area`): only the receiver's runtime class names the target.
    Virtual { method: String },
    /// A language intrinsic or builtin the backend knows by name.
    Builtin,
}

/// Every callee spelling of a module, resolved, and the method table of its
/// classes.
#[derive(Debug, Default)]
pub struct CalleeTable {
    by_name: HashMap<String, Callee>,
    /// Each class's own methods: owner → method → function.
    methods: HashMap<String, HashMap<String, FunctionId>>,
}

impl CalleeTable {
    /// Resolves every `Constant::Function` the module's code holds and builds
    /// the method table of every class with a layout.
    #[must_use]
    pub fn new(module: &Module) -> Self {
        let functions = function_ids(module);
        let mut table = Self::default();
        for function in &module.functions {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    if let Instruction::Constant {
                        value: Constant::Function(name),
                        ..
                    } = instruction
                        && !table.by_name.contains_key(name)
                    {
                        let callee = resolve(module, &functions, name);
                        table.by_name.insert(name.clone(), callee);
                    }
                }
            }
        }
        for (name, id) in &functions {
            if !table.by_name.contains_key(name) {
                let callee = resolve(module, &functions, name);
                table.by_name.insert(name.clone(), callee);
            }
            if let Some((owner, method)) = name.rsplit_once('.')
                && !owner.starts_with(MODULE_PREFIX)
            {
                table
                    .methods
                    .entry(owner.to_owned())
                    .or_default()
                    .entry(method.to_owned())
                    .or_insert(*id);
            }
        }
        table
    }

    /// The target `name` resolved to, if the module's code names it.
    #[must_use]
    pub fn callee(&self, name: &str) -> Option<&Callee> {
        self.by_name.get(name)
    }

    /// The function a receiver of `class` runs for `method`: the nearest
    /// ancestor's, in `module`'s class model.
    #[must_use]
    pub fn method(&self, module: &Module, class: &str, method: &str) -> Option<FunctionId> {
        module
            .class_model
            .ancestors(class)
            .into_iter()
            .find_map(|ancestor| self.methods.get(ancestor)?.get(method).copied())
    }
}

fn function_ids(module: &Module) -> HashMap<String, FunctionId> {
    module
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, function)| {
            u32::try_from(index)
                .ok()
                .map(|index| (function.name.clone(), FunctionId(index)))
        })
        .collect()
}

/// One spelling, classified as the backends always have: an exact function
/// of the module first, then the prefixed forms, then a dotted method.
fn resolve(module: &Module, functions: &HashMap<String, FunctionId>, name: &str) -> Callee {
    if let Some(target) = name.strip_prefix(SUPER_PREFIX) {
        return functions
            .get(target)
            .map_or(Callee::Builtin, |id| Callee::Super(*id));
    }
    if let Some(id) = functions.get(name) {
        let method = module.functions.get(id.index()).and_then(|function| {
            let redispatches = matches!(function.kind, crate::FunctionKind::User);
            (redispatches && function.owner.is_some())
                .then(|| name.rsplit('.').next().map(str::to_owned))
                .flatten()
        });
        return Callee::Function { id: *id, method };
    }
    if let Some(rest) = name.strip_prefix(MODULE_PREFIX)
        && let Some((module_id, member)) = rest.split_once('.')
        && let Ok(module_id) = module_id.parse::<u32>()
        && module.standard_library_of(ModuleId(module_id)).is_some()
    {
        return Callee::Library {
            module: ModuleId(module_id),
            member: member.to_owned(),
        };
    }
    if let Some(rest) = name.strip_prefix(bn_catalog::text("modules", "callee.host_prefix"))
        && let Some((capability, member)) = rest.split_once('.')
    {
        return Callee::Host {
            capability: capability.to_owned(),
            member: member.to_owned(),
        };
    }
    // A dotted name with no function: an interface or inherited method, of
    // this module or an imported one (`#2.IShape.Area`).
    if let Some((_, method)) = name.rsplit_once('.') {
        return Callee::Virtual {
            method: method.to_owned(),
        };
    }
    Callee::Builtin
}

/// A module-qualified (`#<id>.`) or `@super:` callee that names nothing is
/// invalid IR: no backend could make the call.
///
/// # Errors
///
/// `INVALID_IR` naming the first unresolved callee.
pub(crate) fn validate_callees(module: &Module) -> Result<(), Diagnostic> {
    let table = CalleeTable::new(module);
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Instruction::Constant {
                    value: Constant::Function(name),
                    span,
                    ..
                } = instruction
                else {
                    continue;
                };
                let malformed_module = name.strip_prefix(MODULE_PREFIX).is_some_and(|rest| {
                    rest.split_once('.')
                        .is_none_or(|(id, _)| id.parse::<u32>().is_err())
                });
                let lost_super = name.starts_with(SUPER_PREFIX)
                    && !matches!(table.callee(name), Some(Callee::Super(_)));
                if malformed_module || lost_super {
                    return Err(unresolved(name, *span));
                }
            }
        }
    }
    Ok(())
}

fn unresolved(name: &str, span: Span) -> Diagnostic {
    invalid_ir(
        bn_catalog::fill("modules", "ir.callee_unresolved", &[("name", &name)]),
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::{Callee, CalleeTable, FunctionId, validate_callees};
    use crate::{
        BasicBlock, BlockId, Constant, Function, FunctionKind, Instruction, Module, ModuleId,
        Terminator, ValueId,
    };
    use bn_source::{Position, Revision, SourceId, Span};
    use bn_types::Type;
    use std::collections::{HashMap, HashSet};

    fn span() -> Span {
        let position = Position {
            source_id: SourceId(1),
            revision: Revision(1),
            offset: 0,
            line: 1,
            column: 1,
        };
        Span {
            start: position,
            end: position,
        }
    }

    /// A function that holds one `Constant::Function` per name in `calls`.
    fn function(name: &str, owner: Option<&str>, calls: &[&str]) -> Function {
        let instructions = calls
            .iter()
            .zip(0_u32..)
            .map(|(callee, index)| Instruction::Constant {
                destination: ValueId(index),
                value: Constant::Function((*callee).to_owned()),
                ty: Type::Unknown,
                span: span(),
            })
            .collect();
        Function {
            name: name.to_owned(),
            kind: FunctionKind::User,
            owner: owner.map(str::to_owned),
            asynchronous: false,
            parameters: Vec::new(),
            weak_symbols: HashSet::new(),
            return_type: Type::Unknown,
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return { value: None },
            }],
            symbol_names: HashMap::new(),
            span: span(),
        }
    }

    fn module_with_shapes(calls: &[&str]) -> Module {
        let mut module = Module::default();
        module.class_model.add_base("Circle", "Shape");
        for class in ["Shape", "Circle"] {
            module.field_layouts.insert(
                class.to_owned(),
                crate::FieldLayout {
                    owner: class.to_owned(),
                    fields: Vec::new(),
                    span: span(),
                },
            );
        }
        module.bnmath_providers.insert(ModuleId(3));
        module.functions = vec![
            function("Start", None, calls),
            function("Shape.Area", Some("Shape"), &[]),
            function("Shape.Name", Some("Shape"), &[]),
            function("Circle.Area", Some("Circle"), &[]),
        ];
        module
    }

    #[test]
    fn callee_ids_classify_every_spelling_once() {
        let module = module_with_shapes(&[
            "Shape.Area",
            "@super:Shape.Area",
            "#3.MEAN",
            "HOST.Clock.Now",
            "IShape.Perimeter",
            "ASC",
        ]);
        let table = CalleeTable::new(&module);
        assert_eq!(
            table.callee("Shape.Area"),
            Some(&Callee::Function {
                id: FunctionId(1),
                method: Some("Area".into())
            })
        );
        assert_eq!(
            table.callee("@super:Shape.Area"),
            Some(&Callee::Super(FunctionId(1)))
        );
        assert_eq!(
            table.callee("#3.MEAN"),
            Some(&Callee::Library {
                module: ModuleId(3),
                member: "MEAN".into()
            })
        );
        assert_eq!(
            table.callee("HOST.Clock.Now"),
            Some(&Callee::Host {
                capability: "Clock".into(),
                member: "Now".into()
            })
        );
        assert_eq!(
            table.callee("IShape.Perimeter"),
            Some(&Callee::Virtual {
                method: "Perimeter".into()
            })
        );
        assert_eq!(table.callee("ASC"), Some(&Callee::Builtin));
        assert_eq!(
            table.callee("Start"),
            Some(&Callee::Function {
                id: FunctionId(0),
                method: None
            }),
            "every function is callable by name, the internal calls included"
        );
    }

    #[test]
    fn callee_ids_resolve_methods_by_the_nearest_ancestor() {
        let module = module_with_shapes(&[]);
        let table = CalleeTable::new(&module);
        assert_eq!(table.method(&module, "Circle", "Area"), Some(FunctionId(3)));
        assert_eq!(table.method(&module, "Circle", "Name"), Some(FunctionId(2)));
        assert_eq!(table.method(&module, "Shape", "Area"), Some(FunctionId(1)));
        assert_eq!(table.method(&module, "Shape", "Radius"), None);
    }

    #[test]
    fn callee_ids_reject_an_unresolved_qualified_callee() {
        assert!(validate_callees(&module_with_shapes(&["#3.MEAN", "@super:Shape.Area"])).is_ok());
        assert!(validate_callees(&module_with_shapes(&["#x.MEAN"])).is_err());
        assert!(validate_callees(&module_with_shapes(&["#2.IShape.Area"])).is_ok());
        assert!(validate_callees(&module_with_shapes(&["@super:Shape.Missing"])).is_err());
    }
}
