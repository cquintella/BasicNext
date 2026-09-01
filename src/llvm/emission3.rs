#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn emit_constant_assignment(
    text: &mut String,
    destination: ValueId,
    ty: &Type,
    value: &str,
) {
    match llvm_type(ty).expect("validated constant type") {
        "i8" | "i16" | "i32" | "i64" => {
            let _ = writeln!(
                text,
                "  %v{} = add {} 0, {value}",
                destination.0,
                llvm_type(ty).expect("validated integer constant type")
            );
        }
        "float" => {
            let _ = writeln!(text, "  %v{} = fadd float 0.0, {value}", destination.0);
        }
        "double" => {
            let _ = writeln!(text, "  %v{} = fadd double 0.0, {value}", destination.0);
        }
        _ => unreachable!("validated scalar constant type"),
    }
}

pub(crate) fn emit_boolean_assignment(text: &mut String, destination: ValueId, value: bool) {
    let _ = writeln!(
        text,
        "  %v{} = or i1 0, {}",
        destination.0,
        i32::from(value)
    );
}

pub(crate) fn emit_constant_value(
    text: &mut String,
    destination: ValueId,
    ty: &Type,
    value: &ConstantValue,
) {
    match value {
        ConstantValue::Integer(number, _) => {
            emit_constant_assignment(text, destination, ty, &number.to_string());
        }
        ConstantValue::Float(number) => {
            emit_constant_assignment(text, destination, ty, &render_float(*number, ty));
        }
        ConstantValue::Boolean(value) => emit_boolean_assignment(text, destination, *value),
        ConstantValue::String(_) => {
            let _ = writeln!(
                text,
                "  %v{} = getelementptr i8, ptr @.bn_str{}, i64 0",
                destination.0, destination.0
            );
        }
    }
}
