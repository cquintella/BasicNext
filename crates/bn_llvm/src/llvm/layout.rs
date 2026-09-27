//! LLVM object-layout calculations derived exclusively from validated BN IR metadata.

use bn_ir::FieldRef;

use super::{FunctionKind, Module, Type, llvm_type};

pub(crate) const OBJECT_HEADER_BYTES: u32 = 16;

#[derive(Clone, Debug)]
pub(crate) struct LayoutField {
    pub reference: FieldRef,
    pub ty: Type,
}

pub(crate) fn field_byte_offset(module: &Module, field: &FieldRef) -> Option<u32> {
    let mut offset = OBJECT_HEADER_BYTES;
    for candidate in class_layout_fields(module, &field.owner) {
        let (size, alignment) = llvm_storage_layout(&candidate.ty)
            .expect("target validation accepted member field type");
        offset = align_to(offset, alignment);
        if candidate.reference == *field {
            return Some(offset);
        }
        offset = offset.saturating_add(size);
    }
    None
}

pub(crate) fn class_instance_bytes(module: &Module, type_name: &str) -> u64 {
    let mut total = OBJECT_HEADER_BYTES;
    let mut object_alignment = OBJECT_HEADER_BYTES;
    for field in class_layout_fields(module, type_name) {
        let (size, alignment) =
            llvm_storage_layout(&field.ty).expect("target validation accepted member field type");
        total = align_to(total, alignment).saturating_add(size);
        object_alignment = object_alignment.max(alignment);
    }
    u64::from(align_to(total, object_alignment))
}

pub(crate) fn field_type(module: &Module, owner: &str, field: &str) -> Option<Type> {
    module.field_ref(owner, field).and_then(|reference| {
        class_layout_fields(module, owner)
            .into_iter()
            .find_map(|candidate| (candidate.reference == reference).then_some(candidate.ty))
    })
}

pub(crate) fn vector_field_offsets(module: &Module, owner: &str) -> Vec<u32> {
    class_layout_fields(module, owner)
        .into_iter()
        .filter_map(|field| {
            matches!(field.ty, Type::Vector { .. })
                .then(|| field_byte_offset(module, &field.reference))
                .flatten()
        })
        .collect()
}

pub(crate) fn is_struct_type(module: &Module, ty: &Type) -> bool {
    let Type::Named(name) = ty else {
        return false;
    };
    module
        .function_of_kind(FunctionKind::Default, name)
        .is_some()
}

pub(crate) fn struct_copy_supported(module: &Module, ty: &Type) -> bool {
    let Type::Named(name) = ty else {
        return true;
    };
    if !is_struct_type(module, ty) {
        return true;
    }
    class_layout_fields(module, name)
        .into_iter()
        .all(|field| !matches!(field.ty, Type::Vector { .. } | Type::Pointer { .. }))
}

pub(crate) fn class_layout_fields(module: &Module, class: &str) -> Vec<LayoutField> {
    let layout = module.field_layouts.get(class);
    layout.map_or_else(Vec::new, |layout| {
        layout
            .fields
            .iter()
            .map(|entry| LayoutField {
                reference: FieldRef {
                    owner: class.to_string(),
                    id: entry.id,
                    slot: entry.slot,
                },
                ty: entry.ty.clone(),
            })
            .collect()
    })
}

fn align_to(offset: u32, alignment: u32) -> u32 {
    let remainder = offset % alignment;
    if remainder == 0 {
        offset
    } else {
        offset.saturating_add(alignment - remainder)
    }
}

fn llvm_storage_layout(ty: &Type) -> Option<(u32, u32)> {
    match llvm_type(ty)? {
        "i1" | "i8" => Some((1, 1)),
        "i16" => Some((2, 2)),
        "i32" | "float" => Some((4, 4)),
        "i64" | "double" | "ptr" => Some((8, 8)),
        "{ ptr, i32 }" | "{ i1, double }" | "{ i1, ptr }" => Some((16, 8)),
        "{ i1, i32 }" => Some((8, 4)),
        "{ i1, ptr, i32 }" | "{ i1, ptr, i64 }" => Some((24, 8)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bn_ir::{FieldId, FieldLayout, FieldLayoutEntry, FieldRef, FieldSlot};
    use bn_source::{Position, Revision, SourceId, Span};

    use super::{Type, class_layout_fields};

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

    #[test]
    fn class_layout_uses_validated_metadata_without_initializer_instructions() {
        let module = bn_ir::Module {
            field_names: vec!["first".into(), "second".into()],
            field_layouts: BTreeMap::from([(
                "Point".into(),
                FieldLayout {
                    owner: "Point".into(),
                    fields: vec![
                        FieldLayoutEntry {
                            id: FieldId::from_raw(0),
                            slot: FieldSlot::from_raw(0),
                            ty: Type::Integer(bn_types::IntegerType::Int32),
                            declaring_owner: "Point".into(),
                            weak: false,
                            span: span(),
                        },
                        FieldLayoutEntry {
                            id: FieldId::from_raw(1),
                            slot: FieldSlot::from_raw(1),
                            ty: Type::Boolean,
                            declaring_owner: "Point".into(),
                            weak: false,
                            span: span(),
                        },
                    ],
                    span: span(),
                },
            )]),
            ..bn_ir::Module::default()
        };

        let fields = class_layout_fields(&module, "Point");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].reference.slot.value(), 0);
        assert_eq!(fields[0].ty, Type::Integer(bn_types::IntegerType::Int32));
        assert_eq!(fields[1].reference.slot.value(), 1);
        assert_eq!(fields[1].ty, Type::Boolean);
    }

    #[test]
    fn qualified_layout_lookup_never_falls_back_to_a_last_segment() {
        let layout = |owner: &str, id: u32, ty: Type| FieldLayout {
            owner: owner.into(),
            fields: vec![FieldLayoutEntry {
                id: FieldId::from_raw(id),
                slot: FieldSlot::from_raw(0),
                ty,
                declaring_owner: owner.into(),
                weak: false,
                span: span(),
            }],
            span: span(),
        };
        let module = bn_ir::Module {
            field_names: vec!["left".into(), "right".into()],
            field_layouts: BTreeMap::from([
                (
                    "#0.Point".into(),
                    layout("#0.Point", 0, Type::Integer(bn_types::IntegerType::Int32)),
                ),
                ("#1.Point".into(), layout("#1.Point", 1, Type::Boolean)),
            ]),
            ..bn_ir::Module::default()
        };

        assert!(class_layout_fields(&module, "Point").is_empty());
        assert_eq!(
            class_layout_fields(&module, "#0.Point")[0]
                .reference
                .id
                .value(),
            0
        );
        assert_eq!(
            class_layout_fields(&module, "#1.Point")[0]
                .reference
                .id
                .value(),
            1
        );
        assert!(
            super::field_byte_offset(
                &module,
                &FieldRef {
                    owner: "#0.Point".into(),
                    id: FieldId::from_raw(1),
                    slot: FieldSlot::from_raw(0),
                }
            )
            .is_none()
        );
    }
}
