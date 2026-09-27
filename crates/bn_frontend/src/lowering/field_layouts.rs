// Field-layout construction and reference resolution for frontend lowering.
use std::collections::{BTreeMap, HashMap, HashSet};

use super::{
    Diagnostic, FieldId, FieldLayout, FieldLayoutEntry, FieldRef, FieldSlot, Instruction, Module,
    PendingLayout, Span, Type, default_span, ir_error, qualified_class_name,
};

pub(super) fn record_owner(ty: &Type, prefix: &str) -> Option<String> {
    match ty {
        Type::Named(name) => Some(qualified_class_name(prefix, name)),
        Type::ImportedNamed { module, name } => Some(format!("#{}.{}", module.0, name)),
        _ => None,
    }
}

pub(super) fn resolve_member_fields(module: &mut Module) -> Result<(), Diagnostic> {
    let mut references = HashMap::new();
    for (owner, layout) in &module.field_layouts {
        for entry in &layout.fields {
            let index = usize::try_from(entry.id.value())
                .map_err(|_| ir_error("field ID does not fit", entry.span))?;
            let name = module
                .field_names
                .get(index)
                .ok_or_else(|| ir_error("field ID is absent from the name table", entry.span))?;
            references.insert(
                (owner.clone(), name.clone()),
                (
                    FieldRef {
                        owner: owner.clone(),
                        id: entry.id,
                        slot: entry.slot,
                    },
                    entry.ty.clone(),
                ),
            );
        }
    }
    let layouts = &module.field_layouts;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                match instruction {
                    Instruction::Member {
                        field,
                        name,
                        owner,
                        ty,
                        span,
                        ..
                    } if !matches!(ty, Type::Function { .. }) => {
                        let resolved = resolve_member_field(&references, owner, name, *span)?;
                        owner.clone_from(&resolved.owner);
                        *field = Some(resolved);
                    }
                    Instruction::SetMember {
                        field,
                        name,
                        owner,
                        span,
                        ..
                    }
                    | Instruction::SetMemberIndex {
                        field,
                        name,
                        owner,
                        span,
                        ..
                    } => {
                        let resolved = resolve_member_field(&references, owner, name, *span)?;
                        owner.clone_from(&resolved.owner);
                        *field = Some(resolved);
                    }
                    Instruction::SetField {
                        root_owner,
                        path,
                        fields,
                        span,
                        ..
                    }
                    | Instruction::SetFieldIndex {
                        root_owner,
                        path,
                        fields,
                        span,
                        ..
                    } => {
                        *fields = Some(resolve_field_path(
                            layouts,
                            &references,
                            root_owner,
                            path,
                            *span,
                        )?);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn resolve_member_field(
    references: &HashMap<(String, String), (FieldRef, Type)>,
    owner: &str,
    name: &str,
    span: Span,
) -> Result<FieldRef, Diagnostic> {
    if let Some((field, _)) = references.get(&(owner.to_owned(), name.to_owned())) {
        return Ok(field.clone());
    }

    // Imported member targets may carry a source-level owner name without the
    // module qualifier. Recover it only when the complete module graph has one
    // matching layout; ambiguity remains a lowering error.
    let suffix = format!(".{owner}");
    let mut candidates =
        references
            .iter()
            .filter_map(|((candidate_owner, candidate_name), (field, _))| {
                (candidate_name == name
                    && candidate_owner.starts_with('#')
                    && candidate_owner.ends_with(&suffix))
                .then_some(field)
            });
    let Some(field) = candidates.next() else {
        return Err(ir_error(
            format!("unresolved record field access {owner}.{name}"),
            span,
        ));
    };
    if candidates.next().is_some() {
        return Err(ir_error(
            format!("ambiguous imported record field access {owner}.{name}"),
            span,
        ));
    }
    Ok(field.clone())
}

fn resolve_field_path(
    layouts: &BTreeMap<String, FieldLayout>,
    references: &HashMap<(String, String), (FieldRef, Type)>,
    root_owner: &str,
    path: &[String],
    span: Span,
) -> Result<Vec<FieldRef>, Diagnostic> {
    if root_owner.is_empty() || path.is_empty() {
        return Err(ir_error(
            "record field path must have an owner and a field",
            span,
        ));
    }
    let mut owner = root_owner.to_string();
    let mut fields = Vec::with_capacity(path.len());
    for (index, name) in path.iter().enumerate() {
        let (field, ty) = references
            .get(&(owner.clone(), name.clone()))
            .ok_or_else(|| ir_error("unresolved record field path", span))?;
        fields.push(field.clone());
        if index + 1 < path.len() {
            owner = field_owner_for_type(layouts, &owner, ty)
                .ok_or_else(|| ir_error("nested field is not a record type", span))?;
        }
    }
    Ok(fields)
}

fn field_owner_for_type(
    layouts: &BTreeMap<String, FieldLayout>,
    current_owner: &str,
    ty: &Type,
) -> Option<String> {
    match ty {
        Type::ImportedNamed {
            module: imported,
            name,
        } => {
            let owner = format!("#{}.{}", imported.0, name);
            layouts.contains_key(&owner).then_some(owner)
        }
        Type::Named(name) => {
            if layouts.contains_key(name) {
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
            layouts.contains_key(&owner).then_some(owner)
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_field_layout(
    owner: &str,
    pending: &HashMap<String, PendingLayout>,
    class_bases: &HashMap<String, String>,
    names: &mut Vec<String>,
    ids: &mut HashMap<String, FieldId>,
    layouts: &mut BTreeMap<String, FieldLayout>,
    visiting: &mut HashSet<String>,
) -> Result<(), Diagnostic> {
    if layouts.contains_key(owner) {
        return Ok(());
    }
    let declaration = pending
        .get(owner)
        .ok_or_else(|| ir_error("missing record layout declaration", default_span()))?;
    if !visiting.insert(owner.into()) {
        return Err(ir_error(
            "record layout inheritance is cyclic",
            declaration.span,
        ));
    }
    let mut fields = if let Some(base) = class_bases.get(owner) {
        if pending.contains_key(base) {
            lower_field_layout(base, pending, class_bases, names, ids, layouts, visiting)?;
            layouts
                .get(base)
                .map_or_else(Vec::new, |layout| layout.fields.clone())
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    for field in &declaration.fields {
        let id = if let Some(id) = ids.get(&field.name) {
            *id
        } else {
            let raw = u32::try_from(names.len())
                .map_err(|_| ir_error("too many interned field names", field.span))?;
            let id = FieldId::from_raw(raw);
            names.push(field.name.clone());
            ids.insert(field.name.clone(), id);
            id
        };
        let slot = FieldSlot::from_raw(
            u32::try_from(fields.len())
                .map_err(|_| ir_error("record layout has too many fields", field.span))?,
        );
        fields.push(FieldLayoutEntry {
            id,
            slot,
            ty: field.ty.clone(),
            declaring_owner: declaration.owner.clone(),
            weak: field.weak,
            span: field.span,
        });
    }
    visiting.remove(owner);
    layouts.insert(
        owner.into(),
        FieldLayout {
            owner: declaration.owner.clone(),
            fields,
            span: declaration.span,
        },
    );
    Ok(())
}
