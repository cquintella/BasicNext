//! LLVM object-layout calculations derived exclusively from validated BN IR
//! metadata, and the typed LLVM forms of the shared BN value layouts.

use bn_ir::FieldRef;

#[allow(clippy::wildcard_imports)]
use super::*;

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
        "{ ptr, i32, i32 }" | "{ i1, ptr, i32 }" | "{ i1, ptr, i64 }" | "{ i32, ptr, i64 }" => {
            Some((24, 8))
        }
        _ => None,
    }
}

/// `{ i1, ptr, i64 }`: a handle or status result (`T OR Error`).
pub(crate) fn handle_result_ty() -> crate::ir::LlvmType {
    use crate::ir::LlvmType::{I1, I64, Ptr};
    crate::ir::LlvmType::struct_of([I1, Ptr, I64])
}

/// `{ ptr, i32 }`: a vector or byte buffer (data, length), and an `Endpoint`.
pub(crate) fn vector_ty() -> crate::ir::LlvmType {
    use crate::ir::LlvmType::{I32, Ptr};
    crate::ir::LlvmType::struct_of([Ptr, I32])
}

/// The typed form of a type string `llvm_type` produced; all are canonical.
pub(crate) fn typed_llvm(ty: &str) -> crate::ir::LlvmType {
    crate::ir::LlvmType::parse_canonical(ty).expect("canonical LLVM type")
}

/// How an alternative is represented natively (`value-memory-abi.md`,
/// "Alternative values"). The one place that decides it: `llvm_type`, `IS`,
/// construction and extraction all follow this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlternativeLayout {
    /// `ptr` with a sentinel: `Class OR NULL` (null) and `STRING OR EOF`
    /// (`@.bn_eof`).
    Sentinel,
    /// `{ i1 error, ptr, i64 }`: a `T OR Error` form `bn_rt` returns; `NA` and
    /// `EOF` are sentinel pointers in field 1.
    Status,
    /// `{ i1 error, ptr }`: `HOST.Net.Addresses OR Error`.
    StatusPointer,
    /// `{ i1 error, ptr, i32 }`: `HOST.Net.Endpoint OR Error`.
    StatusEndpoint,
    /// `{ i32 tag, ptr, i64 }`: every other form (`general_alternative.rs`).
    General,
    /// `{ ptr, i32 }`: `POINTER TO T OR NULL`, the region, null for `NULL`.
    Region,
}

impl AlternativeLayout {
    /// The layout of the alternative `members`; `None` when none represents
    /// it (the backend then refuses the form with a support diagnostic).
    pub(crate) fn of(members: &[Type]) -> Option<Self> {
        let sentinel = members.len() == 2
            && ((members.contains(&Type::Null)
                && members.iter().any(|ty| {
                    matches!(ty, Type::Named(name) if name != "Error")
                        || matches!(ty, Type::ImportedNamed { name, .. } if name != "Error")
                            && llvm_type(ty) == Some("ptr")
                }))
                || (members.contains(&Type::String) && members.contains(&Type::EndOfFile)));
        let status = string_na_or_error(members)
            || string_eof_or_error(members)
            || scalar_na_or_error(members)
            || error_or_na(members)
            || float_or_error(members)
            || boolean_or_error(members);
        let late_status = void_or_error(members)
            || integer_or_error(members)
            || integer_eof_or_error(members)
            || imported_or_error(members)
            || opaque_or_error(members);
        let region = members.len() == 2
            && members.contains(&Type::Null)
            && members
                .iter()
                .any(|member| matches!(member, Type::Pointer { .. }));
        Some(if sentinel {
            Self::Sentinel
        } else if status {
            Self::Status
        } else if net_or_error(members) {
            // HOST.Net aggregate results OR Error.
            if members.iter().any(is_net_addresses_type) {
                Self::StatusPointer
            } else if members.iter().any(is_net_endpoint_type) {
                Self::StatusEndpoint
            } else {
                Self::Status
            }
        } else if late_status {
            Self::Status
        } else if general_members(members) {
            Self::General
        } else if region {
            Self::Region
        } else {
            return None;
        })
    }

    pub(crate) const fn llvm(self) -> &'static str {
        match self {
            Self::Sentinel => "ptr",
            Self::Status => "{ i1, ptr, i64 }",
            Self::StatusPointer => "{ i1, ptr }",
            Self::StatusEndpoint => "{ i1, ptr, i32 }",
            Self::General => GENERAL_LAYOUT,
            Self::Region => "{ ptr, i32 }",
        }
    }
}

pub(crate) fn llvm_type(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Boolean => Some("i1"),
        Type::Integer(IntegerType::Byte | IntegerType::Int8) => Some("i8"),
        Type::Integer(IntegerType::Int16 | IntegerType::UInt16) => Some("i16"),
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) => Some("i32"),
        // Untyped integer literals lower as i64 so UINT64/INT64 initializers
        // (including hex source text) stay in range; Store coerces to the slot.
        Type::IntegerLiteral(_) | Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => {
            Some("i64")
        }
        Type::Float(FloatType::Float32) => Some("float"),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some("double"),
        Type::String => Some("ptr"),
        Type::Null => Some("ptr"),
        // `EOF` is the `@.bn_eof` sentinel, the value `INPUT` reads at end of
        // input; `PRINT` writes its text.
        Type::EndOfFile => Some("ptr"),
        Type::Function { .. } => Some("ptr"),
        Type::Named(name) if name == "DATE" || name == "TIME" => Some("i32"),
        Type::NotAvailable => Some("{ i1, double }"),
        Type::Alternative(members) => AlternativeLayout::of(members).map(AlternativeLayout::llvm),
        Type::Named(name) if name == "Error" => Some("{ i1, ptr, i64 }"),
        Type::Named(name) if name == "HOST.Net.Address" || name == "HOST.Net.PingReply" => {
            Some("{ i1, ptr, i64 }")
        }
        Type::Named(name) if name == "HOST.Net.Addresses" => Some("{ i1, ptr }"),
        Type::Named(name) if name == "HOST.Net.Endpoint" => Some("{ ptr, i32 }"),
        // A file handle alone has the layout of `FS.File OR Error`, whose
        // methods read the handle from the payload.
        Type::Named(name) if name == "FS.File" => Some("{ i1, ptr, i64 }"),
        Type::Named(name)
            if matches!(
                name.as_str(),
                "HOST.Net.TCPStream"
                    | "HOST.Net.TCPListener"
                    | "HOST.Net.UDPSocket"
                    | "HOST.Net.UDPPacket"
            ) =>
        {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { name, .. }
            if name == "Address" || name == "PingReply" || name == "Error" =>
        {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { name, .. } if name == "Addresses" => Some("{ i1, ptr }"),
        Type::ImportedNamed { name, .. } if name == "Endpoint" => Some("{ ptr, i32 }"),
        Type::ImportedNamed { name, .. }
            if matches!(
                name.as_str(),
                "TCPStream" | "TCPListener" | "UDPSocket" | "UDPPacket"
            ) =>
        {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { name, .. } if dispatch_handle_name(name) => Some("{ i1, ptr, i64 }"),
        Type::ImportedTypeName { name, .. } if dispatch_handle_name(name) => {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { .. } => Some("ptr"),
        Type::ImportedTypeName { .. } => Some("ptr"),
        Type::Vector {
            element,
            dimensions,
        } if !dimensions.is_empty() && llvm_type(element).is_some() => Some("{ ptr, i32 }"),
        // Dynamic `NEW T[n]` / `POINTER TO T[]` share the vector fat pointer.
        Type::Pointer { element, .. } if matches!(element.as_ref(), Type::Named(name) if name == "VOID") => {
            Some("{ ptr, i32, i32 }")
        }
        Type::Pointer { element, .. } if llvm_type(element).is_some() => Some("{ ptr, i32 }"),
        Type::Named(name) if name == "POINTER" => Some("{ ptr, i32 }"),
        // User class instances (NEW Box(...)) lower as opaque pointers.
        Type::Named(name)
            if !matches!(
                name.as_str(),
                "VOID" | "POINTER" | "DATE" | "TIME" | "Error"
            ) =>
        {
            Some("ptr")
        }
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
