//! LLVM object-layout calculations derived exclusively from validated BN IR
//! metadata, and the typed LLVM forms of the shared BN value layouts.

use bn_ir::FieldRef;

use crate::ir::LlvmType;

#[allow(clippy::wildcard_imports)]
use super::*;

pub(crate) const OBJECT_HEADER_BYTES: u32 = 16;
pub(crate) const I8_BYTES: u32 = 1;
pub(crate) const I16_BYTES: u32 = 2;
pub(crate) const I32_BYTES: u32 = 4;
pub(crate) const POINTER_BYTES: u32 = 8;
pub(crate) const FAT_POINTER_BYTES: u32 = 16;
pub(crate) const REGION_DESCRIPTOR_BYTES: u32 = 24;

pub(crate) const I8_ALIGNMENT: u32 = 1;
pub(crate) const I16_ALIGNMENT: u32 = 2;
pub(crate) const I32_ALIGNMENT: u32 = 4;
pub(crate) const POINTER_ALIGNMENT: u32 = 8;
pub(crate) const AGGREGATE_ALIGNMENT: u32 = 8;

/// Bytes of a STRING header before the payload: count, then length (G7-D2).
pub(crate) const STRING_HEADER_BYTES: i64 = 16;
/// Signed distance from the payload back to the length field.
pub(crate) const STRING_LEN_FROM_PAYLOAD: i64 = (POINTER_BYTES as i64) - STRING_HEADER_BYTES;

pub(crate) const VECTOR_BUFFER_FIELD_INDEX: usize = 0;
pub(crate) const VECTOR_LENGTH_FIELD_INDEX: usize = 1;

#[derive(Clone, Debug)]
pub(crate) struct LayoutField {
    pub reference: FieldRef,
    pub ty: Type,
}

pub(crate) fn field_byte_offset(module: &Module, field: &FieldRef) -> Option<u32> {
    let mut offset = OBJECT_HEADER_BYTES;
    for candidate in class_layout_fields(module, &field.owner) {
        let (size, alignment) = llvm_storage_layout(module, &candidate.ty)
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
    let mut visited = std::collections::HashSet::new();
    class_instance_bytes_inner(module, type_name, &mut visited)
}

fn class_instance_bytes_inner(
    module: &Module,
    type_name: &str,
    visited: &mut std::collections::HashSet<String>,
) -> u64 {
    if !visited.insert(type_name.to_string()) {
        return u64::from(OBJECT_HEADER_BYTES);
    }
    let mut total = OBJECT_HEADER_BYTES;
    let mut object_alignment = OBJECT_HEADER_BYTES;
    for field in class_layout_fields(module, type_name) {
        let (size, alignment) = llvm_storage_layout_inner(module, &field.ty, visited)
            .expect("target validation accepted member field type");
        total = align_to(total, alignment).saturating_add(size);
        object_alignment = object_alignment.max(alignment);
    }
    visited.remove(type_name);
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

/// The layout owner of the STRUCT `ty` names: its own name, or `#<module>.Name`
/// for a STRUCT of an imported module (`Log.Event`); `None` for any other type.
pub(crate) fn struct_owner(module: &Module, ty: &Type) -> Option<String> {
    let owner = match ty {
        Type::Named(name) => name.clone(),
        Type::ImportedNamed {
            module: module_id,
            name,
        } => format!("#{}.{name}", module_id.0),
        _ => return None,
    };
    module
        .function_of_kind(FunctionKind::Default, &owner)
        .is_some()
        .then_some(owner)
}

pub(crate) fn is_struct_type(module: &Module, ty: &Type) -> bool {
    struct_owner(module, ty).is_some()
}

pub(crate) fn struct_copy_supported(module: &Module, ty: &Type) -> bool {
    let Some(name) = struct_owner(module, ty) else {
        return true;
    };
    class_layout_fields(module, &name)
        .into_iter()
        .all(|field| llvm_type(&field.ty).is_some())
}

pub(crate) fn all_struct_owners(module: &Module) -> std::collections::BTreeSet<String> {
    let mut owners = std::collections::BTreeSet::new();
    for name in module.field_layouts.keys() {
        if module
            .function_of_kind(FunctionKind::Default, name)
            .is_some()
        {
            owners.insert(name.clone());
        }
    }
    owners
}

pub(crate) fn struct_managed_fields(module: &Module, owner: &str) -> Vec<LayoutField> {
    let mut visited = std::collections::HashSet::new();
    struct_managed_fields_inner(module, owner, &mut visited)
}

fn struct_managed_fields_inner(
    module: &Module,
    owner: &str,
    visited: &mut std::collections::HashSet<String>,
) -> Vec<LayoutField> {
    if !visited.insert(owner.to_string()) {
        return Vec::new();
    }
    let fields = class_layout_fields(module, owner)
        .into_iter()
        .filter(|field| {
            if matches!(field.ty, Type::Vector { .. }) {
                return true;
            }
            if let Some(nested_owner) = struct_owner(module, &field.ty) {
                return !struct_managed_fields_inner(module, &nested_owner, visited).is_empty();
            }
            false
        })
        .collect();
    visited.remove(owner);
    fields
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

fn llvm_storage_layout(module: &Module, ty: &Type) -> Option<(u32, u32)> {
    let mut visited = std::collections::HashSet::new();
    llvm_storage_layout_inner(module, ty, &mut visited)
}

fn llvm_storage_layout_inner(
    module: &Module,
    ty: &Type,
    visited: &mut std::collections::HashSet<String>,
) -> Option<(u32, u32)> {
    if let Some(owner) = struct_owner(module, ty) {
        let bytes = class_instance_bytes_inner(module, &owner, visited);
        return Some((
            u32::try_from(bytes).unwrap_or_else(|_| {
                panic!("{}", bn_catalog::text("llvm", "msg.struct_size_fits_u32"))
            }),
            POINTER_ALIGNMENT,
        ));
    }
    match llvm_type(ty)? {
        LlvmType::I1 | LlvmType::I8 => Some((I8_BYTES, I8_ALIGNMENT)),
        LlvmType::I16 => Some((I16_BYTES, I16_ALIGNMENT)),
        LlvmType::I32 | LlvmType::Float => Some((I32_BYTES, I32_ALIGNMENT)),
        LlvmType::I64 | LlvmType::Double | LlvmType::Ptr => {
            Some((POINTER_BYTES, POINTER_ALIGNMENT))
        }
        layout
            if is_one_of(
                &layout,
                &[vector_ty(), optional_float_ty(), status_pointer_ty()],
            ) =>
        {
            Some((FAT_POINTER_BYTES, AGGREGATE_ALIGNMENT))
        }
        layout
            if is_one_of(
                &layout,
                &[
                    region_ty(),
                    status_endpoint_ty(),
                    handle_result_ty(),
                    crate::general_alternative::layout(),
                ],
            ) =>
        {
            Some((REGION_DESCRIPTOR_BYTES, AGGREGATE_ALIGNMENT))
        }
        _ => None,
    }
}

/// The payload of a static STRING global: 16 bytes past the header.
#[must_use]
pub(crate) fn string_payload(global: &str) -> crate::ir::LlvmOperand {
    use crate::ir::{LlvmOperand, LlvmType};
    LlvmOperand::ConstGep {
        element: LlvmType::I8,
        base: global.trim_start_matches('@').to_string(),
        indices: vec![(LlvmType::I64, LlvmOperand::int(STRING_HEADER_BYTES))],
    }
}

/// An SSA `getelementptr` of [`string_payload`].
#[must_use]
pub(crate) fn string_payload_address(global: &str) -> crate::ir::LlvmInst {
    use crate::ir::{LlvmInst, LlvmOperand, LlvmType};
    LlvmInst::gep(
        LlvmType::I8,
        LlvmOperand::global(global.trim_start_matches('@')),
        vec![(LlvmType::I64, LlvmOperand::int(STRING_HEADER_BYTES))],
    )
}

/// `{ i1, ptr, i64 }`: a handle or status result (`T OR Error`).
pub(crate) fn handle_result_ty() -> crate::ir::LlvmType {
    use crate::ir::LlvmType::{I1, I64, Ptr};
    LlvmType::struct_of([I1, Ptr, I64])
}

/// `{ ptr, i32 }`: a vector or byte buffer (data, length), and an `Endpoint`.
pub(crate) fn vector_ty() -> crate::ir::LlvmType {
    use crate::ir::LlvmType::{I32, Ptr};
    LlvmType::struct_of([Ptr, I32])
}

/// Whether `ty` is one of `layouts`: an aggregate type cannot be a `match`
/// pattern, so a match on a layout tests it in a guard.
pub(crate) fn is_one_of(ty: &LlvmType, layouts: &[LlvmType]) -> bool {
    layouts.contains(ty)
}

/// `{ i1, ptr }`: `HOST.Net.Addresses OR Error`.
pub(crate) fn status_pointer_ty() -> LlvmType {
    LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr])
}

/// `{ i1, ptr, i32 }`: `HOST.Net.Endpoint OR Error`.
pub(crate) fn status_endpoint_ty() -> LlvmType {
    LlvmType::struct_of([LlvmType::I1, LlvmType::Ptr, LlvmType::I32])
}

/// `{ i1, double }`: a `FLOAT OR NA`-style optional float.
pub(crate) fn optional_float_ty() -> LlvmType {
    LlvmType::struct_of([LlvmType::I1, LlvmType::Double])
}

/// `{ ptr, i32, i32 }`: a `POINTER TO VOID` region.
pub(crate) fn region_ty() -> LlvmType {
    LlvmType::struct_of([LlvmType::Ptr, LlvmType::I32, LlvmType::I32])
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
                            && llvm_type(ty) == Some(LlvmType::Ptr)
                }))
                || (members.contains(&Type::String) && members.contains(&Type::EndOfFile)));
        let status = string_or_error(members)
            || string_na_or_error(members)
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

    pub(crate) fn llvm(self) -> LlvmType {
        match self {
            Self::Sentinel => LlvmType::Ptr,
            Self::Status => handle_result_ty(),
            Self::StatusPointer => status_pointer_ty(),
            Self::StatusEndpoint => status_endpoint_ty(),
            Self::General => crate::general_alternative::layout(),
            Self::Region => vector_ty(),
        }
    }
}

pub(crate) fn llvm_type(ty: &Type) -> Option<LlvmType> {
    match ty {
        Type::Boolean => Some(LlvmType::I1),
        Type::Integer(IntegerType::Byte | IntegerType::Int8) => Some(LlvmType::I8),
        Type::Integer(IntegerType::Int16 | IntegerType::UInt16) => Some(LlvmType::I16),
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) => Some(LlvmType::I32),
        // Untyped integer literals lower as i64 so UINT64/INT64 initializers
        // (including hex source text) stay in range; Store coerces to the slot.
        Type::IntegerLiteral(_) | Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => {
            Some(LlvmType::I64)
        }
        Type::Float(FloatType::Float32) => Some(LlvmType::Float),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some(LlvmType::Double),
        Type::String => Some(LlvmType::Ptr),
        Type::Null => Some(LlvmType::Ptr),
        // `EOF` is the `@.bn_eof` sentinel, the value `INPUT` reads at end of
        // input; `PRINT` writes its text.
        Type::EndOfFile => Some(LlvmType::Ptr),
        Type::Function { .. } => Some(LlvmType::Ptr),
        Type::Named(name) if name == "DATE" || name == "TIME" => Some(LlvmType::I32),
        Type::NotAvailable => Some(optional_float_ty()),
        Type::Alternative(members) => AlternativeLayout::of(members).map(AlternativeLayout::llvm),
        Type::Named(name) if name == "Error" => Some(handle_result_ty()),
        ty if crate::runtime_net::is_net_cidr_type(ty) => Some(handle_result_ty()),
        Type::Named(name) if name == "HOST.Net.Address" || name == "HOST.Net.PingReply" => {
            Some(handle_result_ty())
        }
        Type::Named(name) if name == "HOST.Net.Addresses" => Some(status_pointer_ty()),
        Type::Named(name) if name == "HOST.Net.Endpoint" => Some(vector_ty()),
        // A file handle alone has the layout of `FS.File OR Error`, whose
        // methods read the handle from the payload.
        Type::Named(name) if name == "FS.File" => Some(handle_result_ty()),
        Type::Named(name)
            if matches!(
                name.as_str(),
                "HOST.Net.TCPStream"
                    | "HOST.Net.TCPListener"
                    | "HOST.Net.UDPSocket"
                    | "HOST.Net.UDPPacket"
            ) =>
        {
            Some(handle_result_ty())
        }
        Type::ImportedNamed { name, .. }
            if name == "Address" || name == "PingReply" || name == "Error" =>
        {
            Some(handle_result_ty())
        }
        Type::ImportedNamed { name, .. } if name == "Addresses" => Some(status_pointer_ty()),
        Type::ImportedNamed { name, .. } if name == "Endpoint" => Some(vector_ty()),
        Type::ImportedNamed { name, .. }
            if matches!(
                name.as_str(),
                "TCPStream" | "TCPListener" | "UDPSocket" | "UDPPacket"
            ) =>
        {
            Some(handle_result_ty())
        }
        Type::ImportedNamed { name, .. } if dispatch_handle_name(name) => Some(handle_result_ty()),
        Type::ImportedTypeName { name, .. } if dispatch_handle_name(name) => {
            Some(handle_result_ty())
        }
        Type::ImportedNamed { .. } => Some(LlvmType::Ptr),
        Type::ImportedTypeName { .. } => Some(LlvmType::Ptr),
        Type::Vector {
            element,
            dimensions,
        } if !dimensions.is_empty() && llvm_type(element).is_some() => Some(vector_ty()),
        // Dynamic `NEW T[n]` / `POINTER TO T[]` share the vector fat pointer.
        Type::Pointer { element, .. } if matches!(element.as_ref(), Type::Named(name) if name == "VOID") => {
            Some(region_ty())
        }
        Type::Pointer { element, .. } if llvm_type(element).is_some() => Some(vector_ty()),
        Type::Named(name) if name == "POINTER" => Some(vector_ty()),
        // User class instances (NEW Box(...)) lower as opaque pointers.
        Type::Named(name)
            if !matches!(
                name.as_str(),
                "VOID" | "POINTER" | "DATE" | "TIME" | "Error"
            ) =>
        {
            Some(LlvmType::Ptr)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bn_ir::{FieldId, FieldLayout, FieldLayoutEntry, FieldRef, FieldSlot};
    use bn_source::{Position, Revision, SourceId, Span};

    use super::{Type, class_layout_fields, llvm_type};
    use crate::ir::LlvmType;
    use bn_types::{FloatType, IntegerType};

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

    #[test]
    fn llvm_type_uses_contract_integer_widths() {
        assert_eq!(
            llvm_type(&Type::Integer(IntegerType::Int32)),
            Some(LlvmType::I32)
        );
        assert_eq!(
            llvm_type(&Type::Integer(IntegerType::Byte)),
            Some(LlvmType::I8)
        );
        assert_eq!(
            llvm_type(&Type::Integer(IntegerType::UInt64)),
            Some(LlvmType::I64)
        );
    }

    #[test]
    fn llvm_type_uses_contract_float_widths() {
        assert_eq!(
            llvm_type(&Type::Float(FloatType::Float32)),
            Some(LlvmType::Float)
        );
        assert_eq!(
            llvm_type(&Type::Float(FloatType::Float64)),
            Some(LlvmType::Double)
        );
    }

    #[test]
    fn string_payload_skips_the_header() {
        assert_eq!(
            super::string_payload(".bn_eof").to_string(),
            "getelementptr inbounds (i8, ptr @.bn_eof, i64 16)"
        );
    }
}
