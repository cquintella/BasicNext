// Field layout and resolved-reference validation for language IR.
use std::collections::HashSet;

use bn_source::Span;
use bn_types::Type;

use super::{Diagnostic, Module, default_module_span, invalid_ir};

pub(super) fn validate_field_layouts(module: &Module) -> Result<(), Diagnostic> {
    let module_span = module
        .functions
        .first()
        .map_or_else(default_module_span, |function| function.span);
    let mut names = HashSet::new();
    for name in &module.field_names {
        if name.is_empty() {
            return Err(invalid_ir("field names cannot be empty", module_span));
        }
        if !names.insert(name) {
            return Err(invalid_ir("field names must be unique", module_span));
        }
    }

    for (owner, layout) in &module.field_layouts {
        if owner.is_empty() || layout.owner.is_empty() || layout.owner != *owner {
            return Err(invalid_ir(
                "field layout owner must be non-empty and match its module key",
                layout.span,
            ));
        }
        let mut ids = HashSet::new();
        for (index, field) in layout.fields.iter().enumerate() {
            let expected_slot = u32::try_from(index)
                .map_err(|_| invalid_ir("field layout has too many fields", layout.span))?;
            if field.slot.0 != expected_slot {
                return Err(invalid_ir(
                    "field layout slots must be dense and ordered",
                    field.span,
                ));
            }
            let field_index = usize::try_from(field.id.0)
                .map_err(|_| invalid_ir("field ID does not fit", field.span))?;
            if module.field_names.get(field_index).is_none() {
                return Err(invalid_ir(
                    "field ID is absent from the name table",
                    field.span,
                ));
            }
            if !ids.insert(field.id) {
                return Err(invalid_ir(
                    "field layout contains a duplicate field ID",
                    field.span,
                ));
            }
            if field.declaring_owner.is_empty()
                || !layout_owner_contains(module, owner, &field.declaring_owner)
            {
                return Err(invalid_ir(
                    "field declaring owner is not in the layout hierarchy",
                    field.span,
                ));
            }
        }
        if let Some(base_owner) = module.class_bases.get(owner)
            && let Some(base_layout) = module.field_layouts.get(base_owner)
            && (layout.fields.len() < base_layout.fields.len()
                || !layout
                    .fields
                    .iter()
                    .zip(&base_layout.fields)
                    .all(|(derived, base)| same_layout_field(derived, base)))
        {
            return Err(invalid_ir(
                "derived field layout must preserve the base layout prefix",
                layout.span,
            ));
        }
    }
    Ok(())
}

fn same_layout_field(derived: &crate::FieldLayoutEntry, base: &crate::FieldLayoutEntry) -> bool {
    derived.id == base.id
        && derived.slot == base.slot
        && derived.ty == base.ty
        && derived.declaring_owner == base.declaring_owner
        && derived.weak == base.weak
}

fn layout_owner_contains(module: &Module, owner: &str, declaring_owner: &str) -> bool {
    let mut current = owner;
    loop {
        if current == declaring_owner {
            return true;
        }
        let Some(base) = module.class_bases.get(current) else {
            return false;
        };
        current = base;
    }
}

pub(super) fn validate_resolved_field_path(
    module: &Module,
    root_owner: &str,
    path: &[String],
    fields: Option<&[crate::FieldRef]>,
    span: Span,
) -> Result<Type, Diagnostic> {
    if root_owner.is_empty() || path.is_empty() || path.iter().any(String::is_empty) {
        return Err(invalid_ir("field path cannot be empty", span));
    }
    let Some(fields) = fields else {
        return Err(invalid_ir(
            "field path store must carry resolved fields",
            span,
        ));
    };
    if fields.len() != path.len() {
        return Err(invalid_ir(
            "resolved field path length must match its spelling path",
            span,
        ));
    }
    let mut owner = root_owner.to_string();
    let mut terminal_type = None;
    for (index, (name, field)) in path.iter().zip(fields).enumerate() {
        let expected = module
            .field_ref(&owner, name)
            .ok_or_else(|| invalid_ir("field path does not resolve in its owner layout", span))?;
        if *field != expected {
            return Err(invalid_ir(
                "resolved field path does not match its owner layout",
                span,
            ));
        }
        let layout = module
            .field_layouts
            .get(&owner)
            .ok_or_else(|| invalid_ir("field path owner has no layout", span))?;
        let entry = layout
            .fields
            .get(
                usize::try_from(field.slot.value())
                    .map_err(|_| invalid_ir("resolved field slot does not fit", span))?,
            )
            .ok_or_else(|| invalid_ir("resolved field slot is absent", span))?;
        terminal_type = Some(entry.ty.clone());
        if index + 1 < path.len() {
            owner = record_owner_for_type(module, &owner, &entry.ty).ok_or_else(|| {
                invalid_ir("non-final field path component is not a record", span)
            })?;
        }
    }
    terminal_type.ok_or_else(|| invalid_ir("field path has no terminal field", span))
}

pub(super) fn validate_field_reference<'a>(
    module: &'a Module,
    owner: &str,
    name: &str,
    field: &crate::FieldRef,
    span: Span,
) -> Result<&'a Type, Diagnostic> {
    let expected = module
        .field_ref(owner, name)
        .ok_or_else(|| invalid_ir("member field does not resolve in its owner layout", span))?;
    if field != &expected {
        return Err(invalid_ir(
            "resolved member field does not match its owner layout",
            span,
        ));
    }
    let layout = module
        .field_layouts
        .get(owner)
        .ok_or_else(|| invalid_ir("member owner has no field layout", span))?;
    layout
        .fields
        .get(
            usize::try_from(field.slot.value())
                .map_err(|_| invalid_ir("resolved member slot does not fit", span))?,
        )
        .map(|entry| &entry.ty)
        .ok_or_else(|| invalid_ir("resolved member slot is absent", span))
}

pub(super) fn receiver_matches_owner(module: &Module, receiver: &Type, owner: &str) -> bool {
    match receiver {
        Type::ImportedNamed {
            module: imported,
            name,
        } => owner_is_or_derives_from(module, &format!("#{}.{name}", imported.0), owner),
        Type::Named(name) | Type::TypeName(name) => module
            .field_layouts
            .keys()
            .filter(|candidate| {
                candidate.as_str() == name
                    || candidate
                        .strip_suffix(name)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            })
            .any(|candidate| owner_is_or_derives_from(module, candidate, owner)),
        Type::Alternative(options) => options
            .iter()
            .any(|option| receiver_matches_owner(module, option, owner)),
        _ => false,
    }
}

fn owner_is_or_derives_from(module: &Module, receiver_owner: &str, field_owner: &str) -> bool {
    let mut current = receiver_owner;
    loop {
        if current == field_owner {
            return true;
        }
        let Some(base) = module.class_bases.get(current) else {
            return false;
        };
        current = base;
    }
}

fn record_owner_for_type(module: &Module, current_owner: &str, ty: &Type) -> Option<String> {
    match ty {
        Type::ImportedNamed {
            module: imported,
            name,
        } => {
            let owner = format!("#{}.{}", imported.0, name);
            module.field_layouts.contains_key(&owner).then_some(owner)
        }
        Type::Named(name) => {
            if module.field_layouts.contains_key(name) {
                return Some(name.clone());
            }
            let prefix = current_owner
                .rsplit_once('.')
                .map_or("", |(prefix, _)| prefix);
            let owner = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            module.field_layouts.contains_key(&owner).then_some(owner)
        }
        _ => None,
    }
}
