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
            let llvm_ty = llvm_type(ty).expect("validated integer constant type");
            let rendered = parse_integer(value)
                .map(|number| render_llvm_integer(number, llvm_ty))
                .unwrap_or_else(|| value.to_string());
            let _ = writeln!(text, "  %v{} = add {llvm_ty} 0, {rendered}", destination.0);
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

/// Short-circuit AND/OR reuses one ValueId across blocks. Those values live in
/// `%scN` allocas; never emit a second `%vN` SSA def for them.
pub(crate) fn define_boolean(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    value: bool,
) {
    if analysis.multi_defs.contains(&destination) {
        let _ = writeln!(
            text,
            "  store i1 {}, ptr %sc{}",
            u8::from(value),
            destination.0
        );
    } else {
        emit_boolean_assignment(text, destination, value);
    }
}

pub(crate) fn define_boolean_from(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
    destination: ValueId,
    source: ValueId,
) {
    let operand = i1_operand(text, analysis, state, source);
    if analysis.multi_defs.contains(&destination) {
        let _ = writeln!(text, "  store i1 {operand}, ptr %sc{}", destination.0);
    } else {
        let _ = writeln!(text, "  %v{} = or i1 false, {operand}", destination.0);
    }
}

pub(crate) fn i1_operand(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
    value: ValueId,
) -> String {
    if analysis.multi_defs.contains(&value) {
        let n = state.md_temp;
        state.md_temp += 1;
        let _ = writeln!(text, "  %md{}_{n} = load i1, ptr %sc{}", value.0, value.0);
        format!("%md{}_{n}", value.0)
    } else {
        format!("%v{}", value.0)
    }
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

pub(crate) fn emit_constant_value_analyzed(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    ty: &Type,
    value: &ConstantValue,
) {
    match value {
        ConstantValue::Boolean(flag) => define_boolean(text, analysis, destination, *flag),
        _ => emit_constant_value(text, destination, ty, value),
    }
}
