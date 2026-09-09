use super::{HashSet, Instruction, Module, Type, llvm_type};

pub(crate) const OBJECT_HEADER_BYTES: u32 = 8;

pub(crate) fn field_byte_offset(module: &Module, owner: &str, field: &str) -> u32 {
    let mut offset = OBJECT_HEADER_BYTES;
    for (declaring_class, name, ty) in class_layout_fields(module, owner) {
        let (size, alignment) =
            llvm_storage_layout(&ty).expect("target validation accepted member field type");
        offset = align_to(offset, alignment);
        if class_names_match(&declaring_class, owner) && name == field {
            return offset;
        }
        offset = offset.saturating_add(size);
    }
    offset
}

pub(crate) fn class_instance_bytes(module: &Module, type_name: &str) -> u64 {
    let mut total = OBJECT_HEADER_BYTES;
    let mut object_alignment = OBJECT_HEADER_BYTES;
    for (_, _, ty) in class_layout_fields(module, type_name) {
        let (size, alignment) =
            llvm_storage_layout(&ty).expect("target validation accepted member field type");
        total = align_to(total, alignment).saturating_add(size);
        object_alignment = object_alignment.max(alignment);
    }
    u64::from(align_to(total, object_alignment))
}

pub(crate) fn field_type(module: &Module, owner: &str, field: &str) -> Option<Type> {
    class_layout_fields(module, owner)
        .into_iter()
        .find_map(|(declaring_class, name, ty)| {
            (class_names_match(&declaring_class, owner) && name == field).then_some(ty)
        })
}

pub(crate) fn vector_field_offsets(module: &Module, owner: &str) -> Vec<u32> {
    class_layout_fields(module, owner)
        .into_iter()
        .filter_map(|(declaring_class, name, ty)| {
            matches!(ty, Type::Vector { .. })
                .then(|| field_byte_offset(module, &declaring_class, &name))
        })
        .collect()
}

pub(crate) fn is_struct_type(module: &Module, ty: &Type) -> bool {
    let Type::Named(name) = ty else {
        return false;
    };
    let default_function = format!("{name}.$default");
    module
        .functions
        .iter()
        .any(|function| function.name == default_function)
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
        .all(|(_, _, field_ty)| !matches!(field_ty, Type::Vector { .. } | Type::Pointer { .. }))
}

fn class_names_match(left: &str, right: &str) -> bool {
    left == right || left.rsplit('.').next() == right.rsplit('.').next()
}

fn class_layout_fields(module: &Module, class: &str) -> Vec<(String, String, Type)> {
    fn append(
        module: &Module,
        class: &str,
        visiting: &mut HashSet<String>,
        fields: &mut Vec<(String, String, Type)>,
    ) {
        if !visiting.insert(class.to_string()) {
            return;
        }
        if let Some(base) = module.class_bases.get(class) {
            append(module, base, visiting, fields);
        }
        let fields_function = format!("{class}.$fields");
        let default_function = format!("{class}.$default");
        if let Some(function) = module
            .functions
            .iter()
            .find(|function| function.name == fields_function || function.name == default_function)
        {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    if let Instruction::SetMember { name, ty, .. } = instruction {
                        fields.push((class.to_string(), name.clone(), ty.clone()));
                    }
                }
            }
        }
        visiting.remove(class);
    }

    let mut fields = Vec::new();
    append(module, class, &mut HashSet::new(), &mut fields);
    fields
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
