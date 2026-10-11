// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Canonical type names in an imported module's IR: the module's own classes,
// structs and interfaces are written `#<module>.Name`, the spelling every IR
// registry (`$fields`, field layouts, class globals) and the entry module
// already use, so the IR validator and the backends resolve them without
// knowing which module a function came from (known issues K52, K57).

use std::collections::{HashMap, HashSet};

use super::{
    Constant, DeclarationKind, Function, Instruction, Item, PendingLayout, Program, Type,
    qualified_class_name,
};

/// The type names `program` declares: its classes, structs and interfaces.
pub(super) fn declared_type_names(program: &Program) -> HashSet<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration {
                kind: DeclarationKind::Class | DeclarationKind::Struct | DeclarationKind::Interface,
                name,
                ..
            } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// Rewrites, in the functions lowered for the module with `prefix`, every
/// reference to a type in `declared` to its canonical name.
pub(super) fn qualify_functions(
    functions: &mut [Function],
    prefix: &str,
    declared: &HashSet<String>,
) {
    for function in functions {
        qualify_type(&mut function.return_type, prefix, declared);
        for instruction in function
            .blocks
            .iter_mut()
            .flat_map(|block| block.instructions.iter_mut())
        {
            if let Instruction::Constant {
                value: Constant::Type(name),
                ..
            } = instruction
            {
                qualify_test_name(name, prefix, declared);
            }
            if let Some(ty) = instruction.ty_mut() {
                qualify_type(ty, prefix, declared);
            }
        }
    }
}

/// Rewrites the field types of the layouts the module with `prefix` owns.
pub(super) fn qualify_layouts(
    layouts: &mut HashMap<String, PendingLayout>,
    prefix: &str,
    declared: &HashSet<String>,
) {
    for layout in layouts
        .values_mut()
        .filter(|layout| layout.owner.starts_with(prefix))
    {
        for field in &mut layout.fields {
            qualify_type(&mut field.ty, prefix, declared);
        }
    }
}

fn qualify_type(ty: &mut Type, prefix: &str, declared: &HashSet<String>) {
    match ty {
        Type::Named(name) | Type::TypeName(name) if declared.contains(name.as_str()) => {
            *name = qualified_class_name(prefix, name);
        }
        Type::Alternative(members) => {
            for member in members {
                qualify_type(member, prefix, declared);
            }
        }
        Type::Vector { element, .. } | Type::Pointer { element, .. } => {
            qualify_type(element, prefix, declared);
        }
        Type::Function {
            parameters,
            return_type,
        } => {
            for parameter in parameters {
                qualify_type(parameter, prefix, declared);
            }
            qualify_type(return_type, prefix, declared);
        }
        _ => {}
    }
}

/// An `IS` test operand: a type name, or a fixed vector of one (`Name[2]`).
fn qualify_test_name(name: &mut String, prefix: &str, declared: &HashSet<String>) {
    let base_end = name.find('[').unwrap_or(name.len());
    if declared.contains(&name[..base_end]) {
        *name = qualified_class_name(prefix, name);
    }
}
