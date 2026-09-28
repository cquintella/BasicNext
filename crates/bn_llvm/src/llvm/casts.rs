// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `AS` conversions in native code: which casts `bnc` supports and their
// lowering, including range-checked integer narrowing and `AS STRING` (C3).
#![allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_cast(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source_ty: &Type,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    if *target_ty == Type::String && *source_ty != Type::String {
        emit_to_string(text, destination, value, source_ty);
        return;
    }
    match (llvm_type(source_ty), llvm_type(target_ty)) {
        (Some(source), Some("i1")) => emit_to_boolean(text, destination, value, source),
        (Some(source), Some(target)) if integer_llvm(source) && integer_llvm(target) => {
            emit_integer_to_integer(
                text,
                block_id,
                destination,
                value,
                source,
                target,
                source_ty,
                target_ty,
                state,
            );
        }
        (Some(source), Some(target)) if integer_llvm(source) && float_llvm(target) => {
            let opcode = if is_unsigned(source_ty) {
                "uitofp"
            } else {
                "sitofp"
            };
            let _ = writeln!(
                text,
                "  %v{} = {opcode} {source} %v{} to {target}",
                destination.0, value.0
            );
        }
        (Some(source), Some(target)) if float_llvm(source) && integer_llvm(target) => {
            emit_float_to_integer(
                text,
                block_id,
                destination,
                value,
                source,
                target,
                target_ty,
                state,
            );
        }
        (Some("float"), Some("double")) => {
            let _ = writeln!(
                text,
                "  %v{} = fpext float %v{} to double",
                destination.0, value.0
            );
        }
        (Some("double"), Some("float")) => {
            let _ = writeln!(
                text,
                "  %v{} = fptrunc double %v{} to float",
                destination.0, value.0
            );
        }
        (Some("ptr"), Some("ptr"))
        | (Some("float"), Some("float"))
        | (Some("double"), Some("double")) => {
            emit_same_type_copy(text, destination, value, target_ty);
        }
        _ => unreachable!("validated cast shape"),
    }
}

/// Whether native code implements `source AS target`.
pub(crate) fn cast_supported(source: Option<&Type>, target: &Type) -> bool {
    let Some(source) = source else {
        return false;
    };
    matches!(
        (source, target),
        (
            Type::Integer(_) | Type::IntegerLiteral(_),
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::Boolean
        ) | (
            Type::Float(_) | Type::FloatLiteral,
            Type::Float(_) | Type::Integer(_) | Type::Boolean
        ) | (
            Type::Integer(_)
                | Type::IntegerLiteral(_)
                | Type::Float(_)
                | Type::FloatLiteral
                | Type::Boolean,
            Type::String
        ) | (Type::Boolean, Type::Boolean)
            | (Type::String, Type::Boolean | Type::String)
    )
}

/// A cast to STRING, which calls the `bn_rt` text ABI (C3).
pub(crate) const fn is_text_cast(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::Cast {
            ty: Type::String,
            ..
        }
    )
}

/// `AS STRING` (C3): `bn_rt` formats with the same code the interpreter uses.
fn emit_to_string(text: &mut String, destination: ValueId, value: ValueId, source_ty: &Type) {
    let dest = destination.0;
    let source = llvm_type(source_ty).expect("validated text cast source");
    let (symbol, argument) = match source {
        "i1" => {
            let _ = writeln!(
                text,
                "  %v{dest} = select i1 %v{}, ptr @.bn_true, ptr @.bn_false",
                value.0
            );
            return;
        }
        "i64" => (
            if is_unsigned(source_ty) {
                "bn_rt_text_uint"
            } else {
                "bn_rt_text_int"
            },
            format!("i64 %v{}", value.0),
        ),
        "i8" | "i16" | "i32" => {
            let (symbol, extend) = if is_unsigned(source_ty) {
                ("bn_rt_text_uint", "zext")
            } else {
                ("bn_rt_text_int", "sext")
            };
            let _ = writeln!(
                text,
                "  %textwide{dest} = {extend} {source} %v{} to i64",
                value.0
            );
            (symbol, format!("i64 %textwide{dest}"))
        }
        "float" => {
            let _ = writeln!(
                text,
                "  %textwide{dest} = fpext float %v{} to double",
                value.0
            );
            ("bn_rt_text_float32", format!("double %textwide{dest}"))
        }
        "double" => ("bn_rt_text_float", format!("double %v{}", value.0)),
        _ => unreachable!("validated text cast source"),
    };
    let _ = writeln!(text, "  %v{dest} = call ptr @{symbol}({argument})");
}

fn emit_same_type_copy(text: &mut String, destination: ValueId, value: ValueId, ty: &Type) {
    match llvm_type(ty).expect("validated copy type") {
        "i1" => {
            let _ = writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, value.0);
        }
        "ptr" => {
            let _ = writeln!(
                text,
                "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                destination.0, value.0
            );
        }
        "float" => {
            let _ = writeln!(
                text,
                "  %v{} = fadd float 0.0, %v{}",
                destination.0, value.0
            );
        }
        "double" => {
            let _ = writeln!(
                text,
                "  %v{} = fadd double 0.0, %v{}",
                destination.0, value.0
            );
        }
        other => {
            let _ = writeln!(text, "  %v{} = add {other} 0, %v{}", destination.0, value.0);
        }
    }
}

fn emit_to_boolean(text: &mut String, destination: ValueId, value: ValueId, source: &str) {
    match source {
        "i1" => {
            let _ = writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, value.0);
        }
        "i8" | "i16" | "i32" | "i64" => {
            let _ = writeln!(
                text,
                "  %v{} = icmp ne {source} %v{}, 0",
                destination.0, value.0
            );
        }
        "float" => {
            let _ = writeln!(
                text,
                "  %v{} = fcmp une float %v{}, 0.0",
                destination.0, value.0
            );
        }
        "double" => {
            let _ = writeln!(
                text,
                "  %v{} = fcmp une double %v{}, 0.0",
                destination.0, value.0
            );
        }
        "ptr" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %boolch{dest} = load i8, ptr %v{}", value.0);
            let _ = writeln!(text, "  %v{dest} = icmp ne i8 %boolch{dest}, 0");
        }
        _ => unreachable!("validated boolean cast source"),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_integer_to_integer(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source: &str,
    target: &str,
    source_ty: &Type,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let ext = if is_unsigned(source_ty) {
        "zext"
    } else {
        "sext"
    };
    let _ = writeln!(
        text,
        "  %castw{dest} = {ext} {source} %v{} to i128",
        value.0
    );
    emit_i128_fit_trunc(text, block_id, dest, target, target_ty, state);
}

#[allow(clippy::too_many_arguments)]
fn emit_float_to_integer(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source: &str,
    target: &str,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %castnan{dest} = fcmp uno {source} %v{}, 0.0",
        value.0
    );
    let inf = "0x7FF0000000000000";
    let ninf = "0xFFF0000000000000";
    let _ = writeln!(
        text,
        "  %castpinf{dest} = fcmp oeq {source} %v{}, {inf}",
        value.0
    );
    let _ = writeln!(
        text,
        "  %castninf{dest} = fcmp oeq {source} %v{}, {ninf}",
        value.0
    );
    let _ = writeln!(
        text,
        "  %castinf{dest} = or i1 %castpinf{dest}, %castninf{dest}"
    );
    let _ = writeln!(
        text,
        "  %castbad{dest} = or i1 %castnan{dest}, %castinf{dest}"
    );
    let finite = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &format!("%castbad{dest}"),
        finite,
        bn_diag::DiagId::INVALID_NUMERIC_CONVERSION,
        vec![(
            "message",
            Fact::Text("NAN and infinity cannot convert to an integer".into()),
        )],
    );
    let _ = writeln!(
        text,
        "  %castw{dest} = fptosi {source} %v{} to i128",
        value.0
    );
    emit_i128_fit_trunc(text, block_id, dest, target, target_ty, state);
}

fn emit_i128_fit_trunc(
    text: &mut String,
    block_id: BlockId,
    dest: u32,
    target: &str,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let (min, max) = i128_bounds(target_ty);
    let _ = writeln!(text, "  %castlo{dest} = icmp slt i128 %castw{dest}, {min}");
    let _ = writeln!(text, "  %casthi{dest} = icmp sgt i128 %castw{dest}, {max}");
    let _ = writeln!(text, "  %castov{dest} = or i1 %castlo{dest}, %casthi{dest}");
    let ok = take_continuation(block_id, state);
    emit_overflow_trap(
        text,
        block_id,
        state,
        &format!("%castov{dest}"),
        ok,
        &format!("%castw{dest}"),
        target_ty,
    );
    let _ = writeln!(text, "  %v{dest} = trunc i128 %castw{dest} to {target}");
}

/// The inclusive range of an integer type as `i128` literals.
pub(crate) fn i128_bounds(ty: &Type) -> (&'static str, &'static str) {
    match integer_kind(ty) {
        IntegerType::Byte => ("0", "255"),
        IntegerType::Int8 => ("-128", "127"),
        IntegerType::Int16 => ("-32768", "32767"),
        IntegerType::Int32 => ("-2147483648", "2147483647"),
        IntegerType::Int64 => ("-9223372036854775808", "9223372036854775807"),
        IntegerType::UInt16 => ("0", "65535"),
        IntegerType::UInt32 => ("0", "4294967295"),
        IntegerType::UInt64 => ("0", "18446744073709551615"),
    }
}
