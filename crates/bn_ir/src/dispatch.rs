// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If applicable, see the MPL-2.0 license.

//! Shared dynamic method resolution across compiler, interpreter, and backends.
//!
//! Evaluates virtual dispatch against the inheritance hierarchy in [`ClassModel`].

use crate::Module;

/// Resolves the symbol name for `method` given the runtime class of the receiver.
///
/// Walks the ancestors of `runtime_class` from most-derived to least-derived.
/// Returns the first function symbol `{Ancestor}.{method}` defined in `module.functions`.
#[must_use]
pub fn resolve_method<'a>(
    module: &'a Module,
    runtime_class: &str,
    method: &str,
) -> Option<&'a str> {
    for ancestor in module.class_model.ancestors(runtime_class) {
        let symbol = format!("{ancestor}.{method}");
        if let Some(function) = module.functions.iter().find(|f| f.name == symbol) {
            return Some(&function.name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::resolve_method;
    use crate::{BasicBlock, BlockId, Function, FunctionKind, Module, Terminator};
    use bn_source::{Position, Revision, SourceId, Span};
    use bn_types::Type;
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    fn test_span() -> Span {
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

    fn make_test_module() -> Module {
        Module {
            source_name: None,
            functions: Vec::new(),
            field_names: Vec::new(),
            field_layouts: BTreeMap::new(),
            class_model: bn_types::class_model::ClassModel::default(),
            interfaces: BTreeSet::new(),
            bndata_providers: HashSet::new(),
            bnmath_providers: HashSet::new(),
            bnlog_providers: HashSet::new(),
            bnjson_providers: HashSet::new(),
            bnweb_providers: HashSet::new(),
            bndispatch_providers: HashSet::new(),
            bncrypto_providers: HashSet::new(),
            bnsqlite_providers: HashSet::new(),
            filesystem_import: None,
            clock_import: None,
            random_import: None,
            console_import: None,
            network_import: None,
            exec_import: None,
            env_import: None,
            bnlog_import: None,
            bnweb_import: None,
            bnsqlite_import: None,
        }
    }

    fn add_fn(module: &mut Module, name: &str) {
        module.functions.push(Function {
            name: name.to_string(),
            kind: FunctionKind::User,
            owner: None,
            asynchronous: false,
            parameters: Vec::new(),
            weak_symbols: HashSet::new(),
            return_type: Type::Unknown,
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: None },
            }],
            span: test_span(),
        });
    }

    #[test]
    fn intermediate_override_resolves_to_mid() {
        let mut module = make_test_module();
        module.class_model.add_base("Mid", "Base");
        module.class_model.add_base("Sub", "Mid");

        add_fn(&mut module, "Base.Name");
        add_fn(&mut module, "Mid.Name");

        // Sub inherits Name from Mid (override)
        assert_eq!(resolve_method(&module, "Sub", "Name"), Some("Mid.Name"));
        assert_eq!(resolve_method(&module, "Mid", "Name"), Some("Mid.Name"));
        assert_eq!(resolve_method(&module, "Base", "Name"), Some("Base.Name"));
    }

    #[test]
    fn qualified_names_do_not_collide() {
        let mut module = make_test_module();
        module.class_model.add_base("Dog", "Animal");
        module.class_model.add_base("#0.Dog", "#0.Animal");

        add_fn(&mut module, "Animal.Speak");
        add_fn(&mut module, "Dog.Speak");
        add_fn(&mut module, "#0.Animal.Speak");
        add_fn(&mut module, "#0.Dog.Speak");

        assert_eq!(resolve_method(&module, "Dog", "Speak"), Some("Dog.Speak"));
        assert_eq!(
            resolve_method(&module, "#0.Dog", "Speak"),
            Some("#0.Dog.Speak")
        );
    }
}
