#![allow(clippy::wildcard_imports)]
use super::*;

/// The class whose instance an ARC binding of type `ty` holds: `ty` itself
/// for a class type, or the class member of a general alternative.
pub(crate) fn object_class(module: &Module, ty: &Type) -> Option<Type> {
    if is_class_type(module, ty) {
        return Some(ty.clone());
    }
    general_alternative(ty)?
        .iter()
        .find(|member| is_class_type(module, member))
        .cloned()
}

/// The object pointer a value `operand` of type `ty` holds, for the ARC
/// protocol: the value itself for a class type; for a general alternative,
/// its pointer payload when the tag is `OBJECT`, else null (retain and
/// release ignore null). Registers are named `{name}_…`.
pub(crate) fn held_object(text: &mut String, ty: &Type, operand: &str, name: &str) -> String {
    use crate::ir::{
        ICmpCond, InstSink, LlvmInst, LlvmOperand,
        LlvmType::{I32, Ptr},
    };
    if general_alternative(ty).is_none() {
        return operand.to_owned();
    }
    let layout = crate::layout::typed_llvm(GENERAL_LAYOUT);
    let value = LlvmOperand::raw(operand);
    let reg = |suffix: &str| format!("{name}_{suffix}");
    text.assign(
        reg("tag"),
        LlvmInst::extract(layout.clone(), value.clone(), 0),
    );
    text.assign(
        reg("isobj"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            I32,
            LlvmOperand::reg(reg("tag")),
            LlvmOperand::int(i64::from(bn_types::alternatives::OBJECT)),
        ),
    );
    text.assign(reg("ptr"), LlvmInst::extract(layout, value, 1));
    text.assign(
        name,
        LlvmInst::select(
            LlvmOperand::reg(reg("isobj")),
            Ptr,
            LlvmOperand::reg(reg("ptr")),
            LlvmOperand::null(),
        ),
    );
    format!("%{name}")
}

pub(crate) fn is_class_type(module: &Module, ty: &Type) -> bool {
    let Some(name) = class_name(ty) else {
        return false;
    };
    !is_struct_type(module, ty)
        && module
            .function_of_kind(FunctionKind::FieldInit, &name)
            .is_some()
}

/// A `NEW T[n]` region: counted by the ARC core like a class instance. The
/// fat pointer's data begins `REGION_HEADER_BYTES` past the allocation base,
/// whose header holds the core id at `+8`, as an object's does.
pub(crate) fn is_region_type(ty: &Type) -> bool {
    matches!(ty, Type::Pointer { .. })
}

pub(crate) const REGION_HEADER_BYTES: u64 = 16;

pub(crate) fn class_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Named(name) => Some(name.clone()),
        Type::ImportedNamed { module, name } => Some(format!("#{}.{name}", module.0)),
        Type::Alternative(alternatives)
            if alternatives.len() == 2
                && alternatives.iter().any(|item| matches!(item, Type::Null)) =>
        {
            alternatives.iter().find_map(|item| match item {
                Type::Named(name) => Some(name.clone()),
                Type::ImportedNamed { module, name } => Some(format!("#{}.{name}", module.0)),
                _ => None,
            })
        }
        _ => None,
    }
}
