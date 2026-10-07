#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{BinaryOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::typed_llvm;

fn own(destination: ValueId) -> String {
    format!("v{}", destination.0)
}

pub(crate) fn emit_constant_assignment(
    text: &mut String,
    destination: ValueId,
    ty: &Type,
    value: &str,
) {
    let llvm_ty = llvm_type(ty).expect("validated constant type");
    let (op, zero, rendered) = match llvm_ty {
        "i8" | "i16" | "i32" | "i64" => {
            let rendered = parse_integer(value).map_or_else(
                || value.to_string(),
                |number| render_llvm_integer(number, llvm_ty),
            );
            (BinaryOp::Add, "0", rendered)
        }
        "float" | "double" => (BinaryOp::FAdd, "0.0", value.to_string()),
        _ => unreachable!("validated scalar constant type"),
    };
    let inst = I::binary(op, typed_llvm(llvm_ty), O::raw(zero), O::raw(rendered));
    text.assign(own(destination), inst);
}

pub(crate) fn emit_boolean_assignment(text: &mut String, destination: ValueId, value: bool) {
    let inst = I::binary(BinaryOp::Or, T::I1, O::int(0), O::int(i64::from(value)));
    text.assign(own(destination), inst);
}

/// The `%scN` alloca of a short-circuit value.
fn short_circuit_slot(value: ValueId) -> O {
    O::reg(format!("sc{}", value.0))
}

/// Short-circuit AND/OR reuses one `ValueId` across blocks. Those values live in
/// `%scN` allocas; never emit a second `%vN` SSA def for them.
pub(crate) fn define_boolean(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    value: bool,
) {
    if analysis.multi_defs.contains(&destination) {
        let slot = short_circuit_slot(destination);
        text.emit(I::store(T::I1, O::int(i64::from(value)), slot));
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
    let operand = O::raw(i1_operand(text, analysis, state, source));
    if analysis.multi_defs.contains(&destination) {
        text.emit(I::store(T::I1, operand, short_circuit_slot(destination)));
    } else {
        let copy = I::binary(BinaryOp::Or, T::I1, O::bool(false), operand);
        text.assign(own(destination), copy);
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
        let name = format!("md{}_{n}", value.0);
        text.assign(&name, I::load(T::I1, short_circuit_slot(value)));
        format!("%{name}")
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
            let global = O::global(format!(".bn_str{}", destination.0));
            text.assign(
                own(destination),
                I::gep(T::I8, global, vec![(T::I64, O::int(0))]),
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
